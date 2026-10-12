//! Declarative graph migration and transformation engine.
//!
//! Traceability: `doc/01_Development_Plan.md` § Ticket 6.2.1,
//! `doc/14_Dynamic_Graph_Mutations_and_Diffing.md`.

use crate::dag::MemoryDag;
use crate::error::DagError;
use crate::model::{EdgePayload, ProvenanceKind};
use crate::mutation::MutationDirective;
use crate::traits::NodePayload;
use std::fmt;

/// Atomic migration operation describing graph restructuring.
#[derive(Clone)]
pub enum MigrationStep<T: NodePayload> {
    /// Reparents a child node from an old parent to a new parent.
    Reparent {
        /// Child node key to reparent.
        child_key: T::Key,
        /// Current parent node key.
        old_parent_key: T::Key,
        /// Destination parent node key.
        new_parent_key: T::Key,
        /// Relationship label.
        relationship: String,
        /// Confidence score.
        confidence: f32,
    },
    /// Splits an existing node into new categories/nodes and routes children to the appropriate target.
    SplitCategory {
        /// Source category key being split.
        source_category: T::Key,
        /// New category payloads replacing or subdividing the source.
        new_categories: Vec<T>,
        /// Child rerouting mapping `(child_key, target_new_category_key)`.
        child_routes: Vec<(T::Key, T::Key)>,
        /// Relationship label.
        relationship: String,
    },
    /// Merges multiple source nodes into a single consolidated target node.
    MergeNodes {
        /// Source node keys being merged.
        sources: Vec<T::Key>,
        /// Consolidated target payload.
        target: T,
        /// Relationship label.
        relationship: String,
    },
}

impl<T: NodePayload + fmt::Debug> fmt::Debug for MigrationStep<T>
where
    T::Key: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reparent {
                child_key,
                old_parent_key,
                new_parent_key,
                relationship,
                confidence,
            } => f
                .debug_struct("Reparent")
                .field("child_key", child_key)
                .field("old_parent_key", old_parent_key)
                .field("new_parent_key", new_parent_key)
                .field("relationship", relationship)
                .field("confidence", confidence)
                .finish(),
            Self::SplitCategory {
                source_category,
                new_categories,
                child_routes,
                relationship,
            } => f
                .debug_struct("SplitCategory")
                .field("source_category", source_category)
                .field("new_categories", new_categories)
                .field("child_routes", child_routes)
                .field("relationship", relationship)
                .finish(),
            Self::MergeNodes {
                sources,
                target,
                relationship,
            } => f
                .debug_struct("MergeNodes")
                .field("sources", sources)
                .field("target", target)
                .field("relationship", relationship)
                .finish(),
        }
    }
}

/// Declarative runner executing schema and topological graph migrations safely.
#[derive(Clone)]
pub struct GraphMigration<T: NodePayload> {
    steps: Vec<MigrationStep<T>>,
}

impl<T: NodePayload> Default for GraphMigration<T> {
    fn default() -> Self {
        Self { steps: Vec::new() }
    }
}

impl<T: NodePayload + fmt::Debug> fmt::Debug for GraphMigration<T>
where
    T::Key: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GraphMigration")
            .field("steps", &self.steps)
            .finish()
    }
}

impl<T: NodePayload> GraphMigration<T> {
    /// Creates an empty migration plan.
    #[must_use]
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    /// Adds a step to the migration plan.
    pub fn add_step(&mut self, step: MigrationStep<T>) -> &mut Self {
        self.steps.push(step);
        self
    }

    /// Validates and applies all migration steps to the in-memory DAG arena,
    /// emitting corresponding [`MutationDirective`]s for persistent storage synchronization.
    ///
    /// # Errors
    /// Returns [`DagError`] if a node is missing or if any migration step would introduce a cycle.
    pub fn apply(&self, dag: &mut MemoryDag<T>) -> Result<Vec<MutationDirective<T>>, DagError> {
        let mut directives = Vec::new();

        for step in &self.steps {
            match step {
                MigrationStep::Reparent {
                    child_key,
                    old_parent_key,
                    new_parent_key,
                    relationship,
                    confidence,
                } => {
                    // Check if new parent edge would create a cycle
                    if dag.would_create_cycle(new_parent_key, child_key) {
                        return Err(DagError::CycleDetected {
                            node_indices: vec![],
                        });
                    }

                    // Remove old edge
                    dag.remove_edge_by_keys(old_parent_key, child_key, Some(relationship));
                    directives.push(MutationDirective::RemoveEdge {
                        parent: old_parent_key.clone(),
                        child: child_key.clone(),
                        relationship: Some(relationship.clone()),
                    });

                    // Add new edge
                    let edge = EdgePayload::new(
                        relationship,
                        *confidence,
                        ProvenanceKind::DeterministicRule,
                    )?;
                    dag.add_edge_by_keys(new_parent_key, child_key, edge.clone())?;
                    directives.push(MutationDirective::AddEdge {
                        parent: new_parent_key.clone(),
                        child: child_key.clone(),
                        edge,
                    });
                }
                MigrationStep::SplitCategory {
                    source_category,
                    new_categories,
                    child_routes,
                    relationship,
                } => {
                    // 1. Insert new split categories
                    for new_cat in new_categories {
                        dag.upsert_node(new_cat.clone());
                        directives.push(MutationDirective::UpsertNode(new_cat.clone()));
                    }

                    // 2. Reroute child edges
                    for (child_key, target_cat_key) in child_routes {
                        if dag.would_create_cycle(target_cat_key, child_key) {
                            return Err(DagError::CycleDetected {
                                node_indices: vec![],
                            });
                        }

                        dag.remove_edge_by_keys(source_category, child_key, Some(relationship));
                        directives.push(MutationDirective::RemoveEdge {
                            parent: source_category.clone(),
                            child: child_key.clone(),
                            relationship: Some(relationship.clone()),
                        });

                        let edge =
                            EdgePayload::new(relationship, 1.0, ProvenanceKind::DeterministicRule)?;
                        dag.add_edge_by_keys(target_cat_key, child_key, edge.clone())?;
                        directives.push(MutationDirective::AddEdge {
                            parent: target_cat_key.clone(),
                            child: child_key.clone(),
                            edge,
                        });
                    }
                }
                MigrationStep::MergeNodes {
                    sources,
                    target,
                    relationship,
                } => {
                    dag.upsert_node(target.clone());
                    directives.push(MutationDirective::UpsertNode(target.clone()));

                    for source_key in sources {
                        let children_keys: Vec<T::Key> = dag
                            .children_of(source_key)
                            .iter()
                            .map(|c| c.key().clone())
                            .collect();

                        for c_key in children_keys {
                            dag.remove_edge_by_keys(source_key, &c_key, Some(relationship));
                            directives.push(MutationDirective::RemoveEdge {
                                parent: source_key.clone(),
                                child: c_key.clone(),
                                relationship: Some(relationship.clone()),
                            });

                            let edge = EdgePayload::new(
                                relationship,
                                1.0,
                                ProvenanceKind::DeterministicRule,
                            )?;
                            dag.add_edge_by_keys(target.key(), &c_key, edge.clone())?;
                            directives.push(MutationDirective::AddEdge {
                                parent: target.key().clone(),
                                child: c_key,
                                edge,
                            });
                        }
                    }
                }
            }
        }

        // Final topological consistency validation
        dag.validate_acyclic()?;

        Ok(directives)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct MockNode {
        id: String,
        name: String,
        level: String,
    }

    impl MockNode {
        fn new(id: &str, name: &str, level: &str) -> Self {
            Self {
                id: id.into(),
                name: name.into(),
                level: level.into(),
            }
        }
    }

    impl NodePayload for MockNode {
        type Key = String;
        fn key(&self) -> &Self::Key {
            &self.id
        }
        fn name(&self) -> &str {
            &self.name
        }
        fn entity_level(&self) -> &str {
            &self.level
        }
    }

    #[test]
    fn test_category_split_migration() {
        let mut dag = MemoryDag::new();

        // Initial setup: "Confectionery" contains "Carlos V Dark" and "Gummy Bears"
        let conf = MockNode::new("CAT-CONF", "Confectionery", "Category");
        let choco = MockNode::new("SKU-CHOCO", "Carlos V Dark 50g", "Product");
        let gummy = MockNode::new("SKU-GUMMY", "Gummy Bears 100g", "Product");

        dag.upsert_node(conf);
        dag.upsert_node(choco);
        dag.upsert_node(gummy);

        let e1 = EdgePayload::new("PARENT_OF", 1.0, ProvenanceKind::DeterministicRule).unwrap();
        let e2 = EdgePayload::new("PARENT_OF", 1.0, ProvenanceKind::DeterministicRule).unwrap();
        dag.add_edge_by_keys(&"CAT-CONF".into(), &"SKU-CHOCO".into(), e1)
            .unwrap();
        dag.add_edge_by_keys(&"CAT-CONF".into(), &"SKU-GUMMY".into(), e2)
            .unwrap();

        // Plan Migration: Split "Confectionery" into "CAT-CHOCO" (Chocolate) and "CAT-CANDY" (Candy)
        let cat_chocolate = MockNode::new("CAT-CHOCO", "Chocolate", "Category");
        let cat_candy = MockNode::new("CAT-CANDY", "Candy & Gummies", "Category");

        let mut migration = GraphMigration::new();
        migration.add_step(MigrationStep::SplitCategory {
            source_category: "CAT-CONF".into(),
            new_categories: vec![cat_chocolate, cat_candy],
            child_routes: vec![
                ("SKU-CHOCO".into(), "CAT-CHOCO".into()),
                ("SKU-GUMMY".into(), "CAT-CANDY".into()),
            ],
            relationship: "PARENT_OF".into(),
        });

        let directives = migration.apply(&mut dag).expect("migration must succeed");
        // 2 UpsertNode + 2 RemoveEdge + 2 AddEdge = 6 directives
        assert_eq!(directives.len(), 6);

        // Check new parents in DAG
        let choco_parents = dag.parents_of(&"SKU-CHOCO".into());
        assert_eq!(choco_parents.len(), 1);
        assert_eq!(choco_parents[0].key(), "CAT-CHOCO");

        let gummy_parents = dag.parents_of(&"SKU-GUMMY".into());
        assert_eq!(gummy_parents.len(), 1);
        assert_eq!(gummy_parents[0].key(), "CAT-CANDY");

        // Old category should have 0 children
        assert!(dag.children_of(&"CAT-CONF".into()).is_empty());
        assert!(dag.is_acyclic());
    }

    #[test]
    fn test_migration_reparent_prevents_cycles() {
        let mut dag = MemoryDag::new();
        let a = MockNode::new("A", "A", "L0");
        let b = MockNode::new("B", "B", "L1");
        let c = MockNode::new("C", "C", "L2");

        dag.upsert_node(a);
        dag.upsert_node(b);
        dag.upsert_node(c);

        let e1 = EdgePayload::new("PARENT_OF", 1.0, ProvenanceKind::DeterministicRule).unwrap();
        let e2 = EdgePayload::new("PARENT_OF", 1.0, ProvenanceKind::DeterministicRule).unwrap();
        dag.add_edge_by_keys(&"A".into(), &"B".into(), e1).unwrap();
        dag.add_edge_by_keys(&"B".into(), &"C".into(), e2).unwrap();

        // Reparenting A to C would create cycle C -> A -> B -> C!
        let mut migration = GraphMigration::new();
        migration.add_step(MigrationStep::Reparent {
            child_key: "A".into(),
            old_parent_key: "ORIGIN".into(),
            new_parent_key: "C".into(),
            relationship: "PARENT_OF".into(),
            confidence: 1.0,
        });

        let res = migration.apply(&mut dag);
        assert!(matches!(res, Err(DagError::CycleDetected { .. })));
    }
}
