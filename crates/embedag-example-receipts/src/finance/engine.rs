use std::collections::HashMap;

use embedag_core::dag::MemoryDag;
use embedag_llm::prompt::PromptFormatter;
use embedag_llm::provider::{LlmError, LlmProvider};
use embedag_llm::schema::ExtractionDirective;

use super::dto::{CategorizedInvoice, FinanceInvoice, FinanceMerchant, InvoiceCategorizationMode};
use super::nodes::{
    InvoiceCategoryNode, ItemCategoryNode, MerchantCategoryNode, PurchaseCategoryNode,
};
use super::temporal::{evaluate_temporal_context, sanitize_slug};

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
}
