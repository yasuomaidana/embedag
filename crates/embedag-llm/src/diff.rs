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

/// Proposes the edge `parent_key -> entity_name` unless the DAG already has it; the confidence is
/// clamped to `[0.0, 1.0]`.
fn push_new_edge<T>(
    diff: &mut GraphDiff<String>,
    dag: &MemoryDag<T>,
    parent_key: &String,
    entity_name: &String,
    relationship: &str,
    confidence: f32,
) where
    T: NodePayload<Key = String>,
{
    let edge_exists = dag
        .children_of(parent_key)
        .iter()
        .any(|child| child.key() == entity_name);
    if edge_exists {
        return;
    }
    if let Some(edge) = proposed_edge(relationship, confidence) {
        diff.new_edges
            .push((parent_key.clone(), entity_name.clone(), edge));
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
                relationship,
                confidence,
                ..
            } => {
                push_new_node(&mut diff, dag, entity_name);
                push_new_edge(
                    &mut diff,
                    dag,
                    parent_key,
                    entity_name,
                    relationship,
                    *confidence,
                );
            }
            ExtractionDirective::InsertMultiParentNode {
                parent_keys,
                entity_name,
                relationship,
                confidence,
                ..
            } => {
                push_new_node(&mut diff, dag, entity_name);
                for parent_key in parent_keys {
                    push_new_edge(
                        &mut diff,
                        dag,
                        parent_key,
                        entity_name,
                        relationship,
                        *confidence,
                    );
                }
            }
            ExtractionDirective::CreateRootNode { entity_name, .. } => {
                push_new_node(&mut diff, dag, entity_name);
            }
            ExtractionDirective::UpdateAttributes {
                target_key,
                updated_fields,
                ..
            } => push_update(&mut diff, dag, target_key, updated_fields),
            ExtractionDirective::RetargetEdge {
                child_key,
                old_parent_key,
                new_parent_key,
                relationship,
                confidence,
            } => {
                diff.removed_edges
                    .push((old_parent_key.clone(), child_key.clone()));
                if let Some(edge) = proposed_edge(relationship, *confidence) {
                    diff.new_edges
                        .push((new_parent_key.clone(), child_key.clone(), edge));
                }
            }
            ExtractionDirective::MarkAmbiguous { .. } => {}
        }

        diff
    }
}

/// Proposes updating `target_key` when it exists and there is something to update.
fn push_update<T>(
    diff: &mut GraphDiff<String>,
    dag: &MemoryDag<T>,
    target_key: &String,
    updated_fields: &HashMap<String, serde_json::Value>,
) where
    T: NodePayload<Key = String>,
{
    if dag.contains_key(target_key) && !updated_fields.is_empty() {
        diff.updated_nodes
            .push((target_key.clone(), updated_fields.clone()));
    }
}

/// An LLM-proposed edge; the confidence is clamped to `[0.0, 1.0]`.
fn proposed_edge(relationship: &str, confidence: f32) -> Option<EdgePayload> {
    EdgePayload::new(
        relationship,
        confidence.clamp(0.0, 1.0),
        ProvenanceKind::LlmConstrainedInference,
    )
    .ok()
}

/// Proposes the node `entity_name` unless the DAG already has it.
fn push_new_node<T>(diff: &mut GraphDiff<String>, dag: &MemoryDag<T>, entity_name: &String)
where
    T: NodePayload<Key = String>,
{
    if !dag.contains_key(entity_name) {
        diff.new_nodes
            .push((entity_name.clone(), entity_name.clone()));
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

    use rstest::{fixture, rstest};
    use serde_json::json;

    /// `root -> child`, both present.
    #[fixture]
    fn dag() -> MemoryDag<DummyNode> {
        let mut dag = MemoryDag::new();
        for key in ["root", "other", "child"] {
            dag.upsert_node(DummyNode {
                key: key.into(),
                label: key.into(),
            });
        }
        let edge = EdgePayload::new("PARENT_OF", 1.0, ProvenanceKind::DeterministicRule).unwrap();
        dag.add_edge_by_keys(&"root".to_string(), &"child".to_string(), edge)
            .unwrap();
        dag
    }

    fn multi_parent(parents: &[&str], entity: &str) -> ExtractionDirective {
        ExtractionDirective::InsertMultiParentNode {
            parent_keys: parents.iter().map(|p| p.to_string()).collect(),
            entity_name: entity.into(),
            entity_level: "sku".into(),
            relationship: "PARENT_OF".into(),
            confidence: 0.9,
        }
    }

    #[rstest]
    #[case::new_node_two_parents(multi_parent(&["root", "other"], "fresh"), 1, 2)]
    #[case::existing_node_one_new_parent(multi_parent(&["root", "other"], "child"), 0, 1)]
    #[case::fully_known(multi_parent(&["root"], "child"), 0, 0)]
    fn multi_parent_inserts_only_propose_what_is_missing(
        dag: MemoryDag<DummyNode>,
        #[case] directive: ExtractionDirective,
        #[case] new_nodes: usize,
        #[case] new_edges: usize,
    ) {
        let diff = ChangesetDiffEngine::diff_directive(&dag, &directive);
        assert_eq!(
            (diff.new_nodes.len(), diff.new_edges.len()),
            (new_nodes, new_edges)
        );
    }

    #[rstest]
    #[case::new_root("brand-new", 1)]
    #[case::existing_root("root", 0)]
    fn root_nodes_are_only_proposed_when_missing(
        dag: MemoryDag<DummyNode>,
        #[case] name: &str,
        #[case] new_nodes: usize,
    ) {
        let directive = ExtractionDirective::CreateRootNode {
            entity_name: name.into(),
            entity_level: "category".into(),
            confidence: 1.0,
        };
        let diff = ChangesetDiffEngine::diff_directive(&dag, &directive);
        assert_eq!(diff.new_nodes.len(), new_nodes);
    }

    #[rstest]
    #[case::known_target("root", json!({"colour": "red"}), 1)]
    #[case::unknown_target("ghost", json!({"colour": "red"}), 0)]
    #[case::nothing_to_update("root", json!({}), 0)]
    fn attribute_updates_need_a_known_target_and_fields(
        dag: MemoryDag<DummyNode>,
        #[case] target: &str,
        #[case] fields: serde_json::Value,
        #[case] updates: usize,
    ) {
        let updated_fields = serde_json::from_value(fields).unwrap();
        let directive = ExtractionDirective::UpdateAttributes {
            target_key: target.into(),
            updated_fields,
            confidence: 0.8,
        };
        let diff = ChangesetDiffEngine::diff_directive(&dag, &directive);
        assert_eq!(diff.updated_nodes.len(), updates);
    }

    #[rstest]
    #[case::valid("MOVED", 3.0, 1)]
    #[case::nan_confidence("MOVED", f32::NAN, 0)]
    fn retargeting_removes_the_old_edge_and_proposes_a_clamped_new_one(
        dag: MemoryDag<DummyNode>,
        #[case] relationship: &str,
        #[case] confidence: f32,
        #[case] new_edges: usize,
    ) {
        let directive = ExtractionDirective::RetargetEdge {
            child_key: "child".into(),
            old_parent_key: "root".into(),
            new_parent_key: "other".into(),
            relationship: relationship.into(),
            confidence,
        };
        let diff = ChangesetDiffEngine::diff_directive(&dag, &directive);
        assert_eq!(
            diff.removed_edges,
            vec![("root".to_string(), "child".to_string())]
        );
        assert_eq!(diff.new_edges.len(), new_edges);
        assert!(diff.new_edges.iter().all(|(_, _, e)| e.confidence <= 1.0));
    }

    #[rstest]
    fn an_unusable_confidence_proposes_no_edge(dag: MemoryDag<DummyNode>) {
        let directive = ExtractionDirective::InsertNode {
            parent_key: "other".into(),
            entity_name: "child".into(),
            entity_level: "sku".into(),
            relationship: "PARENT_OF".into(),
            confidence: f32::NAN,
        };
        assert!(ChangesetDiffEngine::diff_directive(&dag, &directive).is_empty());
    }

    #[rstest]
    fn ambiguity_proposes_nothing(dag: MemoryDag<DummyNode>) {
        let directive = ExtractionDirective::MarkAmbiguous {
            candidate_keys: vec!["root".into()],
            reason: "two matches".into(),
        };
        assert!(ChangesetDiffEngine::diff_directive(&dag, &directive).is_empty());
    }

    #[test]
    fn a_diff_is_empty_only_when_every_part_is() {
        let empty: GraphDiff<String> = GraphDiff::default();
        assert!(empty.is_empty());
        let edge = || EdgePayload::new("R", 1.0, ProvenanceKind::DeterministicRule).unwrap();
        let variants = [
            GraphDiff {
                new_nodes: vec![("a".into(), "a".into())],
                ..empty.clone()
            },
            GraphDiff {
                updated_nodes: vec![("a".into(), HashMap::new())],
                ..empty.clone()
            },
            GraphDiff {
                new_edges: vec![("a".into(), "b".into(), edge())],
                ..empty.clone()
            },
            GraphDiff {
                removed_edges: vec![("a".into(), "b".into())],
                ..empty.clone()
            },
        ];
        assert!(variants.iter().all(|d| !d.is_empty()));
    }
}
