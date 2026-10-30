//! The receipt categorization demonstration shared by the runtime examples.

use embedag_example_receipts::{InvoiceItem, ReceiptCategoryNode, ReceiptGraphPipeline};
use embedag_llm::provider::LlmProvider;

/// Runs the receipt categorization demonstration against `provider`, a local runtime named `runtime`.
pub async fn run(
    runtime: &str,
    provider: impl LlmProvider,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== EmbedAG Receipt Categorization Engine ({runtime} Provider) ===");

    let mut pipeline = ReceiptGraphPipeline::new(provider);

    assert!(
        pipeline.dag.is_empty(),
        "Pipeline must start from an empty graph"
    );
    println!("Initial graph state: empty (0 nodes, 0 edges)\n");

    let ticket_items = [
        InvoiceItem::new(
            "item_1",
            "Horizon Organic Whole Milk 1 Gallon",
            4.99,
            1,
            None,
        ),
        InvoiceItem::new("item_2", "Fairlife 2% Reduced Fat Milk 52oz", 3.89, 2, None),
        InvoiceItem::new("item_3", "Artisan Sourdough Loaf", 4.50, 1, None),
        InvoiceItem::new("item_4", "Traditional French Baguette", 2.75, 2, None),
    ];

    for (index, item) in ticket_items.iter().enumerate() {
        println!("-------------------------------------------------------------------------------");
        println!("PROCESSING ITEM #{}: \"{}\"", index + 1, item.item_name);
        println!("-------------------------------------------------------------------------------");

        let report = pipeline.process_item(item).await?;

        println!(
            "* Stage 1 (Candidate Subgraphs Inspection): Active candidates in DAG = {}",
            report.stage1_active_candidates_count
        );
        println!(
            "* Stage 2 (Field Context Annotation): Schema includes field description = {}",
            report.stage2_schema_has_field_constraints
        );
        println!(
            "* Stage 3 (Prompt Generated): Length = {} bytes",
            report.stage3_prompt.len()
        );
        println!(
            "* Stage 4 ({runtime} Inference): Action = {:?}, Directive = {:?}",
            report.stage4_directive, report.stage4_chain_of_thought
        );
        println!(
            "* Stage 5 (Changeset Diffing): New nodes to insert = {}, New edges = {}",
            report.stage5_diff_new_nodes, report.stage5_diff_new_edges
        );
        println!(
            "* Stage 6 (Graph Mutation & Deduplication): Reused existing category = {}",
            report.stage6_reused_existing_category
        );
        println!(
            "* Current DAG State: Total Nodes = {}, Total Edges = {}\n",
            report.total_dag_nodes_after, report.total_dag_edges_after
        );
    }

    println!("=== Final Generated Taxonomy DAG ===");
    let categories: Vec<&ReceiptCategoryNode> = pipeline
        .dag
        .inner()
        .node_weights()
        .filter(|n| n.entity_level == "category")
        .collect();

    for cat in &categories {
        println!(
            "Category Node: [{}] '{}'",
            cat.category_id, cat.category_name
        );
    }

    println!(
        "\nVerification: Items 1 & 2 shared the same category, and items 3 & 4 shared their category."
    );
    println!(
        "Total categories created = {} (No unnecessary duplicate nodes created!)",
        categories.len()
    );

    Ok(())
}
