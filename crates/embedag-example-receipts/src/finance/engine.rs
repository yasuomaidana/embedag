use std::collections::HashMap;

use embedag_core::dag::MemoryDag;
use embedag_llm::provider::{LlmError, LlmProvider};

use super::dto::{CategorizedInvoice, FinanceInvoice, FinanceMerchant, InvoiceCategorizationMode};
use super::nodes::{
    InvoiceCategoryNode, ItemCategoryNode, MerchantCategoryNode, PurchaseCategoryNode,
};
use super::temporal::evaluate_temporal_context;

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
        let temporal = evaluate_temporal_context(&invoice.date);
        let merchant = self.merchants.get(&invoice.merchant_id).cloned();
        let is_luxury_store = merchant.as_ref().is_some_and(|m| self.is_luxury_store(m));

        let mut item_categories: HashMap<String, (String, String)> = HashMap::new();
        let mut purchase_categories: HashMap<String, Vec<(String, String)>> = HashMap::new();
        let mut category_spends: HashMap<String, f64> = HashMap::new();

        // Intrinsic item taxonomy (one value) + contextual purchase taxonomy (one or more).
        for item in &invoice.items {
            let (item_cat_key, item_cat_name) = self.infer_item_intrinsic_category(item).await?;
            *category_spends.entry(item_cat_name.clone()).or_insert(0.0) += item.total_price;
            item_categories.insert(item.item_id.clone(), (item_cat_key, item_cat_name));
            let categories = self.purchase_categories_for(
                &invoice,
                item,
                is_luxury_store,
                temporal.is_holiday_season,
            );
            purchase_categories.insert(item.item_id.clone(), categories);
        }

        let invoice_categories = match mode {
            InvoiceCategorizationMode::LlmDecided => {
                self.llm_decided_categories(&invoice, merchant.as_ref(), &temporal, is_luxury_store)
                    .await
            }
            InvoiceCategorizationMode::CustomDensity(density_fn) => {
                density_fn(&invoice, &category_spends)
            }
        };
        self.record_invoice_categories(&invoice_categories);

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
