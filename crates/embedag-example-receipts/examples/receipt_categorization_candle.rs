//! Example: Receipt Categorization using Hugging Face Candle tensor inference engine.
//!
//! Demonstrates:
//! 1. Defining only domain structures and their relationships (`InvoiceItem`, `ReceiptCategoryNode`).
//! 2. Context passed to struct field (`#[schemars(description = "...")]`).
//! 3. Growing the taxonomy graph on-the-fly starting from an empty graph.
//! 4. Constrained LLM inference via `CandleProvider` handling all classification.
//! 5. Similar items ("Whole Milk", "2% Reduced Fat Milk") mapped to the same category ("Dairy")
//!    reusing the existing node to guarantee zero unnecessary duplicate nodes.

use embedag_example_receipts::{InvoiceItem, ReceiptCategoryNode, ReceiptGraphPipeline};
use embedag_llm::provider::CandleProvider;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("===============================================================================");
    println!("STAGE 0: Initialize Hugging Face Candle Tensor Provider and Empty Taxonomy DAG");
    println!("===============================================================================");

    let provider = CandleProvider::new("candle-smollm");
    let mut pipeline = ReceiptGraphPipeline::new(provider);

    assert!(pipeline.dag.is_empty(), "Pipeline must start from an empty graph");
    println!("Initial graph state: empty (0 nodes, 0 edges)\n");

    let ticket_items = [
        InvoiceItem {
            item_name: "Horizon Organic Whole Milk 1 Gallon".into(),
            price: 4.99,
            quantity: 1,
        },
        InvoiceItem {
            item_name: "Fairlife 2% Reduced Fat Milk 52oz".into(),
            price: 3.89,
            quantity: 2,
        },
        InvoiceItem {
            item_name: "Artisan Sourdough Loaf".into(),
            price: 4.50,
            quantity: 1,
        },
        InvoiceItem {
            item_name: "Traditional French Baguette".into(),
            price: 2.75,
            quantity: 2,
        },
    ];

    for (index, item) in ticket_items.iter().enumerate() {
        println!("-------------------------------------------------------------------------------");
        println!("PROCESSING ITEM #{}: \"{}\"", index + 1, item.item_name);
        println!("-------------------------------------------------------------------------------");

        let report = pipeline.process_item(item).await?;

        println!("* Stage 1 (Candidate Subgraphs Inspection): Active candidates in DAG = {}", report.stage1_active_candidates_count);
        println!("* Stage 2 (Field Context Annotation): Schema includes field description = {}", report.stage2_schema_has_field_constraints);
        println!("* Stage 3 (Prompt Generated): Length = {} bytes", report.stage3_prompt.len());
        println!("* Stage 4 (Candle Inference): Action = {:?}, Directive = {:?}", report.stage4_directive, report.stage4_chain_of_thought);
        println!("* Stage 5 (Changeset Diffing): New nodes to insert = {}, New edges = {}", report.stage5_diff_new_nodes, report.stage5_diff_new_edges);
        println!("* Stage 6 (Graph Update & Deduplication): Reused existing category node = {}", report.stage6_reused_existing_category);
        println!("* Current Graph Totals: {} total nodes, {} total edges\n", report.total_dag_nodes_after, report.total_dag_edges_after);
    }

    println!("===============================================================================");
    println!("FINAL TAXONOMY GRAPH TOPOLOGY & DEDUPLICATION VERIFICATION");
    println!("===============================================================================");

    // Verify category deduplication: exactly 2 categories ("Dairy" and "Bakery") for 4 items
    let categories: Vec<&ReceiptCategoryNode> = pipeline
        .dag
        .inner()
        .node_weights()
        .filter(|n| n.entity_level == "category")
        .collect();

    println!("Total Category Nodes in Graph: {}", categories.len());
    for cat in &categories {
        let children = pipeline.dag.children_of(&cat.category_id);
        println!("- Category '{}' (id: '{}') has {} child items:", cat.category_name, cat.category_id, children.len());
        for child in children {
            println!("    * {}", child.category_name);
        }
    }

    assert_eq!(categories.len(), 2, "Must contain exactly 2 categories: Dairy and Bakery");
    let dairy_children = pipeline.dag.children_of(&"cat_dairy".to_string());
    assert_eq!(dairy_children.len(), 2, "Both milk items must be grouped under single 'cat_dairy' node");

    println!("\nMermaid Flowchart:\n{}", pipeline.dag.to_mermaid());
    println!("\nSuccessfully verified on-the-fly graph generation and deduplication with Candle!");
    Ok(())
}
