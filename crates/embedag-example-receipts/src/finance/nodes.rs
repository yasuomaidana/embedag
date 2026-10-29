use embedag_diesel::cte::HierarchicalModel;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
