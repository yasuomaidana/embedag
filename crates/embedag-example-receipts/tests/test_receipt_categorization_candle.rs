use embedag_example_receipts::{InvoiceItem, ReceiptCategoryNode, ReceiptGraphPipeline};
use embedag_llm::async_trait;
use embedag_llm::provider::{CandleProvider, LlmError, LlmProvider, ModelMetadata};
use embedag_llm::schema::{ExtractionDirective, ExtractionEnvelope};
use serde_json::Value;

/// Test mock provider encapsulating receipt-specific domain inference strictly within tests.
struct TestReceiptCandleProvider {
    inner: CandleProvider,
}

#[async_trait]
impl LlmProvider for TestReceiptCandleProvider {
    async fn generate_structured(&self, prompt: &str, _schema: &Value) -> Result<Value, LlmError> {
        let directive = if prompt.contains("Horizon Organic Whole Milk") {
            ExtractionDirective::CreateRootNode {
                entity_name: "Dairy".to_string(),
                entity_level: "category".to_string(),
                confidence: 0.99,
            }
        } else if prompt.contains("Fairlife 2% Reduced Fat Milk") {
            ExtractionDirective::InsertNode {
                parent_key: "cat_dairy".to_string(),
                entity_name: "Fairlife 2% Reduced Fat Milk 52oz".to_string(),
                entity_level: "item".to_string(),
                relationship: "CHILD_OF".to_string(),
                confidence: 0.98,
            }
        } else if prompt.contains("Artisan Sourdough Loaf") {
            ExtractionDirective::CreateRootNode {
                entity_name: "Bakery".to_string(),
                entity_level: "category".to_string(),
                confidence: 0.99,
            }
        } else if prompt.contains("Traditional French Baguette") {
            ExtractionDirective::InsertNode {
                parent_key: "cat_bakery".to_string(),
                entity_name: "Traditional French Baguette".to_string(),
                entity_level: "item".to_string(),
                relationship: "CHILD_OF".to_string(),
                confidence: 0.98,
            }
        } else {
            ExtractionDirective::CreateRootNode {
                entity_name: "General".to_string(),
                entity_level: "category".to_string(),
                confidence: 0.9,
            }
        };

        let envelope = ExtractionEnvelope {
            chain_of_thought: "Test simulated reasoning for retail receipts.".to_string(),
            directive,
        };
        Ok(serde_json::to_value(&envelope)?)
    }

    fn model_info(&self) -> ModelMetadata {
        self.inner.model_info()
    }
}

#[tokio::test]
async fn test_candle_dynamic_graph_generation_and_deduplication() {
    let provider = TestReceiptCandleProvider {
        inner: CandleProvider::new("candle-smollm"),
    };
    let mut pipeline = ReceiptGraphPipeline::new(provider);

    // Initial state: DAG is completely empty
    assert!(pipeline.dag.is_empty());
    assert_eq!(pipeline.dag.node_count(), 0);

    // Item 1: Horizon Organic Whole Milk -> Model creates category 'Dairy'
    let item1 = InvoiceItem::new(
        "item_1",
        "Horizon Organic Whole Milk 1 Gallon",
        4.99,
        1,
        None,
    );
    let report1 = pipeline.process_item(&item1).await.expect("process item 1");

    assert_eq!(report1.stage1_active_candidates_count, 0);
    assert!(report1.stage2_schema_has_field_constraints);
    assert!(!report1.stage6_reused_existing_category);
    assert_eq!(pipeline.dag.node_count(), 2); // 1 category + 1 item

    // Item 2: Fairlife 2% Reduced Fat Milk -> Similar item in same category!
    let item2 = InvoiceItem::new("item_2", "Fairlife 2% Reduced Fat Milk 52oz", 3.89, 2, None);
    let report2 = pipeline.process_item(&item2).await.expect("process item 2");

    // Stage 1 saw existing candidate 'cat_dairy (Dairy)'
    assert_eq!(report2.stage1_active_candidates_count, 1);
    // Stage 6 verified that existing category was reused
    assert!(report2.stage6_reused_existing_category);

    // CRITICAL ASSERTION: No additional unnecessary category node was created!
    let categories: Vec<&ReceiptCategoryNode> = pipeline
        .dag
        .inner()
        .node_weights()
        .filter(|n| n.entity_level == "category")
        .collect();
    assert_eq!(
        categories.len(),
        1,
        "Graph must have exactly 1 category node, no duplicate categories"
    );
    assert_eq!(categories[0].category_id, "cat_dairy");
    assert_eq!(categories[0].category_name, "Dairy");

    // Both milk items are children of the single cat_dairy node
    let dairy_children = pipeline.dag.children_of(&"cat_dairy".to_string());
    assert_eq!(
        dairy_children.len(),
        2,
        "Both items must be attached under 'cat_dairy'"
    );

    // Item 3: Artisan Sourdough Loaf -> Model creates category 'Bakery'
    let item3 = InvoiceItem::new("item_3", "Artisan Sourdough Loaf", 4.50, 1, None);
    let report3 = pipeline.process_item(&item3).await.expect("process item 3");
    assert!(!report3.stage6_reused_existing_category);

    // Total categories is now 2
    let categories_after_item3: Vec<&ReceiptCategoryNode> = pipeline
        .dag
        .inner()
        .node_weights()
        .filter(|n| n.entity_level == "category")
        .collect();
    assert_eq!(categories_after_item3.len(), 2);

    // Item 4: Traditional French Baguette -> Similar bakery item!
    let item4 = InvoiceItem::new("item_4", "Traditional French Baguette", 2.75, 1, None);
    let report4 = pipeline.process_item(&item4).await.expect("process item 4");
    assert!(report4.stage6_reused_existing_category);

    // Final deduplication check: 4 items mapped to exactly 2 categories!
    let final_categories: Vec<&ReceiptCategoryNode> = pipeline
        .dag
        .inner()
        .node_weights()
        .filter(|n| n.entity_level == "category")
        .collect();
    assert_eq!(final_categories.len(), 2);

    let bakery_children = pipeline.dag.children_of(&"cat_bakery".to_string());
    assert_eq!(bakery_children.len(), 2);
}
