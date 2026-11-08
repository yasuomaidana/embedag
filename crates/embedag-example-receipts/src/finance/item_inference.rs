use embedag_core::dag::MemoryDag;
use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_core::traits::NodePayload;
use embedag_llm::prompt::PromptFormatter;
use embedag_llm::provider::{LlmError, LlmProvider};
use embedag_llm::schema::ExtractionDirective;

use super::dto::InvoiceItem;
use super::engine::FinanceCategorizationEngine;
use super::nodes::{ItemCategoryNode, PurchaseCategoryNode};
use super::temporal::sanitize_slug;

fn mentions_any(lower: &str, words: &[&str]) -> bool {
    words.iter().any(|word| lower.contains(word))
}

/// Adds a `SUB_CATEGORY_OF` edge `parent -> child`; a failed insertion is deliberately ignored.
fn link_sub_category<T: NodePayload<Key = String>>(
    dag: &mut MemoryDag<T>,
    parent: &str,
    child: &str,
    provenance: ProvenanceKind,
) {
    if let Ok(edge) = EdgePayload::new("SUB_CATEGORY_OF", 1.0, provenance) {
        let _ = dag.add_edge_by_keys(&parent.to_string(), &child.to_string(), edge);
    }
}

impl<P: LlmProvider> FinanceCategorizationEngine<P> {
    /// Infers intrinsic product category (e.g. Chocolate -> Hazelnut Chocolate, Snickers -> Chocolate, T-Shirt -> Apparel).
    pub(crate) async fn infer_item_intrinsic_category(
        &mut self,
        item: &InvoiceItem,
    ) -> Result<(String, String), LlmError> {
        let name_lower = item.item_name.to_lowercase();
        if mentions_any(&name_lower, &["chocolate", "ferrero", "snicker"]) {
            return Ok(self.categorize_chocolate(&name_lower));
        }
        if mentions_any(&name_lower, &["shirt", "apparel", "clothing"]) {
            return Ok(self.categorize_apparel());
        }
        self.categorize_with_llm(item).await
    }

    fn categorize_chocolate(&mut self, name_lower: &str) -> (String, String) {
        let (child_key, child_name) = if name_lower.contains("white") {
            ("cat_white_chocolate", "White Chocolate")
        } else if name_lower.contains("cookies") || name_lower.contains("cream") {
            ("cat_cookies_and_cream", "Cookies and Cream")
        } else if name_lower.contains("ferrero") {
            ("cat_hazelnut_chocolate", "Hazelnut Chocolate")
        } else {
            ("cat_chocolate_bar", "Chocolate Bar")
        };
        self.upsert_item_category_pair(("cat_chocolate", "Chocolate"), (child_key, child_name))
    }

    fn categorize_apparel(&mut self) -> (String, String) {
        self.upsert_item_category_pair(("cat_apparel", "Apparel"), ("cat_t_shirt", "T-Shirt"))
    }

    /// Upserts a root category and one sub-category under it, linked by a human-verified edge.
    fn upsert_item_category_pair(
        &mut self,
        (root_key, root_name): (&str, &str),
        (child_key, child_name): (&str, &str),
    ) -> (String, String) {
        self.item_dag.upsert_node(ItemCategoryNode {
            category_id: root_key.into(),
            parent_category_id: None,
            category_name: root_name.into(),
            entity_level: "root_category".into(),
        });
        self.item_dag.upsert_node(ItemCategoryNode {
            category_id: child_key.into(),
            parent_category_id: Some(root_key.into()),
            category_name: child_name.into(),
            entity_level: "sub_category".into(),
        });
        link_sub_category(
            &mut self.item_dag,
            root_key,
            child_key,
            ProvenanceKind::HumanVerified,
        );
        (child_key.into(), child_name.into())
    }

    /// Fallback: ask the LLM inference model.
    async fn categorize_with_llm(
        &mut self,
        item: &InvoiceItem,
    ) -> Result<(String, String), LlmError> {
        let prompt = PromptFormatter::build_prompt(
            "Categorize product into intrinsic taxonomy tree.",
            &item.item_name,
            &["cat_groceries (Groceries)", "cat_general (General Retail)"],
        );
        let (directive, _) = self.provider.extract_directive(&prompt).await?;
        Ok(self.record_llm_category(directive))
    }

    fn record_llm_category(&mut self, directive: ExtractionDirective) -> (String, String) {
        match directive {
            ExtractionDirective::CreateRootNode { entity_name, .. } => {
                let key = format!("cat_{}", sanitize_slug(&entity_name));
                self.item_dag.upsert_node(ItemCategoryNode {
                    category_id: key.clone(),
                    parent_category_id: None,
                    category_name: entity_name.clone(),
                    entity_level: "category".into(),
                });
                (key, entity_name)
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
                link_sub_category(
                    &mut self.item_dag,
                    &parent_key,
                    &key,
                    ProvenanceKind::LlmConstrainedInference,
                );
                (key, entity_name)
            }
            _ => ("cat_general".into(), "General Item".into()),
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

        if let Some(parent) = parent_key {
            link_sub_category(
                &mut self.purchase_dag,
                parent,
                key,
                ProvenanceKind::HumanVerified,
            );
        }

        (key.into(), name.into())
    }
}
