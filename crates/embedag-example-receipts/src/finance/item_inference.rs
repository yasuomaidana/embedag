use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_llm::diff::ChangesetDiffEngine;
use embedag_llm::prompt::PromptFormatter;
use embedag_llm::provider::{LlmError, LlmProvider};
use embedag_llm::schema::ExtractionDirective;

use super::dto::InvoiceItem;
use super::engine::FinanceCategorizationEngine;
use super::nodes::{ItemCategoryNode, PurchaseCategoryNode};
use super::temporal::sanitize_slug;

impl<P: LlmProvider> FinanceCategorizationEngine<P> {
    /// Infers intrinsic product category (e.g. Chocolate -> Hazelnut Chocolate, Snickers -> Chocolate, T-Shirt -> Apparel).
    pub(crate) async fn infer_item_intrinsic_category(
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
    pub(crate) fn upsert_purchase_category(
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
}
