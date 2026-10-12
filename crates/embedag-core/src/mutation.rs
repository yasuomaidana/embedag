//! Typestate mutation plans and atomic graph modification directives.
//!
//! Traceability: `doc/03_Architectural_Design_Patterns.md` §2.3 and
//! `doc/13_Topological_Cycle_Detection_and_Guards.md` §3.

use std::fmt;
use std::marker::PhantomData;

use crate::dag::MemoryDag;
use crate::error::DagError;
use crate::model::EdgePayload;
use crate::traits::NodePayload;

/// Typestate indicating a mutation plan has not yet been verified against graph cycle invariants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unverified;

/// Typestate indicating a mutation plan has successfully passed Tarjan SCC / reachability shadow validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CycleValidated;

/// Typestate indicating a mutation plan has been committed to the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Committed;

/// An atomic graph modification directive.
#[derive(Clone, PartialEq)]
pub enum MutationDirective<T: NodePayload> {
    /// Inserts or updates a node.
    UpsertNode(T),
    /// Adds a directed edge from parent to child.
    AddEdge {
        /// Parent node business key.
        parent: T::Key,
        /// Child node business key.
        child: T::Key,
        /// Edge metadata and confidence.
        edge: EdgePayload,
    },
    /// Removes a directed edge between parent and child, optionally filtered by relationship type.
    RemoveEdge {
        /// Parent node business key.
        parent: T::Key,
        /// Child node business key.
        child: T::Key,
        /// Optional relationship label filter.
        relationship: Option<String>,
    },
}

impl<T: NodePayload + fmt::Debug> fmt::Debug for MutationDirective<T>
where
    T::Key: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UpsertNode(node) => f.debug_tuple("UpsertNode").field(node).finish(),
            Self::AddEdge {
                parent,
                child,
                edge,
            } => f
                .debug_struct("AddEdge")
                .field("parent", parent)
                .field("child", child)
                .field("edge", edge)
                .finish(),
            Self::RemoveEdge {
                parent,
                child,
                relationship,
            } => f
                .debug_struct("RemoveEdge")
                .field("parent", parent)
                .field("child", child)
                .field("relationship", relationship)
                .finish(),
        }
    }
}

/// A batch of mutation directives governed by compile-time typestate progression.
///
/// Unverified plans cannot execute `.apply()` against a [`MemoryDag`]; they must first
/// be promoted to [`CycleValidated`] via [`MutationPlan::validate_cycles`].
#[derive(Clone)]
pub struct MutationPlan<T: NodePayload, State = Unverified> {
    directives: Vec<MutationDirective<T>>,
    validated_revision: Option<u64>,
    _state: PhantomData<State>,
}

impl<T: NodePayload + fmt::Debug, State> fmt::Debug for MutationPlan<T, State>
where
    T::Key: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MutationPlan")
            .field("directives", &self.directives)
            .field("validated_revision", &self.validated_revision)
            .finish()
    }
}

impl<T: NodePayload> Default for MutationPlan<T, Unverified> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: NodePayload> MutationPlan<T, Unverified> {
    /// Creates a new, empty unverified mutation plan.
    #[must_use]
    pub fn new() -> Self {
        Self {
            directives: Vec::new(),
            validated_revision: None,
            _state: PhantomData,
        }
    }

    /// Appends an upsert node directive to the plan.
    #[must_use]
    pub fn upsert_node(mut self, node: T) -> Self {
        self.directives.push(MutationDirective::UpsertNode(node));
        self
    }

    /// Appends an add edge directive to the plan.
    #[must_use]
    pub fn add_edge(mut self, parent: T::Key, child: T::Key, edge: EdgePayload) -> Self {
        self.directives.push(MutationDirective::AddEdge {
            parent,
            child,
            edge,
        });
        self
    }

    /// Appends a remove edge directive to the plan.
    #[must_use]
    pub fn remove_edge(
        mut self,
        parent: T::Key,
        child: T::Key,
        relationship: Option<String>,
    ) -> Self {
        self.directives.push(MutationDirective::RemoveEdge {
            parent,
            child,
            relationship,
        });
        self
    }

    /// Appends a raw directive to the plan.
    #[must_use]
    pub fn add_directive(mut self, directive: MutationDirective<T>) -> Self {
        self.directives.push(directive);
        self
    }

    /// Simulates the mutation batch on a clone (shadow arena) of the target DAG.
    ///
    /// If no cycles are detected, transitions the plan to the [`CycleValidated`] typestate.
    ///
    /// # Errors
    /// - [`DagError::CycleDetected`]: If proposed mutations would create a directed loop.
    /// - [`DagError::NodeNotFound`]: If an edge references a nonexistent node not introduced in this batch.
    pub fn validate_cycles(
        self,
        dag: &MemoryDag<T>,
    ) -> Result<MutationPlan<T, CycleValidated>, DagError> {
        let mut shadow = dag.clone();
        let current_rev = dag.revision();

        for directive in &self.directives {
            match directive {
                MutationDirective::UpsertNode(node) => {
                    shadow.upsert_node(node.clone());
                }
                MutationDirective::AddEdge {
                    parent,
                    child,
                    edge,
                } => {
                    shadow.add_edge_by_keys(parent, child, edge.clone())?;
                }
                MutationDirective::RemoveEdge {
                    parent,
                    child,
                    relationship,
                } => {
                    shadow.remove_edge_by_keys(parent, child, relationship.as_deref());
                }
            }
        }

        shadow.validate_acyclic()?;

        Ok(MutationPlan {
            directives: self.directives,
            validated_revision: Some(current_rev),
            _state: PhantomData,
        })
    }
}

impl<T: NodePayload> MutationPlan<T, CycleValidated> {
    /// Applies the cycle-validated mutation plan directly to the target [`MemoryDag`].
    ///
    /// # Errors
    /// - [`DagError::StalePlan`]: If the target DAG has been modified (revision bumped) since validation.
    /// - [`DagError::NodeNotFound`]: If an edge target is missing.
    pub fn apply(self, dag: &mut MemoryDag<T>) -> Result<MutationPlan<T, Committed>, DagError> {
        if let Some(rev) = self.validated_revision.filter(|&rev| rev != dag.revision()) {
            return Err(DagError::StalePlan {
                validated: rev,
                current: dag.revision(),
            });
        }

        for directive in &self.directives {
            match directive {
                MutationDirective::UpsertNode(node) => {
                    dag.upsert_node(node.clone());
                }
                MutationDirective::AddEdge {
                    parent,
                    child,
                    edge,
                } => {
                    dag.add_edge_by_keys(parent, child, edge.clone())?;
                }
                MutationDirective::RemoveEdge {
                    parent,
                    child,
                    relationship,
                } => {
                    dag.remove_edge_by_keys(parent, child, relationship.as_deref());
                }
            }
        }

        Ok(MutationPlan {
            directives: self.directives,
            validated_revision: Some(dag.revision()),
            _state: PhantomData,
        })
    }
}

impl<T: NodePayload, State> MutationPlan<T, State> {
    /// Returns the slice of mutation directives in this plan.
    #[must_use]
    pub fn directives(&self) -> &[MutationDirective<T>] {
        &self.directives
    }

    /// Returns the number of directives in this plan.
    #[must_use]
    pub fn len(&self) -> usize {
        self.directives.len()
    }

    /// Returns `true` if this plan contains zero directives.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.directives.is_empty()
    }

    /// Returns the DAG revision captured at validation time, if validated.
    #[must_use]
    pub fn validated_revision(&self) -> Option<u64> {
        self.validated_revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ProvenanceKind;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct MockItem {
        id: String,
        label: String,
    }

    impl MockItem {
        fn new(id: &str, label: &str) -> Self {
            Self {
                id: id.to_string(),
                label: label.to_string(),
            }
        }
    }

    impl NodePayload for MockItem {
        type Key = String;

        fn key(&self) -> &Self::Key {
            &self.id
        }

        fn name(&self) -> &str {
            &self.label
        }

        fn entity_level(&self) -> &str {
            "Item"
        }
    }

    fn test_edge(rel: &str) -> EdgePayload {
        EdgePayload::new(rel, 1.0, ProvenanceKind::DeterministicRule).unwrap()
    }

    #[test]
    fn test_plan_validation_success_and_failure() {
        let mut dag = MemoryDag::new();
        dag.upsert_node(MockItem::new("A", "Node A"));
        dag.upsert_node(MockItem::new("B", "Node B"));
        dag.add_edge_by_keys(&"A".to_string(), &"B".to_string(), test_edge("rel"))
            .unwrap();

        // Valid plan: Add C, connect B -> C
        let plan_valid = MutationPlan::new()
            .upsert_node(MockItem::new("C", "Node C"))
            .add_edge("B".to_string(), "C".to_string(), test_edge("rel"));

        let validated = plan_valid
            .validate_cycles(&dag)
            .expect("acyclic extension must pass");
        assert_eq!(validated.len(), 2);

        // Cyclic plan: Connect B -> A (closes A -> B -> A)
        let plan_cyclic =
            MutationPlan::new().add_edge("B".to_string(), "A".to_string(), test_edge("rev"));
        let err = plan_cyclic.validate_cycles(&dag).unwrap_err();
        assert!(matches!(err, DagError::CycleDetected { .. }));
    }

    #[test]
    fn test_validated_plan_apply_succeeds() {
        let mut dag = MemoryDag::new();
        dag.upsert_node(MockItem::new("A", "Node A"));

        let plan = MutationPlan::new()
            .upsert_node(MockItem::new("B", "Node B"))
            .add_edge("A".to_string(), "B".to_string(), test_edge("parent_of"));

        let validated = plan.validate_cycles(&dag).unwrap();
        let committed = validated.apply(&mut dag).unwrap();

        assert_eq!(committed.len(), 2);
        assert_eq!(dag.node_count(), 2);
        assert_eq!(dag.edge_count(), 1);
        assert_eq!(dag.children_of(&"A".to_string())[0].key(), "B");
    }

    #[test]
    fn test_stale_plan_rejection() {
        let mut dag = MemoryDag::new();
        dag.upsert_node(MockItem::new("A", "Node A"));

        let plan = MutationPlan::new().upsert_node(MockItem::new("B", "Node B"));
        let validated = plan.validate_cycles(&dag).unwrap();

        // Concurrently mutate the DAG before committing plan
        dag.upsert_node(MockItem::new("CONCURRENT", "Concurrent Node"));

        // Attempting to apply stale plan must fail with StalePlan
        let res = validated.apply(&mut dag);
        assert!(matches!(
            res,
            Err(DagError::StalePlan {
                validated: 1,
                current: 2
            })
        ));
    }
}
