//! Undirected similarity graphs and connected-component clustering over syntax trees.

use std::collections::{HashMap, HashSet, VecDeque};

use petgraph::graph::{NodeIndex, UnGraph};
use petgraph::visit::EdgeRef;

use crate::ast::Matcher;
use crate::tree::Tree;

/// Undirected graph of pairwise similarity scores (`f32`) between named syntax [`Tree`]s.
///
/// Similarity is symmetric and may contain cycles, so it is backed by [`petgraph::graph::UnGraph`]
/// rather than `MemoryDag`.
#[derive(Debug, Clone, Default)]
pub struct SimilarityGraph {
    graph: UnGraph<String, f32>,
    id_to_index: HashMap<String, NodeIndex>,
}

impl SimilarityGraph {
    /// Builds a [`SimilarityGraph`] by scoring all pairs in `items` and retaining edges with
    /// `score >= threshold`.
    #[must_use]
    pub fn build<M: Matcher>(items: &[(String, Tree)], matcher: &M, threshold: f32) -> Self {
        let mut graph = UnGraph::with_capacity(items.len(), 0);
        let mut id_to_index = HashMap::with_capacity(items.len());
        let mut indices = Vec::with_capacity(items.len());

        for (id, _) in items {
            let idx = graph.add_node(id.clone());
            id_to_index.insert(id.clone(), idx);
            indices.push(idx);
        }

        for i in 0..items.len() {
            for j in (i + 1)..items.len() {
                let s = matcher.score(&items[i].1, &items[j].1);
                if s >= threshold {
                    graph.add_edge(indices[i], indices[j], s);
                }
            }
        }

        Self { graph, id_to_index }
    }

    /// Returns a reference to the underlying [`UnGraph`].
    #[must_use]
    pub fn inner(&self) -> &UnGraph<String, f32> {
        &self.graph
    }

    /// Returns all neighbors of `id` paired with their similarity scores, sorted by
    /// score descending and then by neighbor ID ascending.
    #[must_use]
    pub fn neighbors(&self, id: &str) -> Vec<(String, f32)> {
        let Some(&idx) = self.id_to_index.get(id) else {
            return Vec::new();
        };

        let mut list = Vec::new();
        for edge in self.graph.edges(idx) {
            let other_idx = if edge.source() == idx {
                edge.target()
            } else {
                edge.source()
            };
            if let Some(other_id) = self.graph.node_weight(other_idx) {
                list.push((other_id.clone(), *edge.weight()));
            }
        }

        list.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        list
    }

    /// Computes connected components of the similarity graph.
    ///
    /// Each inner component vector is sorted lexicographically by ID, and the outer vector
    /// of components is also sorted lexicographically.
    #[must_use]
    pub fn clusters(&self) -> Vec<Vec<String>> {
        let mut visited: HashSet<NodeIndex> = HashSet::with_capacity(self.graph.node_count());
        let mut components: Vec<Vec<String>> = Vec::new();

        for start_idx in self.graph.node_indices() {
            if !visited.insert(start_idx) {
                continue;
            }
            let mut comp = Vec::new();
            let mut queue = VecDeque::new();
            queue.push_back(start_idx);

            while let Some(curr) = queue.pop_front() {
                if let Some(id) = self.graph.node_weight(curr) {
                    comp.push(id.clone());
                }
                for nb in self.graph.neighbors(curr) {
                    if visited.insert(nb) {
                        queue.push_back(nb);
                    }
                }
            }

            comp.sort();
            components.push(comp);
        }

        components.sort();
        components
    }
}
