//! Pull-based streaming traversal iterators over [`petgraph::graph::DiGraph`].
//!
//! Enables zero-eager-allocation lazy consumption and early short-circuit termination (e.g. `.take(N)`).
//! Traceability: `doc/21_Heavy_Graph_Handling_and_Topological_Pagination.md`.

use std::collections::{HashSet, VecDeque};

use petgraph::Direction;
use petgraph::graph::{DiGraph, NodeIndex};

use crate::model::EdgePayload;
use crate::traits::NodePayload;

/// Pull-based streaming traversal iterator over a DAG in Breadth-First Search (BFS) order.
///
/// Unlike eager methods that allocate the entire transitive closure in a `Vec<&T>`,
/// [`TraversalIter`] advances lazily across graph levels, yielding nodes on demand.
pub struct TraversalIter<'a, T: NodePayload> {
    graph: &'a DiGraph<T, EdgePayload>,
    direction: Direction,
    queue: VecDeque<(NodeIndex, usize)>,
    visited: HashSet<NodeIndex>,
    max_depth: Option<usize>,
}

impl<'a, T: NodePayload> TraversalIter<'a, T> {
    /// Creates a new streaming traversal iterator starting from `start_idx`.
    #[must_use]
    pub fn new(
        graph: &'a DiGraph<T, EdgePayload>,
        start_idx: Option<NodeIndex>,
        direction: Direction,
        max_depth: Option<usize>,
    ) -> Self {
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();

        if let Some(idx) = start_idx {
            queue.push_back((idx, 0));
            visited.insert(idx);
        }

        Self {
            graph,
            direction,
            queue,
            visited,
            max_depth,
        }
    }

    /// Fetches the next node alongside its relative hop depth from the starting seed node.
    pub fn next_with_depth(&mut self) -> Option<(&'a T, usize)> {
        while let Some((curr_idx, depth)) = self.queue.pop_front() {
            let can_expand = self.max_depth.is_none_or(|md| depth < md);
            if can_expand {
                for neighbor_idx in self.graph.neighbors_directed(curr_idx, self.direction) {
                    if self.visited.insert(neighbor_idx) {
                        self.queue.push_back((neighbor_idx, depth + 1));
                    }
                }
            }

            // Depth 0 is the start node itself; we only yield transitive descendants/ancestors
            if depth > 0
                && let Some(payload) = self.graph.node_weight(curr_idx)
            {
                return Some((payload, depth));
            }
        }
        None
    }
}

impl<'a, T: NodePayload> Iterator for TraversalIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_with_depth().map(|(node, _)| node)
    }
}

/// Iterator yielding transitive ancestors in pull-based streaming BFS order.
pub type AncestorsIter<'a, T> = TraversalIter<'a, T>;

/// Iterator yielding transitive descendants in pull-based streaming BFS order.
pub type DescendantsIter<'a, T> = TraversalIter<'a, T>;
