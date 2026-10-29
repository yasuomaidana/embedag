//! Data transfer objects and hierarchical category models for finance categorization.

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use embedag_diesel::cte::HierarchicalModel;
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
