//! Example domain implementation: Purchase Ticket Invoice Categorization.
//!
//! Demonstrates how an end-user developer:
//! 1. Defines only domain structures (`InvoiceTicket`, `InvoiceItem`, `ReceiptCategoryNode`) and how they are related.
//! 2. Derives `#[derive(HierarchicalModel)]` to automatically generate table configuration, context for LLM, and type-safe CRUD operations.
//! 3. Passes field-level context instructions via `#[schemars(description = "...")]`.
//! 4. Generates the taxonomy graph dynamically on-the-fly starting from an empty graph.
//! 5. Uses real inference models (`OxillamaProvider` and `CandleProvider`) to validate that similar items with the same categories
//!    reuse existing category nodes without creating unnecessary duplicate nodes.

use embedag_core::dag::MemoryDag;
use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_diesel::cte::HierarchicalModel;
use embedag_llm::diff::ChangesetDiffEngine;
use embedag_llm::prompt::PromptFormatter;
use embedag_llm::provider::{LlmError, LlmProvider};
use embedag_llm::schema::{ExtractionDirective, generate_schema_for};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub mod finance;
pub use finance::*;

/// Developer's custom domain model representing a receipt item category hierarchy node `{ parent_category, category }`.
///
/// Automatically implements [`HierarchicalModel`], [`HierarchicalNode`], and [`NodePayload`],
/// providing `table_config()`, `table_context()`, and type-safe CRUD methods (`insert_sqlite`, `insert_batch_sqlite`, etc.)
/// directly to Diesel CTE queries and LLM prompts without hardcoded manual builder boilerplate.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, HierarchicalModel, PartialEq, Eq)]
#[hierarchical(
    table = "receipt_categories",
    id = "category_id",
    parent = "parent_category_id",
    name = "category_name",
    description = "Retail purchase ticket receipt categories hierarchy mapping store departments and product categories"
)]
pub struct ReceiptCategoryNode {
    pub category_id: String,
    pub parent_category_id: Option<String>,
    /// single word category, categories with multiples word but single meaning are allowed too
    #[schemars(
        description = "single word category, categories with multiples word but single meaning are allowed too"
    )]
    pub category_name: String,
    pub entity_level: String,
}

pub type ReceiptCategoryRecord = ReceiptCategoryNode;

/// Telemetry report tracking each stage of the inference and DAG generation pipeline.
#[derive(Debug, Clone)]
pub struct StageExecutionReport {
    pub item_name: String,
    pub stage1_active_candidates_count: usize,
    pub stage2_schema_has_field_constraints: bool,
    pub stage3_table_context_included: bool,
    pub stage3_prompt: String,
    pub stage4_directive: ExtractionDirective,
    pub stage4_chain_of_thought: String,
    pub stage5_diff_new_nodes: usize,
    pub stage5_diff_new_edges: usize,
    pub stage6_reused_existing_category: bool,
    pub total_dag_nodes_after: usize,
    pub total_dag_edges_after: usize,
}

/// Dynamic on-the-fly receipt categorization and graph generation engine.
pub struct ReceiptGraphPipeline<P: LlmProvider> {
    pub provider: P,
    pub dag: MemoryDag<ReceiptCategoryNode>,
}

impl<P: LlmProvider> ReceiptGraphPipeline<P> {
    /// Initializes the pipeline starting with a completely empty graph.
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            dag: MemoryDag::new(),
        }
    }

    /// Executes all processing stages for an incoming invoice item:
    ///
    /// - Stage 1: Empty Graph & Candidate Inspection (assembles candidates from active graph)
    /// - Stage 2: Schema Derivation with Field Instructions Context
    /// - Stage 3: Prompt Synthesis with Active Subgraphs & Macro-derived Table Context
    /// - Stage 4: Constrained LLM Inference (via pure-Rust OxiLLaMa or Candle)
    /// - Stage 5: Changeset Diffing Engine
    /// - Stage 6: Graph Mutation & Deduplication
    pub async fn process_item(
        &mut self,
        item: &InvoiceItem,
    ) -> Result<StageExecutionReport, LlmError> {
        // --- STAGE 1: Graph State Inspection ---
        let candidate_subgraphs: Vec<String> = self
            .dag
            .inner()
            .node_weights()
            .filter(|n| n.entity_level == "category")
            .map(|n| format!("{} ({})", n.category_id, n.category_name))
            .collect();
        let candidate_refs: Vec<&str> = candidate_subgraphs.iter().map(|s| s.as_str()).collect();
        let stage1_active_candidates_count = candidate_refs.len();

        // --- STAGE 2: Schema Derivation with Field Context Constraints ---
        let schema = generate_schema_for::<ReceiptCategoryNode>();
        let schema_has_field_constraints = serde_json::to_string(&schema)
            .unwrap_or_default()
            .contains("single word category");

        // --- STAGE 3: Prompt Formatting with Graph Candidates & Macro Table Context ---
        let item_payload = serde_json::to_string(item).unwrap_or_default();
        let table_ctx = ReceiptCategoryNode::table_context();
        let prompt = PromptFormatter::build_prompt_full(
            "You are an autonomous retail taxonomy inference engine. Categorize incoming receipt items.",
            &item_payload,
            &candidate_refs,
            Some(&schema),
            Some(&table_ctx),
        );

        // --- STAGE 4: Constrained LLM Inference ---
        let (directive, notes) = self.provider.extract_directive(&prompt).await?;

        // --- STAGE 5: Changeset Diffing Engine ---
        let diff = ChangesetDiffEngine::diff_directive(&self.dag, &directive);
        let stage5_diff_new_nodes = diff.new_nodes.len();
        let stage5_diff_new_edges = diff.new_edges.len();

        // --- STAGE 6: Graph Mutation & Deduplication Application ---
        let mut reused_existing_category = false;
        match &directive {
            ExtractionDirective::CreateRootNode {
                entity_name,
                entity_level,
                ..
            } => {
                let cat_key = format!("cat_{}", sanitize_id(entity_name));
                self.dag.upsert_node(ReceiptCategoryNode {
                    category_id: cat_key.clone(),
                    parent_category_id: None,
                    category_name: entity_name.clone(),
                    entity_level: entity_level.clone(),
                });

                // Attach item under newly created category
                let item_key = format!("item_{}", sanitize_id(&item.item_name));
                self.dag.upsert_node(ReceiptCategoryNode {
                    category_id: item_key.clone(),
                    parent_category_id: Some(cat_key.clone()),
                    category_name: item.item_name.clone(),
                    entity_level: "item".into(),
                });
                let edge =
                    EdgePayload::new("CHILD_OF", 1.0, ProvenanceKind::LlmConstrainedInference)
                        .map_err(|e| LlmError::ConstrainedSamplingError(e.to_string()))?;
                let _ = self.dag.add_edge_by_keys(&cat_key, &item_key, edge);
            }

            ExtractionDirective::InsertNode {
                parent_key,
                entity_name,
                ..
            } => {
                reused_existing_category = true;

                // Existing category is reused without creating a duplicate category node!
                let item_key = format!("item_{}", sanitize_id(entity_name));
                self.dag.upsert_node(ReceiptCategoryNode {
                    category_id: item_key.clone(),
                    parent_category_id: Some(parent_key.clone()),
                    category_name: entity_name.clone(),
                    entity_level: "item".into(),
                });

                let edge =
                    EdgePayload::new("CHILD_OF", 1.0, ProvenanceKind::LlmConstrainedInference)
                        .map_err(|e| LlmError::ConstrainedSamplingError(e.to_string()))?;
                let _ = self.dag.add_edge_by_keys(parent_key, &item_key, edge);
            }

            _ => {}
        }

        Ok(StageExecutionReport {
            item_name: item.item_name.clone(),
            stage1_active_candidates_count,
            stage2_schema_has_field_constraints: schema_has_field_constraints,
            stage3_table_context_included: true,
            stage3_prompt: prompt,
            stage4_directive: directive,
            stage4_chain_of_thought: notes.chain_of_thought,
            stage5_diff_new_nodes,
            stage5_diff_new_edges,
            stage6_reused_existing_category: reused_existing_category,
            total_dag_nodes_after: self.dag.node_count(),
            total_dag_edges_after: self.dag.edge_count(),
        })
    }
}

/// Custom SQLite migration for the developer's project schema.
pub const RECEIPT_CATEGORIES_SQLITE_MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS receipt_categories (
    category_id TEXT PRIMARY KEY,
    parent_category_id TEXT REFERENCES receipt_categories(category_id) ON DELETE CASCADE,
    category_name TEXT NOT NULL,
    entity_level TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_receipt_categories_parent ON receipt_categories(parent_category_id);
"#;

/// Custom PostgreSQL migration for the developer's project schema.
pub const RECEIPT_CATEGORIES_POSTGRES_MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS receipt_categories (
    category_id VARCHAR(255) PRIMARY KEY,
    parent_category_id VARCHAR(255) REFERENCES receipt_categories(category_id) ON DELETE CASCADE,
    category_name VARCHAR(255) NOT NULL,
    entity_level VARCHAR(64) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_receipt_categories_parent ON receipt_categories(parent_category_id);
"#;

/// Runs the developer's custom SQLite migration on the given connection.
pub fn run_receipt_categories_sqlite_migration(
    conn: &mut diesel::SqliteConnection,
) -> Result<(), diesel::result::Error> {
    use diesel::connection::SimpleConnection;
    conn.batch_execute(RECEIPT_CATEGORIES_SQLITE_MIGRATION)?;
    Ok(())
}

/// Runs the developer's custom PostgreSQL migration on the given async client.
pub async fn run_receipt_categories_postgres_migration(
    client: &tokio_postgres::Client,
) -> Result<(), tokio_postgres::Error> {
    client
        .batch_execute(RECEIPT_CATEGORIES_POSTGRES_MIGRATION)
        .await?;
    Ok(())
}

/// Infers hierarchical categories `{ parent_category, category }` from a ticket invoice's items.
/// Provided for backwards compatibility with relational persistence tests.
pub fn categorize_ticket_invoice(ticket: &FinanceInvoice) -> Vec<ReceiptCategoryRecord> {
    let mut records = Vec::new();

    let store_root_id = format!("store_{}", sanitize_id(&ticket.merchant_id));
    records.push(ReceiptCategoryRecord {
        category_id: store_root_id.clone(),
        parent_category_id: None,
        category_name: ticket.merchant_id.clone(),
        entity_level: "store_root".into(),
    });

    for item in &ticket.items {
        let (dept_id, dept_name, cat_id, cat_name) = classify_item_heuristics(&item.item_name);

        let full_dept_id = format!("{}_{}", store_root_id, dept_id);
        if !records.iter().any(|r| r.category_id == full_dept_id) {
            records.push(ReceiptCategoryRecord {
                category_id: full_dept_id.clone(),
                parent_category_id: Some(store_root_id.clone()),
                category_name: dept_name.into(),
                entity_level: "department".into(),
            });
        }

        let full_cat_id = format!("{}_{}", full_dept_id, cat_id);
        if !records.iter().any(|r| r.category_id == full_cat_id) {
            records.push(ReceiptCategoryRecord {
                category_id: full_cat_id.clone(),
                parent_category_id: Some(full_dept_id.clone()),
                category_name: cat_name.into(),
                entity_level: "category".into(),
            });
        }

        let item_node_id = format!("{}_{}", full_cat_id, sanitize_id(&item.item_name));
        if !records.iter().any(|r| r.category_id == item_node_id) {
            records.push(ReceiptCategoryRecord {
                category_id: item_node_id,
                parent_category_id: Some(full_cat_id),
                category_name: item.item_name.clone(),
                entity_level: "item".into(),
            });
        }
    }

    records
}

pub fn sanitize_id(raw: &str) -> String {
    raw.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn classify_item_heuristics(
    name: &str,
) -> (&'static str, &'static str, &'static str, &'static str) {
    let lower = name.to_lowercase();
    if lower.contains("milk") || lower.contains("cheese") || lower.contains("yogurt") {
        (
            "dept_dairy",
            "Dairy & Plant Alternatives",
            "cat_milk_products",
            "Milk & Dairy",
        )
    } else if lower.contains("bread") || lower.contains("croissant") || lower.contains("bagel") {
        (
            "dept_bakery",
            "Fresh Bakery",
            "cat_artisan_breads",
            "Breads & Bagels",
        )
    } else if lower.contains("paper") || lower.contains("towel") || lower.contains("soap") {
        (
            "dept_household",
            "Household & Cleaning",
            "cat_paper_goods",
            "Paper & Cleaning",
        )
    } else if lower.contains("apple") || lower.contains("banana") || lower.contains("salad") {
        (
            "dept_produce",
            "Fresh Produce",
            "cat_fruits_veg",
            "Fruits & Vegetables",
        )
    } else {
        (
            "dept_general",
            "General Groceries",
            "cat_packaged",
            "Packaged Goods",
        )
    }
}
