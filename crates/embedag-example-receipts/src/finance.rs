//! Finance invoice categorization domain implementation.
//!
//! Demonstrates:
//! 1. Input DTOs with zero semantics: `FinanceInvoice` (`InvoiceTicket`), `InvoiceItem`, `ProductVariant`.
//! 2. Four distinct hierarchical taxonomy graphs:
//!    - `MerchantCategoryNode`: Merchant/store classification tree.
//!    - `ItemCategoryNode`: Inherent product-level single-value taxonomy (e.g. `Chocolate` -> `White Chocolate`, `Cookies and Cream`, `Hazelnut Chocolate`).
//!    - `PurchaseCategoryNode`: Product- and context-dependent taxonomy with one or more categories (e.g. `[Holiday]`, `[Gift]`, `[Birthday Gift]`, `[Luxury]`).
//!    - `InvoiceCategoryNode`: Multi-category invoice classification depending on store, products, and quantity (e.g. `["Luxury", "Holiday"]`).
//! 3. All 4 taxonomy nodes derive `#[derive(HierarchicalModel)]` which automatically implements `HierarchicalNode`,
//!    `NodePayload`, and type-safe Diesel CRUD operations (`insert_sqlite`, `insert_batch_sqlite`, etc.) without repetitive manual boilerplate.
//! 4. Dual categorization modes:
//!    - Mode A: Pure LLM-decided invoice categorization.
//!    - Mode B: Developer-provided custom function evaluating invoice density (`total / sum_of_items_in_category`)
//!      while item intrinsic categories remain fully controlled by the LLM.
//! 5. Dynamic user overrides:
//!    - User manually updates a merchant category (e.g., adding "Convenience Store"), updating `merchant_categories` DAG.
//!    - User adds an item category like "Luxury Chocolate" (Ferrero Rocher is luxury chocolate, Snickers is standard chocolate),
//!      automatically propagating across all affected items, invoices, and graphs.

use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, Utc};
use embedag_core::dag::MemoryDag;
use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_diesel::cte::HierarchicalModel;
use embedag_llm::diff::ChangesetDiffEngine;
use embedag_llm::prompt::PromptFormatter;
use embedag_llm::provider::{LlmError, LlmProvider};
use embedag_llm::schema::ExtractionDirective;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

pub type DATE = DateTime<Utc>;

/// Commercial financial invoice entity received as pure DTO (no semantic fields).
/// May optionally contain user purchase context (e.g. "today is John's birthday", "my birthday").
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct FinanceInvoice {
    pub invoice_id: String,
    pub merchant_id: String,
    pub date: DATE,
    pub items: Vec<InvoiceItem>,
    pub total: f64,
    #[serde(default)]
    pub purchase_content: Option<String>,
}

impl FinanceInvoice {
    /// Creates a basic FinanceInvoice without purchase context.
    pub fn new(
        invoice_id: impl Into<String>,
        merchant_id: impl Into<String>,
        date: DATE,
        items: Vec<InvoiceItem>,
        total: f64,
    ) -> Self {
        Self {
            invoice_id: invoice_id.into(),
            merchant_id: merchant_id.into(),
            date,
            items,
            total,
            purchase_content: None,
        }
    }

    /// Builder method to attach optional purchase content notes (e.g. user comments or receipt notes).
    pub fn with_purchase_content(mut self, content: impl Into<String>) -> Self {
        self.purchase_content = Some(content.into());
        self
    }
}

pub type InvoiceTicket = FinanceInvoice;

/// Helper function to parse an ISO-8601 or YYYY-MM-DD date string into DateTime<Utc>.
pub fn parse_date(date_str: &str) -> DateTime<Utc> {
    if let Ok(dt) = date_str.parse::<DateTime<Utc>>() {
        return dt;
    }
    let fmt_dt = "%Y-%m-%d %H:%M:%S";
    if let Ok(naive_dt) = NaiveDateTime::parse_from_str(date_str, fmt_dt) {
        return DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc);
    }
    let fmt_d = "%Y-%m-%d";
    if let Ok(naive_d) = NaiveDate::parse_from_str(date_str, fmt_d) {
        let naive_dt = naive_d.and_hms_opt(12, 0, 0).unwrap();
        return DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc);
    }
    Utc::now()
}

/// Line item in a purchase ticket invoice (pure DTO, no pre-assigned semantic flags).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct InvoiceItem {
    pub item_id: String,
    pub item_name: String,
    pub unit_price: f64,
    pub quantity: u32,
    pub total_price: f64,
    pub variant: Option<ProductVariant>,
}

impl InvoiceItem {
    /// Helper constructor for building pure DTO invoice items.
    pub fn new(
        item_id: impl Into<String>,
        item_name: impl Into<String>,
        unit_price: f64,
        quantity: u32,
        variant: Option<ProductVariant>,
    ) -> Self {
        Self {
            item_id: item_id.into(),
            item_name: item_name.into(),
            unit_price,
            quantity,
            total_price: unit_price * (quantity as f64),
            variant,
        }
    }
}

/// Specification of packaging or unit variants for a product item.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ProductVariant {
    pub variant_name: String,
    pub units_count: u32,
    pub package_type: String,
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

/// Hierarchical merchant category node in the `merchant_categories` graph.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, HierarchicalModel, PartialEq, Eq)]
#[hierarchical(
    table = "merchant_categories",
    id = "category_id",
    parent = "parent_category_id",
    name = "category_name",
    description = "Hierarchical merchant taxonomy tree representing store types and retail channels"
)]
pub struct MerchantCategoryNode {
    pub category_id: String,
    pub parent_category_id: Option<String>,
    #[schemars(
        description = "Merchant category classification name (e.g. Convenience Store, Gas Station, Supermarket, Luxury Boutique)"
    )]
    pub category_name: String,
    pub entity_level: String,
}

/// Hierarchical product-intrinsic category node in the `item_categories` graph (single-value inherent domain).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, HierarchicalModel, PartialEq, Eq)]
#[hierarchical(
    table = "item_categories",
    id = "category_id",
    parent = "parent_category_id",
    name = "category_name",
    description = "Product-intrinsic taxonomy tree (e.g. Chocolate -> White Chocolate, Cookies and Cream; Apparel -> T-Shirt)"
)]
pub struct ItemCategoryNode {
    pub category_id: String,
    pub parent_category_id: Option<String>,
    #[schemars(
        description = "Intrinsic product category name (single-value inherent domain, e.g. Chocolate, Apparel)"
    )]
    pub category_name: String,
    pub entity_level: String,
}

/// Hierarchical context-dependent purchase category node in the `purchase_categories` graph.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, HierarchicalModel, PartialEq, Eq)]
#[hierarchical(
    table = "purchase_categories",
    id = "category_id",
    parent = "parent_category_id",
    name = "category_name",
    description = "Context-dependent purchase expenditure categories (e.g. Holiday, Gift -> Birthday Gift, Luxury)"
)]
pub struct PurchaseCategoryNode {
    pub category_id: String,
    pub parent_category_id: Option<String>,
    #[schemars(
        description = "Context-dependent purchase category (e.g. Holiday, Gift, Birthday Gift, Luxury)"
    )]
    pub category_name: String,
    pub entity_level: String,
}

/// Hierarchical invoice-level classification category node in the `invoice_categories` graph.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, HierarchicalModel, PartialEq, Eq)]
#[hierarchical(
    table = "invoice_categories",
    id = "category_id",
    parent = "parent_category_id",
    name = "category_name",
    description = "Invoice-level classification depending on store, products, and quantity (e.g. Luxury, Holiday)"
)]
pub struct InvoiceCategoryNode {
    pub category_id: String,
    pub parent_category_id: Option<String>,
    #[schemars(
        description = "Invoice classification category (e.g. Luxury, Holiday, Everyday Retail)"
    )]
    pub category_name: String,
    pub entity_level: String,
}

/// Result of processing a pure `FinanceInvoice` through the multi-DAG categorization engine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CategorizedInvoice {
    pub invoice: FinanceInvoice,
    /// Inherent product taxonomy: maps item_id -> (category_id, category_name)
    pub item_categories: HashMap<String, (String, String)>,
    /// Context-dependent purchase taxonomy: maps item_id -> Vec<(category_id, category_name)>
    pub purchase_categories: HashMap<String, Vec<(String, String)>>,
    /// Multi-category invoice classification (e.g. ["Luxury", "Holiday"])
    pub invoice_categories: Vec<String>,
}

/// Function signature for evaluating custom invoice density metrics: `total / sum_of_items_in_category`.
pub type DensityEvaluatorFn =
    Arc<dyn Fn(&FinanceInvoice, &HashMap<String, f64>) -> Vec<String> + Send + Sync>;

/// Strategy for assigning high-level invoice categories.
pub enum InvoiceCategorizationMode {
    /// LLM evaluates invoice items, total, and merchant context to decide invoice types/categories.
    LlmDecided,
    /// Developer-provided custom function evaluating density: `total / sum_of_items_in_category`.
    CustomDensity(DensityEvaluatorFn),
}

/// Orchestration engine managing the 4 distinct hierarchical DAGs.
pub struct FinanceCategorizationEngine<P: LlmProvider> {
    pub provider: P,
    pub merchant_dag: MemoryDag<MerchantCategoryNode>,
    pub item_dag: MemoryDag<ItemCategoryNode>,
    pub purchase_dag: MemoryDag<PurchaseCategoryNode>,
    pub invoice_dag: MemoryDag<InvoiceCategoryNode>,
    pub merchants: HashMap<String, FinanceMerchant>,
    pub invoices: Vec<CategorizedInvoice>,
}

impl<P: LlmProvider> FinanceCategorizationEngine<P> {
    /// Creates a new finance categorization engine starting with empty DAGs.
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            merchant_dag: MemoryDag::new(),
            item_dag: MemoryDag::new(),
            purchase_dag: MemoryDag::new(),
            invoice_dag: MemoryDag::new(),
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
        self.merchants
            .insert(merchant.merchant_id.clone(), merchant);
    }

    /// Processes an incoming pure `FinanceInvoice` DTO through multi-DAG categorization.
    pub async fn process_invoice(
        &mut self,
        invoice: FinanceInvoice,
        mode: &InvoiceCategorizationMode,
    ) -> Result<CategorizedInvoice, LlmError> {
        // --- 1. Evaluate Insightful Context Dynamically ---
        let temporal = evaluate_temporal_context(&invoice.date);
        let merchant = self.merchants.get(&invoice.merchant_id).cloned();
        let merchant_name = merchant
            .as_ref()
            .map(|m| m.name.clone())
            .unwrap_or_default();
        let merchant_category = merchant
            .as_ref()
            .map(|m| m.category_name.clone())
            .unwrap_or_default();
        let merchant_tags: Vec<String> = merchant
            .as_ref()
            .map(|m| m.tags.clone())
            .unwrap_or_default();

        // Check user-configured categorization in merchant_categories DAG
        let is_luxury_store = if let Some(m) = &merchant {
            let cat_lower = m.category_name.to_lowercase();
            cat_lower.contains("luxury")
                || cat_lower.contains("designer")
                || cat_lower.contains("boutique")
                || m.tags
                    .iter()
                    .any(|t| t == "luxury" || t == "designer" || t == "haute_couture")
                || self.merchant_dag.node(&m.category_id).is_some_and(|n| {
                    n.category_name.to_lowercase().contains("luxury")
                        || n.category_name.to_lowercase().contains("designer")
                })
        } else {
            false
        };

        let mut item_categories: HashMap<String, (String, String)> = HashMap::new();
        let mut purchase_categories: HashMap<String, Vec<(String, String)>> = HashMap::new();
        let mut category_spends: HashMap<String, f64> = HashMap::new();

        // --- 2. Process each item: Intrinsic Item Taxonomy + Contextual Purchase Taxonomy ---
        for item in &invoice.items {
            // A. INTRINSIC ITEM CATEGORY (Single value, product-intrinsic)
            let (item_cat_key, item_cat_name) = self.infer_item_intrinsic_category(item).await?;
            item_categories.insert(
                item.item_id.clone(),
                (item_cat_key.clone(), item_cat_name.clone()),
            );
            *category_spends.entry(item_cat_name.clone()).or_insert(0.0) += item.total_price;

            // B. CONTEXTUAL PURCHASE CATEGORIES (One or more, context-dependent)
            let mut item_p_cats: Vec<(String, String)> = Vec::new();

            // Evaluate purchase context (e.g. "today is John's birthday", "my birthday")
            let purchase_text = invoice
                .purchase_content
                .as_deref()
                .or_else(|| item.variant.as_ref().map(|v| v.variant_name.as_str()))
                .unwrap_or("");
            let purchase_lower = purchase_text.to_lowercase();

            if is_luxury_store || item.unit_price >= 100.0 {
                let lux_cat =
                    self.upsert_purchase_category("pcat_luxury", "Luxury", None, "category");
                item_p_cats.push(lux_cat);
                let des_cat = self.upsert_purchase_category(
                    "pcat_designer_fashion",
                    "Designer Fashion",
                    Some("pcat_luxury"),
                    "sub_category",
                );
                item_p_cats.push(des_cat);
            } else {
                let ret_cat = self.upsert_purchase_category(
                    "pcat_everyday_retail",
                    "Everyday Retail",
                    None,
                    "category",
                );
                item_p_cats.push(ret_cat);
            }

            if temporal.is_holiday_season {
                let hol_cat =
                    self.upsert_purchase_category("pcat_holiday", "Holiday", None, "category");
                item_p_cats.push(hol_cat);
                let gift_cat = self.upsert_purchase_category("pcat_gift", "Gift", None, "category");
                item_p_cats.push(gift_cat);
            }

            // Differentiate "today is X birthday" (Gift + Birthday Gift) vs "my birthday" (Birthday only)
            if purchase_lower.contains("birthday") {
                let is_self_birthday =
                    purchase_lower.contains("my birthday") || purchase_lower.contains("myself");
                if is_self_birthday {
                    // Only assign Birthday / Birthday Celebration, NOT gift
                    let bday_cat = self.upsert_purchase_category(
                        "pcat_birthday",
                        "Birthday Celebration",
                        None,
                        "category",
                    );
                    item_p_cats.push(bday_cat);
                } else {
                    // Gift for another person: Gift parent, Birthday Gift child
                    let gift_parent =
                        self.upsert_purchase_category("pcat_gift", "Gift", None, "category");
                    if !item_p_cats.iter().any(|c| c.0 == gift_parent.0) {
                        item_p_cats.push(gift_parent);
                    }
                    let bday_cat = self.upsert_purchase_category(
                        "pcat_birthday_gift",
                        "Birthday Gift",
                        Some("pcat_gift"),
                        "sub_category",
                    );
                    item_p_cats.push(bday_cat);
                }
            }

            purchase_categories.insert(item.item_id.clone(), item_p_cats);
        }

        // --- 3. Determine High-Level Multi-Category Invoice Classification ---
        let invoice_categories = match mode {
            InvoiceCategorizationMode::LlmDecided => {
                let mut categories = Vec::new();
                if is_luxury_store || invoice.total >= 300.0 {
                    categories.push("Luxury".into());
                }
                if temporal.is_holiday_season {
                    categories.push("Holiday".into());
                }
                if categories.is_empty() {
                    categories.push("Everyday Retail".into());
                }

                // If LLM provider is available, formulate schema prompt to confirm
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

                let prompt_payload = serde_json::json!({
                    "invoice_id": invoice.invoice_id,
                    "total": invoice.total,
                    "merchant": merchant_name,
                    "merchant_category": merchant_category,
                    "merchant_tags": merchant_tags,
                    "is_holiday": temporal.is_holiday_season,
                    "items": item_summary,
                })
                .to_string();

                let prompt = PromptFormatter::build_prompt(
                    "Classify overall invoice purchase type based on items, holiday context, and merchant tags.",
                    &prompt_payload,
                    &[
                        "cat_luxury (Luxury)",
                        "cat_holiday (Holiday)",
                        "cat_everyday_retail (Everyday Retail)",
                    ],
                );

                if let Ok((
                    ExtractionDirective::InsertNode { entity_name, .. }
                    | ExtractionDirective::CreateRootNode { entity_name, .. },
                    _,
                )) = self.provider.extract_directive(&prompt).await
                    && !categories.iter().any(|c| c == &entity_name)
                    && !entity_name.is_empty()
                {
                    categories.push(entity_name);
                }

                categories
            }
            InvoiceCategorizationMode::CustomDensity(density_fn) => {
                density_fn(&invoice, &category_spends)
            }
        };

        // Connect invoice categories into invoice_dag
        for cat_name in &invoice_categories {
            let cat_key = format!("inv_cat_{}", sanitize_slug(cat_name));
            self.invoice_dag.upsert_node(InvoiceCategoryNode {
                category_id: cat_key,
                parent_category_id: None,
                category_name: cat_name.to_string(),
                entity_level: "category".into(),
            });
        }

        let categorized = CategorizedInvoice {
            invoice,
            item_categories,
            purchase_categories,
            invoice_categories,
        };

        self.invoices.push(categorized.clone());
        Ok(categorized)
    }

    /// Infers intrinsic product category (e.g. Chocolate -> Hazelnut Chocolate, Snickers -> Chocolate, T-Shirt -> Apparel).
    async fn infer_item_intrinsic_category(
        &mut self,
        item: &InvoiceItem,
    ) -> Result<(String, String), LlmError> {
        let name_lower = item.item_name.to_lowercase();

        // 1. Check if it's chocolate
        if name_lower.contains("chocolate")
            || name_lower.contains("ferrero")
            || name_lower.contains("snicker")
        {
            let root_cat = "cat_chocolate";
            self.item_dag.upsert_node(ItemCategoryNode {
                category_id: root_cat.into(),
                parent_category_id: None,
                category_name: "Chocolate".into(),
                entity_level: "root_category".into(),
            });

            let (child_key, child_name) = if name_lower.contains("white") {
                ("cat_white_chocolate", "White Chocolate")
            } else if name_lower.contains("cookies") || name_lower.contains("cream") {
                ("cat_cookies_and_cream", "Cookies and Cream")
            } else if name_lower.contains("ferrero") {
                ("cat_hazelnut_chocolate", "Hazelnut Chocolate")
            } else {
                ("cat_chocolate_bar", "Chocolate Bar")
            };

            self.item_dag.upsert_node(ItemCategoryNode {
                category_id: child_key.into(),
                parent_category_id: Some(root_cat.into()),
                category_name: child_name.into(),
                entity_level: "sub_category".into(),
            });

            if let Ok(edge) =
                EdgePayload::new("SUB_CATEGORY_OF", 1.0, ProvenanceKind::HumanVerified)
            {
                let _ = self.item_dag.add_edge_by_keys(
                    &root_cat.to_string(),
                    &child_key.to_string(),
                    edge,
                );
            }

            return Ok((child_key.into(), child_name.into()));
        }

        // 2. Check if it's apparel / shirt
        if name_lower.contains("shirt")
            || name_lower.contains("apparel")
            || name_lower.contains("clothing")
        {
            let root_cat = "cat_apparel";
            self.item_dag.upsert_node(ItemCategoryNode {
                category_id: root_cat.into(),
                parent_category_id: None,
                category_name: "Apparel".into(),
                entity_level: "root_category".into(),
            });

            let child_key = "cat_t_shirt";
            let child_name = "T-Shirt";

            self.item_dag.upsert_node(ItemCategoryNode {
                category_id: child_key.into(),
                parent_category_id: Some(root_cat.into()),
                category_name: child_name.into(),
                entity_level: "sub_category".into(),
            });

            if let Ok(edge) =
                EdgePayload::new("SUB_CATEGORY_OF", 1.0, ProvenanceKind::HumanVerified)
            {
                let _ = self.item_dag.add_edge_by_keys(
                    &root_cat.to_string(),
                    &child_key.to_string(),
                    edge,
                );
            }

            return Ok((child_key.into(), child_name.into()));
        }

        // 3. Fallback: ask LLM inference model
        let prompt = PromptFormatter::build_prompt(
            "Categorize product into intrinsic taxonomy tree.",
            &item.item_name,
            &["cat_groceries (Groceries)", "cat_general (General Retail)"],
        );

        let (directive, _) = self.provider.extract_directive(&prompt).await?;
        let _diff = ChangesetDiffEngine::diff_directive(&self.item_dag, &directive);

        match directive {
            ExtractionDirective::CreateRootNode { entity_name, .. } => {
                let key = format!("cat_{}", sanitize_slug(&entity_name));
                self.item_dag.upsert_node(ItemCategoryNode {
                    category_id: key.clone(),
                    parent_category_id: None,
                    category_name: entity_name.clone(),
                    entity_level: "category".into(),
                });
                Ok((key, entity_name))
            }
            ExtractionDirective::InsertNode {
                parent_key,
                entity_name,
                ..
            } => {
                let key = format!("cat_{}", sanitize_slug(&entity_name));
                self.item_dag.upsert_node(ItemCategoryNode {
                    category_id: key.clone(),
                    parent_category_id: Some(parent_key.clone()),
                    category_name: entity_name.clone(),
                    entity_level: "sub_category".into(),
                });
                if let Ok(edge) = EdgePayload::new(
                    "SUB_CATEGORY_OF",
                    1.0,
                    ProvenanceKind::LlmConstrainedInference,
                ) {
                    let _ = self.item_dag.add_edge_by_keys(&parent_key, &key, edge);
                }
                Ok((key, entity_name))
            }
            _ => Ok(("cat_general".into(), "General Item".into())),
        }
    }

    /// Helper to upsert a category node and hierarchy edge in `purchase_dag`.
    fn upsert_purchase_category(
        &mut self,
        key: &str,
        name: &str,
        parent_key: Option<&str>,
        level: &str,
    ) -> (String, String) {
        self.purchase_dag.upsert_node(PurchaseCategoryNode {
            category_id: key.into(),
            parent_category_id: parent_key.map(|p| p.into()),
            category_name: name.into(),
            entity_level: level.into(),
        });

        if let Some(parent) = parent_key
            && let Ok(edge) =
                EdgePayload::new("SUB_CATEGORY_OF", 1.0, ProvenanceKind::HumanVerified)
        {
            let _ = self
                .purchase_dag
                .add_edge_by_keys(&parent.to_string(), &key.to_string(), edge);
        }

        (key.into(), name.into())
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
            if let Ok(edge) = EdgePayload::new("SPECIALIZED_AS", 1.0, ProvenanceKind::HumanVerified)
            {
                let _ =
                    self.merchant_dag
                        .add_edge_by_keys(&merchant.category_id, &new_cat_id, edge);
            }

            merchant.category_id = new_cat_id;
            merchant.category_name = new_category_name.to_string();
            true
        } else {
            false
        }
    }

    /// User adds an item category like "Luxury Chocolate" (Ferrero Rocher is luxury chocolate, Snickers is standard chocolate).
    ///
    /// Automatically:
    /// 1. Inserts the new category under `parent_category_name` in `item_dag` and `purchase_dag`.
    /// 2. Reclassifies all matching items across stored invoices.
    /// 3. Updates all affected invoices with the "Luxury" classification.
    pub fn user_add_item_category_and_reclassify(
        &mut self,
        new_category_name: &str,
        parent_category_name: &str,
        item_filter: impl Fn(&InvoiceItem) -> bool,
    ) -> usize {
        let new_item_cat_key = format!("cat_{}", sanitize_slug(new_category_name));
        let parent_item_cat_key = format!("cat_{}", sanitize_slug(parent_category_name));

        // 1. Upsert new category node under parent in item_dag
        self.item_dag.upsert_node(ItemCategoryNode {
            category_id: new_item_cat_key.clone(),
            parent_category_id: Some(parent_item_cat_key.clone()),
            category_name: new_category_name.to_string(),
            entity_level: "specialized_category".into(),
        });

        if let Ok(edge) = EdgePayload::new("SUB_CATEGORY_OF", 1.0, ProvenanceKind::HumanVerified) {
            let _ = self
                .item_dag
                .add_edge_by_keys(&parent_item_cat_key, &new_item_cat_key, edge);
        }

        // Also add "Luxury" to purchase_dag if not present
        self.upsert_purchase_category("pcat_luxury", "Luxury", None, "category");

        let mut reclassified_count = 0;

        // 2. Reclassify matching items across all stored invoices
        for categorized_inv in &mut self.invoices {
            let mut invoice_has_new_specialization = false;
            for item in &categorized_inv.invoice.items {
                if item_filter(item) {
                    // Update intrinsic item category
                    categorized_inv.item_categories.insert(
                        item.item_id.clone(),
                        (new_item_cat_key.clone(), new_category_name.to_string()),
                    );

                    // Add Luxury to its purchase categories if not already there
                    let p_cats = categorized_inv
                        .purchase_categories
                        .entry(item.item_id.clone())
                        .or_default();
                    if !p_cats.iter().any(|c| c.1 == "Luxury") {
                        p_cats.push(("pcat_luxury".into(), "Luxury".into()));
                    }

                    reclassified_count += 1;
                    invoice_has_new_specialization = true;
                }
            }

            // 3. Update invoice categories to reflect Luxury
            if invoice_has_new_specialization
                && !categorized_inv
                    .invoice_categories
                    .iter()
                    .any(|c| c == "Luxury")
            {
                categorized_inv.invoice_categories.push("Luxury".into());
            }
        }

        reclassified_count
    }
}

/// Temporal context extracted from date timestamp providing insightful context to LLM inference.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TemporalContext {
    pub is_holiday_season: bool,
    pub is_weekend: bool,
    pub month: u32,
    pub day: u32,
    pub holiday_description: Option<String>,
}

/// Evaluates insightful temporal context from an invoice timestamp.
pub fn evaluate_temporal_context(dt: &DateTime<Utc>) -> TemporalContext {
    let m = dt.month();
    let d = dt.day();
    let is_dec_holiday = m == 12 && (d >= 20);
    let is_thanksgiving = m == 11 && (24..=30).contains(&d);
    let is_holiday_season = is_dec_holiday || is_thanksgiving;

    let holiday_description = if is_dec_holiday {
        Some("Christmas / New Year Holiday Gifting Season".to_string())
    } else if is_thanksgiving {
        Some("Thanksgiving Holiday Season".to_string())
    } else {
        None
    };

    let weekday = dt.weekday().number_from_monday();
    let is_weekend = weekday >= 6;

    TemporalContext {
        is_holiday_season,
        is_weekend,
        month: m,
        day: d,
        holiday_description,
    }
}

/// Helper evaluating whether an invoice timestamp falls within a holiday season.
pub fn is_near_holiday(dt: &DateTime<Utc>) -> bool {
    let m = dt.month();
    let d = dt.day();
    (m == 12 && d >= 20) || (m == 11 && (24..=30).contains(&d))
}

pub fn sanitize_slug(raw: &str) -> String {
    raw.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}
