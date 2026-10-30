use crate::schema::ExtractionDirective;
use embedag_core::dag::MemoryDag;
use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_core::traits::NodePayload;
use std::collections::HashMap;

/// Partitioned changeset representing proposed modifications to the graph.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphDiff<K: Clone> {
    pub new_nodes: Vec<(K, String)>,
    pub updated_nodes: Vec<(K, HashMap<String, serde_json::Value>)>,
    pub new_edges: Vec<(K, K, EdgePayload)>,
    pub removed_edges: Vec<(K, K)>,
}

impl<K: Clone> Default for GraphDiff<K> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<K: Clone> GraphDiff<K> {
    /// Creates a new empty diff.
    pub fn empty() -> Self {
        Self {
            new_nodes: Vec::new(),
            updated_nodes: Vec::new(),
            new_edges: Vec::new(),
            removed_edges: Vec::new(),
        }
    }

    /// Returns true if the changeset contains no proposed changes.
    pub fn is_empty(&self) -> bool {
        self.new_nodes.is_empty()
            && self.updated_nodes.is_empty()
            && self.new_edges.is_empty()
            && self.removed_edges.is_empty()
    }
}

/// Changeset diffing engine comparing LLM proposals or target topologies against an active `MemoryDag`.
pub struct ChangesetDiffEngine;

impl ChangesetDiffEngine {
    /// Computes an atomic `GraphDiff` by evaluating an `ExtractionDirective` against an active `MemoryDag`.
    pub fn diff_directive<T>(
        dag: &MemoryDag<T>,
        directive: &ExtractionDirective,
    ) -> GraphDiff<T::Key>
    where
        T: NodePayload<Key = String>,
    {
        let mut diff = GraphDiff::empty();

        match directive {
            ExtractionDirective::InsertNode {
                parent_key,
                entity_name,
                entity_level: _,
                relationship,
                confidence,
            } => {
                let node_exists = dag.contains_key(entity_name);
                if !node_exists {
                    diff.new_nodes
                        .push((entity_name.clone(), entity_name.clone()));
                }

                // Check if directed edge (parent -> child) exists
                let edge_exists = dag
                    .children_of(parent_key)
                    .iter()
                    .any(|child| child.key() == entity_name);

                if !edge_exists {
                    let valid_conf = confidence.clamp(0.0, 1.0);
                    if let Ok(edge) = EdgePayload::new(
                        relationship.clone(),
                        valid_conf,
                        ProvenanceKind::LlmConstrainedInference,
                    ) {
                        diff.new_edges
                            .push((parent_key.clone(), entity_name.clone(), edge));
                    }
                }
            }

            ExtractionDirective::InsertMultiParentNode {
                parent_keys,
                entity_name,
                entity_level: _,
                relationship,
                confidence,
            } => {
                if !dag.contains_key(entity_name) {
                    diff.new_nodes
                        .push((entity_name.clone(), entity_name.clone()));
                }

                for parent_key in parent_keys {
                    let edge_exists = dag
                        .children_of(parent_key)
                        .iter()
                        .any(|child| child.key() == entity_name);

                    if !edge_exists {
                        let valid_conf = confidence.clamp(0.0, 1.0);
                        if let Ok(edge) = EdgePayload::new(
                            relationship.clone(),
                            valid_conf,
                            ProvenanceKind::LlmConstrainedInference,
                        ) {
                            diff.new_edges
                                .push((parent_key.clone(), entity_name.clone(), edge));
                        }
                    }
                }
            }

            ExtractionDirective::CreateRootNode {
                entity_name,
                entity_level: _,
                confidence: _,
            } => {
                if !dag.contains_key(entity_name) {
                    diff.new_nodes
                        .push((entity_name.clone(), entity_name.clone()));
                }
            }

            ExtractionDirective::UpdateAttributes {
                target_key,
                updated_fields,
                confidence: _,
            } => {
                if dag.contains_key(target_key) && !updated_fields.is_empty() {
                    diff.updated_nodes
                        .push((target_key.clone(), updated_fields.clone()));
                }
            }

            ExtractionDirective::RetargetEdge {
                child_key,
                old_parent_key,
                new_parent_key,
                relationship,
                confidence,
            } => {
                diff.removed_edges
                    .push((old_parent_key.clone(), child_key.clone()));
                let valid_conf = confidence.clamp(0.0, 1.0);
                if let Ok(edge) = EdgePayload::new(
                    relationship.clone(),
                    valid_conf,
                    ProvenanceKind::LlmConstrainedInference,
                ) {
                    diff.new_edges
                        .push((new_parent_key.clone(), child_key.clone(), edge));
                }
            }

            ExtractionDirective::MarkAmbiguous { .. } => {}
        }

        diff
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedag_core::model::EdgePayload;

    #[derive(Debug, Clone, PartialEq)]
    struct DummyNode {
        key: String,
        label: String,
    }

    embedag_core::impl_node_payload!(
        DummyNode,
        key = |n| &n.key,
        name = |n| &n.label,
        level = |_| "item"
    );

    #[test]
    fn test_diffing_idempotency() {
        let mut dag = MemoryDag::new();
        let parent = DummyNode {
            key: "parent_cat".into(),
            label: "Beverages".into(),
        };
        let child = DummyNode {
            key: "child_item".into(),
            label: "child_item".into(),
        };

        dag.upsert_node(parent);
        dag.upsert_node(child);
        let edge = EdgePayload::new("PARENT_OF", 1.0, ProvenanceKind::DeterministicRule)
            .expect("valid edge");
        dag.add_edge_by_keys(&"parent_cat".to_string(), &"child_item".to_string(), edge)
            .expect("add edge");

        // Construct identical proposal
        let identical_directive = ExtractionDirective::InsertNode {
            parent_key: "parent_cat".into(),
            entity_name: "child_item".into(),
            entity_level: "sku".into(),
            relationship: "PARENT_OF".into(),
            confidence: 0.99,
        };

        // Diff against active DAG
        let diff = ChangesetDiffEngine::diff_directive(&dag, &identical_directive);

        // Idempotency: identical proposal must result in empty changeset
        assert!(
            diff.is_empty(),
            "Diff for existing node and edge must be empty, but got: {:?}",
            diff
        );
    }

    #[test]
    fn test_diffing_new_entity() {
        let mut dag = MemoryDag::new();
        let root = DummyNode {
            key: "root".into(),
            label: "Root Category".into(),
        };
        dag.upsert_node(root);

        let proposal = ExtractionDirective::InsertNode {
            parent_key: "root".into(),
            entity_name: "new_child".into(),
            entity_level: "sku".into(),
            relationship: "CHILD_OF".into(),
            confidence: 0.95,
        };

        let diff = ChangesetDiffEngine::diff_directive(&dag, &proposal);
        assert!(!diff.is_empty());
        assert_eq!(diff.new_nodes.len(), 1);
        assert_eq!(diff.new_edges.len(), 1);
        assert_eq!(diff.new_nodes[0].0, "new_child");
    }
}
