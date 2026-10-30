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
            apply_directive(&mut shadow, directive)?;
        }

        shadow.validate_acyclic()?;

        Ok(MutationPlan {
            directives: self.directives,
            validated_revision: Some(current_rev),
            _state: PhantomData,
        })
    }

    /// Performs localized differential reachability cycle validation without cloning the entire DAG arena.
    ///
    /// Ideal for heavy graphs ($10^5+$ nodes) where cloning the global arena incurs high latency and memory churn.
    ///
    /// # Errors
    /// - [`DagError::CycleDetected`]: If proposed mutations would create a directed loop.
    /// - [`DagError::NodeNotFound`]: If an edge references a nonexistent node not introduced in this batch.
    pub fn validate_cycles_differential(
        self,
        dag: &MemoryDag<T>,
    ) -> Result<MutationPlan<T, CycleValidated>, DagError> {
        let current_rev = dag.revision();

        // 1. Collect all keys introduced via UpsertNode in this batch
        let mut introduced_keys = std::collections::HashSet::new();
        for dir in &self.directives {
            if let MutationDirective::UpsertNode(node) = dir {
                introduced_keys.insert(node.key().clone());
            }
        }

        // 2. Track removed edges in this batch
        let mut removed_edges = std::collections::HashSet::new();
        for dir in &self.directives {
            if let MutationDirective::RemoveEdge { parent, child, .. } = dir {
                removed_edges.insert((parent.clone(), child.clone()));
            }
        }

        // 3. Track newly added edges in this batch
        let mut overlay_out: std::collections::HashMap<T::Key, Vec<T::Key>> =
            std::collections::HashMap::new();

        for directive in &self.directives {
            if let MutationDirective::AddEdge { parent, child, .. } = directive {
                let parent_exists = dag.contains_key(parent) || introduced_keys.contains(parent);
                if !parent_exists {
                    return Err(DagError::NodeNotFound {
                        key: parent.to_string(),
                    });
                }
                let child_exists = dag.contains_key(child) || introduced_keys.contains(child);
                if !child_exists {
                    return Err(DagError::NodeNotFound {
                        key: child.to_string(),
                    });
                }

                if parent == child {
                    return Err(DagError::CycleDetected {
                        node_indices: vec![],
                    });
                }

                if has_path_differential(dag, child, parent, &overlay_out, &removed_edges) {
                    return Err(DagError::CycleDetected {
                        node_indices: vec![],
                    });
                }

                overlay_out
                    .entry(parent.clone())
                    .or_default()
                    .push(child.clone());
            }
        }

        Ok(MutationPlan {
            directives: self.directives,
            validated_revision: Some(current_rev),
            _state: PhantomData,
        })
    }
}

fn has_path_differential<T: NodePayload>(
    dag: &MemoryDag<T>,
    from: &T::Key,
    to: &T::Key,
    overlay_out: &std::collections::HashMap<T::Key, Vec<T::Key>>,
    removed_edges: &std::collections::HashSet<(T::Key, T::Key)>,
) -> bool {
    let mut queue = std::collections::VecDeque::new();
    let mut visited = std::collections::HashSet::new();

    queue.push_back(from.clone());
    visited.insert(from.clone());

    while let Some(curr) = queue.pop_front() {
        if &curr == to {
            return true;
        }

        if let Some(_node) = dag.node(&curr) {
            for child_node in dag.children_of(&curr) {
                let child_key = child_node.key().clone();
                if removed_edges.contains(&(curr.clone(), child_key.clone())) {
                    continue;
                }
                if &child_key == to {
                    return true;
                }
                if visited.insert(child_key.clone()) {
                    queue.push_back(child_key);
                }
            }
        }

        if let Some(overlay_children) = overlay_out.get(&curr) {
            for child_key in overlay_children {
                if child_key == to {
                    return true;
                }
                if visited.insert(child_key.clone()) {
                    queue.push_back(child_key.clone());
                }
            }
        }
    }

    false
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
            apply_directive(dag, directive)?;
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

/// Applies one directive to `dag`.
fn apply_directive<T: NodePayload>(
    dag: &mut MemoryDag<T>,
    directive: &MutationDirective<T>,
) -> Result<(), DagError> {
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ProvenanceKind;
    use rstest::{fixture, rstest};

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

    crate::impl_node_payload!(
        MockItem,
        key = |n| &n.id,
        name = |n| &n.label,
        level = |_| "Item"
    );

    fn test_edge(rel: &str) -> EdgePayload {
        EdgePayload::new(rel, 1.0, ProvenanceKind::DeterministicRule).unwrap()
    }

    /// A -> B.
    #[fixture]
    fn chain() -> MemoryDag<MockItem> {
        let mut dag = MemoryDag::new();
        dag.upsert_node(MockItem::new("A", "Node A"));
        dag.upsert_node(MockItem::new("B", "Node B"));
        dag.add_edge_by_keys(&"A".to_string(), &"B".to_string(), test_edge("rel"))
            .unwrap();
        dag
    }

    type Validate =
        fn(MutationPlan<MockItem, Unverified>, &MemoryDag<MockItem>) -> Result<usize, DagError>;

    fn plain(
        plan: MutationPlan<MockItem, Unverified>,
        dag: &MemoryDag<MockItem>,
    ) -> Result<usize, DagError> {
        plan.validate_cycles(dag).map(|v| v.len())
    }

    fn differential(
        plan: MutationPlan<MockItem, Unverified>,
        dag: &MemoryDag<MockItem>,
    ) -> Result<usize, DagError> {
        plan.validate_cycles_differential(dag).map(|v| v.len())
    }

    #[rstest]
    fn both_validations_accept_an_extension_and_reject_a_cycle(
        chain: MemoryDag<MockItem>,
        #[values(plain, differential)] validate: Validate,
    ) {
        // Valid plan: Add C, connect B -> C
        let plan_valid = MutationPlan::new()
            .upsert_node(MockItem::new("C", "Node C"))
            .add_edge("B".to_string(), "C".to_string(), test_edge("rel"));
        assert_eq!(
            validate(plan_valid, &chain).expect("acyclic extension must pass"),
            2
        );

        // Cyclic plan: Connect B -> A (closes A -> B -> A)
        let plan_cyclic =
            MutationPlan::new().add_edge("B".to_string(), "A".to_string(), test_edge("rev"));
        let err = validate(plan_cyclic, &chain).unwrap_err();
        assert!(matches!(err, DagError::CycleDetected { .. }));
    }

    /// Adds C and D to the chain and connects the `edges` between existing and new nodes; the plan
    /// either holds `Some(nodes)` after validation or closes a cycle (`None`).
    #[rstest]
    #[case::batch_cycle(&[("B", "C"), ("C", "D"), ("D", "B")], None)]
    #[case::diamond(&[("A", "C"), ("B", "D"), ("C", "D")], Some(5))]
    fn differential_validation_handles_batches_and_diamonds(
        chain: MemoryDag<MockItem>,
        #[case] edges: &[(&str, &str)],
        #[case] expected: Option<usize>,
    ) {
        let plan = edges.iter().fold(
            MutationPlan::new()
                .upsert_node(MockItem::new("C", "Node C"))
                .upsert_node(MockItem::new("D", "Node D")),
            |plan, (parent, child)| {
                plan.add_edge(parent.to_string(), child.to_string(), test_edge("rel"))
            },
        );
        match (differential(plan, &chain), expected) {
            (Ok(len), Some(want)) => assert_eq!(len, want),
            (Err(DagError::CycleDetected { .. }), None) => {}
            (other, _) => panic!("unexpected validation result: {other:?}"),
        }
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
