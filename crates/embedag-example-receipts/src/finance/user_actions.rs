use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_llm::provider::LlmProvider;

use super::dto::InvoiceItem;
use super::engine::FinanceCategorizationEngine;
use super::nodes::{ItemCategoryNode, MerchantCategoryNode};
use super::temporal::sanitize_slug;

impl<P: LlmProvider> FinanceCategorizationEngine<P> {
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
