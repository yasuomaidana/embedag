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
                    ensure_acyclic(dag, new_parent_key, child_key)?;
                    let move_edge = Reroute {
                        from: old_parent_key,
                        to: new_parent_key,
                        child: child_key,
                        relationship,
                        confidence: *confidence,
                    };
                    move_edge.apply(dag, &mut directives)?;
                }
                MigrationStep::SplitCategory {
                    source_category,
                    new_categories,
                    child_routes,
                    relationship,
                } => split_category(
                    dag,
                    &mut directives,
                    (source_category, new_categories, child_routes),
                    relationship,
                )?,
                MigrationStep::MergeNodes {
                    sources,
                    target,
                    relationship,
                } => merge_nodes(dag, &mut directives, sources, target, relationship)?,
            }
        }

        // Final topological consistency validation
        dag.validate_acyclic()?;

        Ok(directives)
    }
}

/// Moves the `relationship` edge of `child` from one parent to another.
struct Reroute<'a, K> {
    from: &'a K,
    to: &'a K,
    child: &'a K,
    relationship: &'a str,
    confidence: f32,
}

impl<K: Clone> Reroute<'_, K> {
    /// Removes the old edge and adds the new one, recording both as directives.
    fn apply<T: NodePayload<Key = K>>(
        &self,
        dag: &mut MemoryDag<T>,
        directives: &mut Vec<MutationDirective<T>>,
    ) -> Result<(), DagError> {
        dag.remove_edge_by_keys(self.from, self.child, Some(self.relationship));
        directives.push(MutationDirective::RemoveEdge {
            parent: self.from.clone(),
            child: self.child.clone(),
            relationship: Some(self.relationship.to_string()),
        });

        let edge = EdgePayload::new(
            self.relationship,
            self.confidence,
            ProvenanceKind::DeterministicRule,
        )?;
        dag.add_edge_by_keys(self.to, self.child, edge.clone())?;
        directives.push(MutationDirective::AddEdge {
            parent: self.to.clone(),
            child: self.child.clone(),
            edge,
        });
        Ok(())
    }
}

/// Fails when making `parent` the parent of `child` would close a loop.
fn ensure_acyclic<T: NodePayload>(
    dag: &MemoryDag<T>,
    parent: &T::Key,
    child: &T::Key,
) -> Result<(), DagError> {
    if dag.would_create_cycle(parent, child) {
        return Err(DagError::CycleDetected {
            node_indices: vec![],
        });
    }
    Ok(())
}

/// Inserts the new categories, then routes each child from the source category to its target.
fn split_category<T: NodePayload>(
    dag: &mut MemoryDag<T>,
    directives: &mut Vec<MutationDirective<T>>,
    (source, new_categories, child_routes): (&T::Key, &[T], &[(T::Key, T::Key)]),
    relationship: &str,
) -> Result<(), DagError> {
    for new_cat in new_categories {
        dag.upsert_node(new_cat.clone());
        directives.push(MutationDirective::UpsertNode(new_cat.clone()));
    }
    for (child_key, target_cat_key) in child_routes {
        ensure_acyclic(dag, target_cat_key, child_key)?;
        Reroute {
            from: source,
            to: target_cat_key,
            child: child_key,
            relationship,
            confidence: 1.0,
        }
        .apply(dag, directives)?;
    }
    Ok(())
}

/// Inserts `target` and moves every child of every source node under it.
fn merge_nodes<T: NodePayload>(
    dag: &mut MemoryDag<T>,
    directives: &mut Vec<MutationDirective<T>>,
    sources: &[T::Key],
    target: &T,
    relationship: &str,
) -> Result<(), DagError> {
    dag.upsert_node(target.clone());
    directives.push(MutationDirective::UpsertNode(target.clone()));

    for source_key in sources {
        let children_keys: Vec<T::Key> = dag
            .children_of(source_key)
            .iter()
            .map(|c| c.key().clone())
            .collect();
        for c_key in children_keys {
            Reroute {
                from: source_key,
                to: target.key(),
                child: &c_key,
                relationship,
                confidence: 1.0,
            }
            .apply(dag, directives)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestNode;

    #[test]
    fn test_category_split_migration() {
        let mut dag = MemoryDag::new();

        // Initial setup: "Confectionery" contains "Carlos V Dark" and "Gummy Bears"
        let conf = TestNode::new("CAT-CONF", "Confectionery", "Category");
        let choco = TestNode::new("SKU-CHOCO", "Carlos V Dark 50g", "Product");
        let gummy = TestNode::new("SKU-GUMMY", "Gummy Bears 100g", "Product");

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
        let cat_chocolate = TestNode::new("CAT-CHOCO", "Chocolate", "Category");
        let cat_candy = TestNode::new("CAT-CANDY", "Candy & Gummies", "Category");

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
        let a = TestNode::new("A", "A", "L0");
        let b = TestNode::new("B", "B", "L1");
        let c = TestNode::new("C", "C", "L2");

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
