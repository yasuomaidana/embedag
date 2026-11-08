//! Context-dependent categorization: luxury stores, purchase categories per item and the invoice's own categories.

use embedag_llm::prompt::PromptFormatter;
use embedag_llm::provider::LlmProvider;
use embedag_llm::schema::ExtractionDirective;

use super::dto::{FinanceInvoice, FinanceMerchant, InvoiceItem};
use super::engine::FinanceCategorizationEngine;
use super::nodes::InvoiceCategoryNode;
use super::temporal::{TemporalContext, sanitize_slug};

/// A `(category key, category name)` pair.
pub type Category = (String, String);

const LUXURY_CATEGORY_WORDS: [&str; 3] = ["luxury", "designer", "boutique"];
const LUXURY_TAGS: [&str; 3] = ["luxury", "designer", "haute_couture"];

fn mentions_luxury(name: &str, words: &[&str]) -> bool {
    let lower = name.to_lowercase();
    words.iter().any(|word| lower.contains(word))
}

/// Unit price from which any purchase counts as luxury.
const LUXURY_UNIT_PRICE: f64 = 100.0;
/// Invoice total from which the whole invoice counts as luxury.
const LUXURY_INVOICE_TOTAL: f64 = 300.0;

impl<P: LlmProvider> FinanceCategorizationEngine<P> {
    /// Whether the merchant is a luxury store by category text, tags or its category in the merchant DAG.
    pub(crate) fn is_luxury_store(&self, merchant: &FinanceMerchant) -> bool {
        mentions_luxury(&merchant.category_name, &LUXURY_CATEGORY_WORDS)
            || merchant
                .tags
                .iter()
                .any(|t| LUXURY_TAGS.contains(&t.as_str()))
            || self
                .merchant_dag
                .node(&merchant.category_id)
                .is_some_and(|n| mentions_luxury(&n.category_name, &["luxury", "designer"]))
    }

    /// The contextual purchase categories of one item (one or more, context-dependent).
    pub(crate) fn purchase_categories_for(
        &mut self,
        invoice: &FinanceInvoice,
        item: &InvoiceItem,
        is_luxury_store: bool,
        is_holiday_season: bool,
    ) -> Vec<Category> {
        let mut cats = Vec::new();
        if is_luxury_store || item.unit_price >= LUXURY_UNIT_PRICE {
            cats.push(self.upsert_purchase_category("pcat_luxury", "Luxury", None, "category"));
            cats.push(self.upsert_purchase_category(
                "pcat_designer_fashion",
                "Designer Fashion",
                Some("pcat_luxury"),
                "sub_category",
            ));
        } else {
            cats.push(self.upsert_purchase_category(
                "pcat_everyday_retail",
                "Everyday Retail",
                None,
                "category",
            ));
        }
        if is_holiday_season {
            cats.push(self.upsert_purchase_category("pcat_holiday", "Holiday", None, "category"));
            cats.push(self.upsert_purchase_category("pcat_gift", "Gift", None, "category"));
        }
        let purchase_text = invoice
            .purchase_content
            .as_deref()
            .or_else(|| item.variant.as_ref().map(|v| v.variant_name.as_str()))
            .unwrap_or("");
        self.add_birthday_categories(&mut cats, &purchase_text.to_lowercase());
        cats
    }

    /// "my birthday" is a Birthday Celebration; "today is X's birthday" is a Gift and a Birthday Gift.
    fn add_birthday_categories(&mut self, cats: &mut Vec<Category>, purchase_lower: &str) {
        if !purchase_lower.contains("birthday") {
            return;
        }
        if purchase_lower.contains("my birthday") || purchase_lower.contains("myself") {
            let celebration = self.upsert_purchase_category(
                "pcat_birthday",
                "Birthday Celebration",
                None,
                "category",
            );
            cats.push(celebration);
            return;
        }
        let gift = self.upsert_purchase_category("pcat_gift", "Gift", None, "category");
        if !cats.iter().any(|c| c.0 == gift.0) {
            cats.push(gift);
        }
        cats.push(self.upsert_purchase_category(
            "pcat_birthday_gift",
            "Birthday Gift",
            Some("pcat_gift"),
            "sub_category",
        ));
    }

    /// Rule-based invoice categories, plus the one the LLM provider confirms when it adds a new one.
    pub(crate) async fn llm_decided_categories(
        &self,
        invoice: &FinanceInvoice,
        merchant: Option<&FinanceMerchant>,
        temporal: &TemporalContext,
        is_luxury_store: bool,
    ) -> Vec<String> {
        let mut categories =
            rule_based_categories(invoice.total, is_luxury_store, temporal.is_holiday_season);
        let prompt = invoice_prompt(invoice, merchant, temporal);
        if let Some(name) = self.confirmed_category(&prompt).await
            && !categories.contains(&name)
        {
            categories.push(name);
        }
        categories
    }

    async fn confirmed_category(&self, prompt: &str) -> Option<String> {
        match self.provider.extract_directive(prompt).await {
            Ok((
                ExtractionDirective::InsertNode { entity_name, .. }
                | ExtractionDirective::CreateRootNode { entity_name, .. },
                _,
            )) if !entity_name.is_empty() => Some(entity_name),
            _ => None,
        }
    }

    /// Connects the invoice categories into `invoice_dag`.
    pub(crate) fn record_invoice_categories(&mut self, categories: &[String]) {
        for name in categories {
            self.invoice_dag.upsert_node(InvoiceCategoryNode {
                category_id: format!("inv_cat_{}", sanitize_slug(name)),
                parent_category_id: None,
                category_name: name.to_string(),
                entity_level: "category".into(),
            });
        }
    }
}

fn rule_based_categories(
    total: f64,
    is_luxury_store: bool,
    is_holiday_season: bool,
) -> Vec<String> {
    let mut categories = Vec::new();
    if is_luxury_store || total >= LUXURY_INVOICE_TOTAL {
        categories.push("Luxury".to_string());
    }
    if is_holiday_season {
        categories.push("Holiday".to_string());
    }
    if categories.is_empty() {
        categories.push("Everyday Retail".to_string());
    }
    categories
}

/// The schema prompt that asks the provider to classify the whole invoice.
fn invoice_prompt(
    invoice: &FinanceInvoice,
    merchant: Option<&FinanceMerchant>,
    temporal: &TemporalContext,
) -> String {
    let item_summary: Vec<String> = invoice
        .items
        .iter()
        .map(|i| {
            format!(
                "{} (Qty: {}, Unit: ${:.2})",
                i.item_name, i.quantity, i.unit_price
            )
        })
        .collect();
    let payload = serde_json::json!({
        "invoice_id": invoice.invoice_id,
        "total": invoice.total,
        "merchant": merchant.map(|m| m.name.as_str()).unwrap_or_default(),
        "merchant_category": merchant.map(|m| m.category_name.as_str()).unwrap_or_default(),
        "merchant_tags": merchant.map(|m| m.tags.clone()).unwrap_or_default(),
        "is_holiday": temporal.is_holiday_season,
        "items": item_summary,
    })
    .to_string();
    PromptFormatter::build_prompt(
        "Classify overall invoice purchase type based on items, holiday context, and merchant tags.",
        &payload,
        &[
            "cat_luxury (Luxury)",
            "cat_holiday (Holiday)",
            "cat_everyday_retail (Everyday Retail)",
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::luxury_total(300.0, false, false, vec!["Luxury"])]
    #[case::luxury_store(10.0, true, false, vec!["Luxury"])]
    #[case::holiday(10.0, false, true, vec!["Holiday"])]
    #[case::both(500.0, false, true, vec!["Luxury", "Holiday"])]
    #[case::neither(10.0, false, false, vec!["Everyday Retail"])]
    fn rule_based(
        #[case] total: f64,
        #[case] luxury: bool,
        #[case] holiday: bool,
        #[case] expected: Vec<&str>,
    ) {
        assert_eq!(rule_based_categories(total, luxury, holiday), expected);
    }

    #[rstest]
    #[case::category_word("Designer Outlet", true)]
    #[case::boutique("Corner BOUTIQUE", true)]
    #[case::plain("Grocery", false)]
    fn luxury_words(#[case] name: &str, #[case] expected: bool) {
        assert_eq!(mentions_luxury(name, &LUXURY_CATEGORY_WORDS), expected);
    }
}
