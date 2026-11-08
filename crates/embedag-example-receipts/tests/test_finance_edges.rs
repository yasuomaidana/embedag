//! Edge cases of the finance categorization engine: dates, holidays, rules, user actions and
//! the LLM fallbacks.

use chrono::{DateTime, Utc};
use embedag_example_receipts::ReceiptGraphPipeline;
use embedag_example_receipts::finance::*;
use embedag_llm::async_trait;
use embedag_llm::provider::{CandleProvider, LlmError, LlmProvider, ModelMetadata};
use embedag_llm::schema::ExtractionDirective;
use rstest::rstest;
use serde_json::Value;

fn at(date: &str) -> DateTime<Utc> {
    parse_date(date).expect("valid date")
}

fn engine() -> FinanceCategorizationEngine<CandleProvider> {
    FinanceCategorizationEngine::new(CandleProvider::new("candle"))
}

fn engine_answering(directive: ExtractionDirective) -> FinanceCategorizationEngine<CandleProvider> {
    FinanceCategorizationEngine::new(CandleProvider::with_simulated_output("candle", directive))
}

fn invoice(date: &str, names: &[&str], total: f64) -> FinanceInvoice {
    let items = names
        .iter()
        .enumerate()
        .map(|(i, name)| InvoiceItem::new(format!("item_{i}"), *name, 10.0, 1, None))
        .collect();
    FinanceInvoice::new("inv", "shop", at(date), items, total)
}

fn merchant(id: &str, category_id: &str) -> FinanceMerchant {
    FinanceMerchant {
        merchant_id: id.into(),
        name: format!("{id} store"),
        tags: vec![],
        category_id: category_id.into(),
        category_name: "General".into(),
    }
}

#[rstest]
#[case::rfc3339("2026-05-18T12:00:00Z", true)]
#[case::date_and_time("2026-05-18 10:30:00", true)]
#[case::date_only("2026-05-18", true)]
#[case::garbage("yesterday", false)]
fn dates_are_parsed_in_three_formats(#[case] input: &str, #[case] valid: bool) {
    assert_eq!(parse_date(input).is_some(), valid);
}

#[rstest]
#[case::christmas("2026-12-25", true, true)]
#[case::thanksgiving_week("2026-11-26", true, false)]
#[case::ordinary_weekday("2026-03-04", false, false)]
#[case::weekend("2026-03-07", false, false)]
fn temporal_context_knows_holidays_and_weekends(
    #[case] date: &str,
    #[case] holiday: bool,
    #[case] christmas: bool,
) {
    let context = evaluate_temporal_context(&at(date));
    assert_eq!(context.is_holiday_season, holiday);
    assert_eq!(
        context.holiday_description.is_some(),
        holiday,
        "a holiday season carries a description"
    );
    let described = context.holiday_description.unwrap_or_default();
    assert_eq!(described.contains("Christmas"), christmas);
    assert_eq!(is_near_holiday(&at(date)), holiday);
    assert_eq!(context.is_weekend, date == "2026-03-07");
}

#[rstest]
#[case::white("White Chocolate Bar", "White Chocolate")]
#[case::cookies("Chocolate Cookies and Cream", "Cookies and Cream")]
#[case::ferrero("Ferrero Rocher", "Hazelnut Chocolate")]
#[case::plain("Snickers", "Chocolate Bar")]
#[case::apparel("Cotton T-Shirt", "T-Shirt")]
#[tokio::test]
async fn items_are_categorized_by_their_intrinsic_nature(
    #[case] name: &str,
    #[case] category: &str,
) {
    let mut engine = engine();
    let processed = engine
        .process_invoice(
            invoice("2026-03-04", &[name], 10.0),
            &InvoiceCategorizationMode::LlmDecided,
        )
        .await
        .unwrap();
    assert_eq!(processed.item_categories["item_0"].1, category);
}

#[rstest]
#[case::new_root(
    ExtractionDirective::CreateRootNode {
        entity_name: "Fresh Fruit".into(),
        entity_level: "category".into(),
        confidence: 0.9,
    },
    "Fresh Fruit"
)]
#[case::under_a_parent(
    ExtractionDirective::InsertNode {
        parent_key: "cat_food".into(),
        entity_name: "Bananas".into(),
        entity_level: "item".into(),
        relationship: "CHILD_OF".into(),
        confidence: 0.9,
    },
    "Bananas"
)]
#[case::anything_else(
    ExtractionDirective::MarkAmbiguous { candidate_keys: vec![], reason: "unsure".into() },
    "General Item"
)]
#[tokio::test]
async fn unknown_items_fall_back_to_the_language_model(
    #[case] directive: ExtractionDirective,
    #[case] category: &str,
) {
    let mut engine = engine_answering(directive);
    let processed = engine
        .process_invoice(
            invoice("2026-03-04", &["Mystery Item"], 10.0),
            &InvoiceCategorizationMode::LlmDecided,
        )
        .await
        .unwrap();
    assert_eq!(processed.item_categories["item_0"].1, category);
}

/// A model that always fails.
struct Failing;

#[async_trait]
impl LlmProvider for Failing {
    async fn generate_structured(&self, _p: &str, _s: &Value) -> Result<Value, LlmError> {
        Err(LlmError::ConstrainedSamplingError("offline".into()))
    }

    fn model_info(&self) -> ModelMetadata {
        ModelMetadata {
            provider_name: "failing".into(),
            model_id: "f".into(),
            quantization: "none".into(),
            is_local_zero_ffi: true,
        }
    }
}

#[rstest]
#[case::confirms_a_new_category(
    ExtractionDirective::CreateRootNode {
        entity_name: "Premium".into(),
        entity_level: "category".into(),
        confidence: 0.9,
    },
    true
)]
#[case::nameless_answer(
    ExtractionDirective::CreateRootNode {
        entity_name: String::new(),
        entity_level: "category".into(),
        confidence: 0.9,
    },
    false
)]
#[case::irrelevant_answer(
    ExtractionDirective::MarkAmbiguous { candidate_keys: vec![], reason: "unsure".into() },
    false
)]
#[tokio::test]
async fn the_model_may_add_one_invoice_category(
    #[case] directive: ExtractionDirective,
    #[case] added: bool,
) {
    let mut engine = engine_answering(directive);
    // `Snickers` is classified by rule, so only the invoice-level question reaches the model.
    let processed = engine
        .process_invoice(
            invoice("2026-03-04", &["Snickers"], 10.0),
            &InvoiceCategorizationMode::LlmDecided,
        )
        .await
        .unwrap();
    assert_eq!(
        processed.invoice_categories.len(),
        if added { 2 } else { 1 }
    );
}

#[tokio::test]
async fn a_failing_model_does_not_break_invoice_classification() {
    let mut engine = FinanceCategorizationEngine::new(Failing);
    let processed = engine
        .process_invoice(
            invoice("2026-03-04", &["Snickers"], 10.0),
            &InvoiceCategorizationMode::LlmDecided,
        )
        .await
        .unwrap();
    assert_eq!(
        processed.invoice_categories,
        vec!["Everyday Retail".to_string()]
    );
}

#[test]
fn updating_a_merchant_category_needs_a_known_merchant_and_a_known_parent() {
    let mut engine = engine();
    assert!(!engine.user_update_merchant_category("nobody", "Boutique"));

    engine.register_merchant(merchant("shop", "mcat_general"));
    assert!(engine.user_update_merchant_category("shop", "Boutique"));
    assert_eq!(engine.merchants["shop"].category_name, "Boutique");

    // A merchant whose category was never added to the merchant graph cannot be specialized.
    engine
        .merchants
        .insert("orphan".into(), merchant("orphan", "mcat_missing"));
    assert!(!engine.user_update_merchant_category("orphan", "Boutique"));
}

#[tokio::test]
async fn adding_an_item_category_reclassifies_matching_items() {
    let mut engine = engine();
    engine
        .process_invoice(
            invoice("2026-03-04", &["Ferrero Rocher", "Snickers"], 20.0),
            &InvoiceCategorizationMode::LlmDecided,
        )
        .await
        .unwrap();

    // The parent category does not exist yet: nothing is reclassified.
    let none =
        engine
            .user_add_item_category_and_reclassify("Luxury Chocolate", "No Such Parent", |_| true);
    assert_eq!(none, 0);

    let count =
        engine.user_add_item_category_and_reclassify("Luxury Chocolate", "Chocolate", |item| {
            item.item_name.contains("Ferrero")
        });
    assert_eq!(count, 1);
    // Running it again finds Luxury already attached everywhere.
    let again =
        engine.user_add_item_category_and_reclassify("Luxury Chocolate", "Chocolate", |item| {
            item.item_name.contains("Ferrero")
        });
    assert_eq!(again, 1);
}

#[tokio::test]
async fn the_receipt_pipeline_ignores_directives_it_cannot_apply() {
    let provider = CandleProvider::with_simulated_output(
        "candle",
        ExtractionDirective::MarkAmbiguous {
            candidate_keys: vec![],
            reason: "unsure".into(),
        },
    );
    let mut pipeline = ReceiptGraphPipeline::new(provider);
    let item = InvoiceItem::new("i1", "Mystery", 1.0, 1, None);
    let report = pipeline.process_item(&item).await.unwrap();
    assert_eq!(report.total_dag_nodes_after, 0);
}
