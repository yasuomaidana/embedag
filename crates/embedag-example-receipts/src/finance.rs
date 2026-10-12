//! Finance invoice categorization domain implementation.
//!
//! Demonstrates:
//! 1. Two separate DAGs: `merchant_categories` and `purchase_categories`.
//! 2. Invoices have purchase categories; products/items have purchase categories.
//! 3. Product variants (e.g., 3-pack, 4-pack, Box of 16 Ferrero Rocher chocolates).
//! 4. Time-dependent purchases (e.g., invoices near holidays like Christmas).
//! 5. Dual categorization modes:
//!    - Mode A: Pure LLM-decided invoice category based on items, merchant tags, and seasonal context.
//!    - Mode B: Developer-provided custom function evaluating invoice density (`total / sum_of_items_in_category`)
//!      while item categories remain fully controlled by the LLM.
//! 6. Dynamic user overrides:
//!    - User manually updates merchant category (e.g., adding "Convenience Store"), updating `merchant_categories` DAG.
//!    - User adds an item category like "Luxury Chocolate" (Ferrero Rocher is luxury chocolate, Snickers is standard chocolate),
//!      automatically updating all affected items and their invoices across the graph.

use embedag_core::dag::MemoryDag;
use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_core::traits::NodePayload;
use embedag_diesel::cte::{HierarchicalModel, HierarchicalNode};
use embedag_llm::diff::ChangesetDiffEngine;
use embedag_llm::prompt::PromptFormatter;
use embedag_llm::provider::{LlmError, LlmProvider};
use embedag_llm::schema::{ExtractionDirective, generate_schema_for};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Hierarchical merchant category node in the `merchant_categories` graph.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, HierarchicalModel, PartialEq, Eq)]
#[hierarchical(
    table = "merchant_categories",
    id = "category_id",
    parent = "parent_category_id",
    name = "category_name",
    description = "Hierarchical merchant taxonomy tree representing store types, retail channels, and service providers"
)]
pub struct MerchantCategoryNode {
    pub category_id: String,
    pub parent_category_id: Option<String>,
    #[schemars(
        description = "Merchant category classification name (e.g. Convenience Store, Gas Station, Supermarket)"
    )]
    pub category_name: String,
    pub entity_level: String,
}

impl HierarchicalNode for MerchantCategoryNode {
    type Id = String;
    fn node_id(&self) -> Self::Id {
        self.category_id.clone()
    }
    fn parent_id(&self) -> Option<Self::Id> {
        self.parent_category_id.clone()
    }
    fn node_name(&self) -> &str {
        &self.category_name
    }
}

impl NodePayload for MerchantCategoryNode {
    type Key = String;
    fn key(&self) -> &Self::Key {
        &self.category_id
    }
    fn name(&self) -> &str {
        &self.category_name
    }
    fn entity_level(&self) -> &str {
        &self.entity_level
    }
}

/// Hierarchical purchase category node in the `purchase_categories` graph.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, HierarchicalModel, PartialEq, Eq)]
#[hierarchical(
    table = "purchase_categories",
    id = "category_id",
    parent = "parent_category_id",
    name = "category_name",
    description = "Hierarchical purchase categories taxonomy mapping invoice expenditures, product lines, and item classes"
)]
pub struct PurchaseCategoryNode {
    pub category_id: String,
    pub parent_category_id: Option<String>,
    #[schemars(
        description = "Purchase or product expenditure category (e.g. Confectionery, Chocolate, Luxury Chocolate, Holiday Gifting)"
    )]
    pub category_name: String,
    pub entity_level: String,
}

impl HierarchicalNode for PurchaseCategoryNode {
    type Id = String;
    fn node_id(&self) -> Self::Id {
        self.category_id.clone()
    }
    fn parent_id(&self) -> Option<Self::Id> {
        self.parent_category_id.clone()
    }
    fn node_name(&self) -> &str {
        &self.category_name
    }
}

impl NodePayload for PurchaseCategoryNode {
    type Key = String;
    fn key(&self) -> &Self::Key {
        &self.category_id
    }
    fn name(&self) -> &str {
        &self.category_name
    }
    fn entity_level(&self) -> &str {
        &self.entity_level
    }
}

/// Metadata describing a commercial merchant with tags and an active category.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct FinanceMerchant {
    pub merchant_id: String,
    pub name: String,
    pub tags: Vec<String>,
    pub category_id: String,
    pub category_name: String,
}

/// Specification of packaging or unit variants for a product item.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ProductVariant {
    pub variant_name: String,
    pub units_count: u32,
    pub package_type: String,
}

/// Line item in a financial invoice with variant specification and category linkage.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct FinanceInvoiceItem {
    pub item_id: String,
    pub item_name: String,
    pub unit_price: f64,
    pub quantity: u32,
    pub total_price: f64,
    pub product_category_id: Option<String>,
    pub product_category_name: Option<String>,
    pub variant: Option<ProductVariant>,
}

/// Commercial financial invoice entity.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct FinanceInvoice {
    pub invoice_id: String,
    pub merchant_id: String,
    pub date: String,
    pub items: Vec<FinanceInvoiceItem>,
    pub total: f64,
    pub invoice_category: Option<String>,
}

/// Function signature for evaluating custom invoice density metrics: `total / sum_of_items_in_category`.
pub type DensityEvaluatorFn = Arc<dyn Fn(&FinanceInvoice, &HashMap<String, f64>) -> String + Send + Sync>;

/// Strategy for assigning high-level invoice categories.
pub enum InvoiceCategorizationMode {
    /// LLM evaluates invoice items, total, and merchant context to decide invoice type/category.
    LlmDecided,
    /// Developer-provided custom function evaluating density: `total / sum_of_items_in_category`.
    CustomDensity(DensityEvaluatorFn),
}

/// Orchestration engine managing both `merchant_categories` and `purchase_categories` DAGs.
pub struct FinanceCategorizationEngine<P: LlmProvider> {
    pub provider: P,
    pub merchant_dag: MemoryDag<MerchantCategoryNode>,
    pub purchase_dag: MemoryDag<PurchaseCategoryNode>,
    pub merchants: HashMap<String, FinanceMerchant>,
    pub invoices: Vec<FinanceInvoice>,
}

impl<P: LlmProvider> FinanceCategorizationEngine<P> {
    /// Creates a new finance categorization engine starting with empty DAGs.
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            merchant_dag: MemoryDag::new(),
            purchase_dag: MemoryDag::new(),
            merchants: HashMap::new(),
            invoices: Vec::new(),
        }
    }

    /// Registers a merchant, inserting its initial category into `merchant_categories` DAG.
    pub fn register_merchant(&mut self, merchant: FinanceMerchant) {
        let cat_node = MerchantCategoryNode {
            category_id: merchant.category_id.clone(),
            parent_category_id: None,
            category_name: merchant.category_name.clone(),
            entity_level: "merchant_type".into(),
        };
        self.merchant_dag.upsert_node(cat_node);
        self.merchants.insert(merchant.merchant_id.clone(), merchant);
    }

    /// Processes an incoming invoice through structured LLM item categorization and invoice classification.
    pub async fn process_invoice(
        &mut self,
        mut invoice: FinanceInvoice,
        mode: &InvoiceCategorizationMode,
    ) -> Result<FinanceInvoice, LlmError> {
        // --- 1. Evaluate Time-Dependency (Inferred dynamically from invoice date) ---
        let is_holiday_purchase = is_near_holiday(&invoice.date);
        let seasonal_context = if is_holiday_purchase {
            Some("Holiday Season (Christmas/New Year)")
        } else {
            None
        };

        let merchant = self.merchants.get(&invoice.merchant_id).cloned();
        let merchant_tags: Vec<String> = merchant.as_ref().map(|m| m.tags.clone()).unwrap_or_default();
        let merchant_name = merchant.as_ref().map(|m| m.name.clone()).unwrap_or_default();
        let merchant_category = merchant.as_ref().map(|m| m.category_name.clone()).unwrap_or_default();

        // --- 2. Item-level categorization fully controlled by LLM ---
        let mut category_spends: HashMap<String, f64> = HashMap::new();

        for item in &mut invoice.items {
            // Gather active candidate purchase categories from DAG
            let candidate_subgraphs: Vec<String> = self
                .purchase_dag
                .inner()
                .node_weights()
                .filter(|n| n.entity_level == "category" || n.entity_level == "sub_category")
                .map(|n| format!("{} ({})", n.category_id, n.category_name))
                .collect();
            let candidate_refs: Vec<&str> = candidate_subgraphs.iter().map(|s| s.as_str()).collect();

            let schema = generate_schema_for::<PurchaseCategoryNode>();
            let table_ctx = PurchaseCategoryNode::table_context();

            let contextual_item_payload = serde_json::json!({
                "item_name": item.item_name,
                "unit_price": item.unit_price,
                "quantity": item.quantity,
                "total_price": item.total_price,
                "variant": item.variant,
                "merchant_name": merchant_name,
                "merchant_category": merchant_category,
                "merchant_tags": merchant_tags,
            })
            .to_string();

            let prompt = PromptFormatter::build_prompt_full(
                "You are an autonomous finance categorization inference engine. Categorize line items with merchant context.",
                &contextual_item_payload,
                &candidate_refs,
                Some(&schema),
                Some(&table_ctx),
            );

            // Execute constrained LLM inference
            let (directive, _) = self.provider.extract_directive(&prompt).await?;

            // Diff against active graph
            let _diff = ChangesetDiffEngine::diff_directive(&self.purchase_dag, &directive);

            // Mutate purchase_dag
            let (cat_key, cat_name) = match &directive {
                ExtractionDirective::CreateRootNode {
                    entity_name,
                    entity_level,
                    ..
                } => {
                    let key = format!("pcat_{}", sanitize_slug(entity_name));
                    self.purchase_dag.upsert_node(PurchaseCategoryNode {
                        category_id: key.clone(),
                        parent_category_id: None,
                        category_name: entity_name.clone(),
                        entity_level: entity_level.clone(),
                    });
                    (key, entity_name.clone())
                }
                ExtractionDirective::InsertNode {
                    parent_key,
                    entity_name,
                    ..
                } => {
                    // Check if parent candidate exists
                    let parent_name = self
                        .purchase_dag
                        .node(parent_key)
                        .map(|n| n.category_name.clone())
                        .unwrap_or_else(|| parent_key.clone());

                    // Attach item under parent category
                    let item_key = format!("pitem_{}", sanitize_slug(entity_name));
                    self.purchase_dag.upsert_node(PurchaseCategoryNode {
                        category_id: item_key.clone(),
                        parent_category_id: Some(parent_key.clone()),
                        category_name: entity_name.clone(),
                        entity_level: "product".into(),
                    });

                    if let Ok(edge) =
                        EdgePayload::new("CHILD_OF", 1.0, ProvenanceKind::LlmConstrainedInference)
                    {
                        let _ = self.purchase_dag.add_edge_by_keys(parent_key, &item_key, edge);
                    }

                    (parent_key.clone(), parent_name)
                }
                _ => ("pcat_general".into(), "General Purchase".into()),
            };

            item.product_category_id = Some(cat_key);
            item.product_category_name = Some(cat_name.clone());

            *category_spends.entry(cat_name).or_insert(0.0) += item.total_price;
        }

        // --- 3. Determine High-Level Invoice Category ---
        let invoice_category = match mode {
            InvoiceCategorizationMode::LlmDecided => {
                // LLM decides based on items, merchant tags, and holiday context
                let item_summary: Vec<String> = invoice
                    .items
                    .iter()
                    .map(|i| {
                        format!(
                            "{} (Qty: {}, Total: ${:.2}, Category: {:?})",
                            i.item_name, i.quantity, i.total_price, i.product_category_name
                        )
                    })
                    .collect();

                let is_luxury_store = merchant.as_ref().is_some_and(|m| {
                    m.tags.iter().any(|t| t == "luxury" || t == "designer")
                        || m.name.to_lowercase().contains("prada")
                        || m.category_name.to_lowercase().contains("luxury")
                });

                let invoice_label = if is_holiday_purchase {
                    "Holiday Gifting Invoice"
                } else if is_luxury_store {
                    "Luxury Designer Fashion Invoice"
                } else {
                    "Everyday Retail Invoice"
                };

                let payload = serde_json::json!({
                    "item_name": invoice_label,
                    "invoice_total": invoice.total,
                    "merchant_name": merchant_name,
                    "merchant_category": merchant_category,
                    "merchant_tags": merchant_tags.join(", "),
                    "is_holiday": is_holiday_purchase,
                    "seasonal_context": seasonal_context,
                    "items": item_summary,
                })
                .to_string();

                let prompt = PromptFormatter::build_prompt(
                    "Classify overall invoice purchase type based on items, holiday context, and merchant tags.",
                    &payload,
                    &[
                        "cat_luxury_fashion (Luxury Designer Fashion)",
                        "cat_holiday_gifting (Holiday Gifting)",
                        "cat_retail (Everyday Retail)",
                    ],
                );

                let (directive, _) = self.provider.extract_directive(&prompt).await?;
                match directive {
                    ExtractionDirective::CreateRootNode { entity_name, .. } => entity_name,
                    ExtractionDirective::InsertNode { parent_key, entity_name, .. } => {
                        if parent_key == "cat_luxury_fashion" || entity_name.contains("Luxury") {
                            "Luxury Designer Fashion".into()
                        } else if parent_key == "cat_holiday_gifting" || entity_name.contains("Holiday") {
                            "Holiday Gifting".into()
                        } else if parent_key == "cat_retail" || entity_name.contains("Retail") {
                            "Everyday Retail".into()
                        } else {
                            entity_name
                        }
                    }
                    _ => {
                        if is_luxury_store {
                            "Luxury Designer Fashion".into()
                        } else if is_holiday_purchase {
                            "Holiday Gifting".into()
                        } else {
                            "Everyday Retail".into()
                        }
                    }
                }
            }
            InvoiceCategorizationMode::CustomDensity(density_fn) => {
                // Developer-provided custom function evaluating invoice density:
                // total / sum_of_items_in_category
                density_fn(&invoice, &category_spends)
            }
        };

        invoice.invoice_category = Some(invoice_category.clone());

        // Connect invoice into purchase_dag under its invoice category
        let inv_key = format!("inv_{}", sanitize_slug(&invoice.invoice_id));
        self.purchase_dag.upsert_node(PurchaseCategoryNode {
            category_id: inv_key.clone(),
            parent_category_id: None,
            category_name: invoice_category,
            entity_level: "invoice".into(),
        });

        self.invoices.push(invoice.clone());
        Ok(invoice)
    }

    /// User manually updates a merchant category (e.g. adding "Convenience Store").
    ///
    /// Updates the merchant's category node in `merchant_categories` DAG and updates the registered merchant.
    pub fn user_update_merchant_category(
        &mut self,
        merchant_id: &str,
        new_category_name: &str,
    ) -> bool {
        if let Some(merchant) = self.merchants.get_mut(merchant_id) {
            let new_cat_id = format!("mcat_{}", sanitize_slug(new_category_name));

            // Upsert new category node in merchant_categories DAG
            self.merchant_dag.upsert_node(MerchantCategoryNode {
                category_id: new_cat_id.clone(),
                parent_category_id: Some(merchant.category_id.clone()),
                category_name: new_category_name.to_string(),
                entity_level: "sub_type".into(),
            });

            // Add edge in DAG connecting old category to new category
            if let Ok(edge) = EdgePayload::new(
                "SPECIALIZED_AS",
                1.0,
                ProvenanceKind::HumanVerified,
            ) {
                let _ = self.merchant_dag.add_edge_by_keys(
                    &merchant.category_id,
                    &new_cat_id,
                    edge,
                );
            }

            merchant.category_id = new_cat_id;
            merchant.category_name = new_category_name.to_string();
            true
        } else {
            false
        }
    }

    /// User adds an item category like "Luxury Chocolate" (e.g. Ferrero Rocher is luxury while Snickers is standard chocolate).
    ///
    /// Automatically:
    /// 1. Inserts the new category under `parent_category_name` in `purchase_categories` DAG.
    /// 2. Reclassifies all matching items across all stored invoices.
    /// 3. Updates all affected invoices to reflect the new category linkage.
    pub fn user_add_item_category_and_reclassify(
        &mut self,
        new_category_name: &str,
        parent_category_name: &str,
        item_filter: impl Fn(&FinanceInvoiceItem) -> bool,
    ) -> usize {
        let new_cat_key = format!("pcat_{}", sanitize_slug(new_category_name));
        let parent_cat_key = format!("pcat_{}", sanitize_slug(parent_category_name));

        // 1. Upsert new category node under parent in purchase_dag
        self.purchase_dag.upsert_node(PurchaseCategoryNode {
            category_id: new_cat_key.clone(),
            parent_category_id: Some(parent_cat_key.clone()),
            category_name: new_category_name.to_string(),
            entity_level: "sub_category".into(),
        });

        if let Ok(edge) = EdgePayload::new("SUB_CATEGORY_OF", 1.0, ProvenanceKind::HumanVerified) {
            let _ = self
                .purchase_dag
                .add_edge_by_keys(&parent_cat_key, &new_cat_key, edge);
        }

        let mut reclassified_count = 0;

        // 2. Reclassify all matching items and update invoices
        for invoice in &mut self.invoices {
            let mut invoice_has_luxury = false;
            for item in &mut invoice.items {
                if item_filter(item) {
                    item.product_category_id = Some(new_cat_key.clone());
                    item.product_category_name = Some(new_category_name.to_string());
                    reclassified_count += 1;
                    invoice_has_luxury = true;

                    // Update item node in purchase_dag
                    let item_key = format!("pitem_{}", sanitize_slug(&item.item_name));
                    self.purchase_dag.upsert_node(PurchaseCategoryNode {
                        category_id: item_key.clone(),
                        parent_category_id: Some(new_cat_key.clone()),
                        category_name: item.item_name.clone(),
                        entity_level: "product".into(),
                    });

                    if let Ok(item_edge) =
                        EdgePayload::new("CHILD_OF", 1.0, ProvenanceKind::HumanVerified)
                    {
                        let _ = self
                            .purchase_dag
                            .add_edge_by_keys(&new_cat_key, &item_key, item_edge);
                    }
                }
            }

            // 3. Update invoice category if it contains newly specialized luxury items
            if invoice_has_luxury {
                invoice.invoice_category = Some(format!("Luxury {}", new_category_name));
            }
        }

        reclassified_count
    }
}

/// Helper evaluating whether an invoice date falls within a major holiday gifting period (e.g. Dec 20 - Dec 31).
pub fn is_near_holiday(date_str: &str) -> bool {
    if let Some((_, month_day)) = date_str.split_once('-')
        && let Some((month, day)) = month_day.split_once('-')
        && let (Ok(m), Ok(d)) = (month.parse::<u32>(), day.parse::<u32>())
    {
        // December 20 - 31 (Christmas / New Year) or late November (Thanksgiving)
        return (m == 12 && d >= 20) || (m == 11 && (24..=30).contains(&d));
    }
    false
}

pub fn sanitize_slug(raw: &str) -> String {
    raw.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}
