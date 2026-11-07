use embedag_core::dag::MemoryDag;
use embedag_core::model::ProvenanceKind;
use embedag_core::traits::NodePayload;
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use std::collections::{HashMap, VecDeque};

/// Topological relationship of an expanded node relative to the retrieved anchor seeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedRelationship {
    Seed,
    Ancestor,
    Descendant,
    Sibling,
}

/// Node captured during bounded topological subgraph expansion with distance decay.
#[derive(Debug, Clone, PartialEq)]
pub struct ExpandedNode<K> {
    pub key: K,
    pub hop_distance: usize,
    pub weight: f32,
    pub relationship: SeedRelationship,
}

/// Directed edge captured within the expanded subgraph.
#[derive(Debug, Clone, PartialEq)]
pub struct ExpandedEdge<K> {
    pub parent_key: K,
    pub child_key: K,
    pub relationship: String,
    pub confidence: f32,
    pub provenance: ProvenanceKind,
}

/// Fully bounded topological subgraph extracted around anchor seeds.
#[derive(Debug, Clone)]
pub struct ExpandedSubgraph<K> {
    pub seeds: Vec<K>,
    pub nodes: HashMap<K, ExpandedNode<K>>,
    pub edges: Vec<ExpandedEdge<K>>,
}

/// Stage 2 Topological Subgraph Expander with exponential distance decay.
#[derive(Debug, Clone)]
pub struct SubgraphExpander {
    pub decay_gamma: f32,
    pub max_hops: usize,
    pub include_siblings: bool,
}

impl Default for SubgraphExpander {
    fn default() -> Self {
        Self {
            decay_gamma: 0.70,
            max_hops: 2,
            include_siblings: true,
        }
    }
}

/// Whether a node reached at `hops`/`weight` improves on its recorded visit (or is unvisited).
fn should_visit(visited: &Visited, idx: NodeIndex, hops: usize, weight: f32) -> bool {
    match visited.get(&idx) {
        Some(&(existing_hops, existing_w, _)) => {
            hops < existing_hops || (hops == existing_hops && weight > existing_w)
        }
        None => true,
    }
}

impl SubgraphExpander {
    pub fn new(decay_gamma: f32, max_hops: usize, include_siblings: bool) -> Self {
        Self {
            decay_gamma,
            max_hops,
            include_siblings,
        }
    }

    /// Computes exponential distance decay weight for a given hop distance.
    pub fn compute_weight(&self, seed_weight: f32, hop_distance: usize) -> f32 {
        if hop_distance > self.max_hops {
            0.0
        } else {
            seed_weight * self.decay_gamma.powi(hop_distance as i32)
        }
    }

    /// Expands the graph around seed candidates up to `max_hops` with distance decay.
    pub fn expand<T: NodePayload>(
        &self,
        dag: &MemoryDag<T>,
        seed_candidates: &[(T::Key, f32)],
    ) -> ExpandedSubgraph<T::Key>
    where
        T::Key: std::fmt::Debug + std::hash::Hash + Eq + Clone,
    {
        let mut walk = Walk {
            expander: self,
            dag,
            visited: Visited::new(),
            queue: VecDeque::new(),
        };
        let seeds = walk.seed(seed_candidates);
        walk.run();
        ExpandedSubgraph {
            seeds,
            nodes: collect_nodes(dag, &walk.visited),
            edges: collect_edges(dag, &walk.visited),
        }
    }
}

/// Hops, weight and relationship recorded for every reached node.
type Visited = HashMap<NodeIndex, (usize, f32, SeedRelationship)>;

/// Direction indicator for BFS exploration.
#[derive(Clone, Copy)]
enum Dir {
    Both,
    AncestorsOnly,
    DescendantsOnly,
}

impl Dir {
    fn goes_up(self) -> bool {
        matches!(self, Self::Both | Self::AncestorsOnly)
    }

    fn goes_down(self) -> bool {
        matches!(self, Self::Both | Self::DescendantsOnly)
    }
}

/// A node waiting in the BFS queue.
struct Frontier {
    idx: NodeIndex,
    hops: usize,
    base_weight: f32,
    dir: Dir,
}

/// How a neighbour is reached from the node being expanded.
struct Arrival {
    hops: usize,
    weight: f32,
    relationship: SeedRelationship,
    dir: Dir,
}

/// Attenuation of a sibling's weight relative to its parent's.
const SIBLING_FACTOR: f32 = 0.70;

/// The breadth-first walk around the seeds.
struct Walk<'a, T: NodePayload> {
    expander: &'a SubgraphExpander,
    dag: &'a MemoryDag<T>,
    visited: Visited,
    queue: VecDeque<Frontier>,
}

impl<T: NodePayload> Walk<'_, T> {
    /// Queues every seed that exists in the graph and returns their keys.
    fn seed(&mut self, candidates: &[(T::Key, f32)]) -> Vec<T::Key>
    where
        T::Key: Clone,
    {
        let mut seeds = Vec::new();
        for (key, weight) in candidates {
            if let Some(idx) = self.dag.node_index(key) {
                seeds.push(key.clone());
                self.visited
                    .insert(idx, (0, *weight, SeedRelationship::Seed));
                self.queue.push_back(Frontier {
                    idx,
                    hops: 0,
                    base_weight: *weight,
                    dir: Dir::Both,
                });
            }
        }
        seeds
    }

    fn run(&mut self) {
        while let Some(frontier) = self.queue.pop_front() {
            if frontier.hops < self.expander.max_hops {
                self.step(&frontier);
            }
        }
    }

    fn step(&mut self, frontier: &Frontier) {
        let hops = frontier.hops + 1;
        let weight = self.expander.compute_weight(frontier.base_weight, hops);
        if frontier.dir.goes_up() {
            self.climb(frontier, hops, weight);
        }
        if frontier.dir.goes_down() {
            self.descend(frontier, hops, weight);
        }
    }

    /// Visits the parents and, for a seed's direct parents, their other children.
    fn climb(&mut self, frontier: &Frontier, hops: usize, weight: f32) {
        let dag = self.dag;
        for parent in dag.parent_indices(frontier.idx) {
            let arrival = Arrival {
                hops,
                weight,
                relationship: SeedRelationship::Ancestor,
                dir: Dir::AncestorsOnly,
            };
            self.discover(parent, frontier.base_weight, &arrival);
            if self.expander.include_siblings && frontier.hops == 0 {
                self.add_siblings(parent, frontier.idx, weight * SIBLING_FACTOR);
            }
        }
    }

    fn descend(&mut self, frontier: &Frontier, hops: usize, weight: f32) {
        let dag = self.dag;
        for child in dag.child_indices(frontier.idx) {
            let arrival = Arrival {
                hops,
                weight,
                relationship: SeedRelationship::Descendant,
                dir: Dir::DescendantsOnly,
            };
            self.discover(child, frontier.base_weight, &arrival);
        }
    }

    /// Records and queues `idx` when this arrival improves on what is known.
    fn discover(&mut self, idx: NodeIndex, base_weight: f32, arrival: &Arrival) {
        if should_visit(&self.visited, idx, arrival.hops, arrival.weight) {
            self.visited
                .insert(idx, (arrival.hops, arrival.weight, arrival.relationship));
            self.queue.push_back(Frontier {
                idx,
                hops: arrival.hops,
                base_weight,
                dir: arrival.dir,
            });
        }
    }

    /// 1-hop sibling expansion: the other, still unvisited, children of a direct parent.
    fn add_siblings(&mut self, parent: NodeIndex, origin: NodeIndex, weight: f32) {
        let dag = self.dag;
        for sibling in dag.child_indices(parent) {
            if sibling != origin && !self.visited.contains_key(&sibling) {
                self.visited
                    .insert(sibling, (1, weight, SeedRelationship::Sibling));
            }
        }
    }
}

/// Maps the collected node indices back to business keys.
fn collect_nodes<T: NodePayload>(
    dag: &MemoryDag<T>,
    visited: &Visited,
) -> HashMap<T::Key, ExpandedNode<T::Key>>
where
    T::Key: std::hash::Hash + Eq + Clone,
{
    let mut nodes = HashMap::new();
    for (&idx, &(hop_distance, weight, relationship)) in visited {
        if let Some(payload) = dag.node_by_index(idx) {
            let key = payload.key().clone();
            nodes.insert(
                key.clone(),
                ExpandedNode {
                    key,
                    hop_distance,
                    weight,
                    relationship,
                },
            );
        }
    }
    nodes
}

/// Every edge connecting two visited nodes.
fn collect_edges<T: NodePayload>(dag: &MemoryDag<T>, visited: &Visited) -> Vec<ExpandedEdge<T::Key>>
where
    T::Key: Clone,
{
    let mut edges = Vec::new();
    for edge in dag.inner().edge_references() {
        let (src, target) = (edge.source(), edge.target());
        if visited.contains_key(&src)
            && visited.contains_key(&target)
            && let (Some(p_node), Some(c_node)) =
                (dag.node_by_index(src), dag.node_by_index(target))
        {
            let weight = edge.weight();
            edges.push(ExpandedEdge {
                parent_key: p_node.key().clone(),
                child_key: c_node.key().clone(),
                relationship: weight.relationship.clone(),
                confidence: weight.confidence,
                provenance: weight.provenance,
            });
        }
    }
    edges
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedag_core::model::EdgePayload;
    use embedag_core::testing::TestNode;
    use rstest::{fixture, rstest};

    fn edge(rel: &str) -> EdgePayload {
        EdgePayload::new(rel, 1.0, ProvenanceKind::DeterministicRule).unwrap()
    }

    /// Hierarchy: Dept (L1) -> Brand (L2) -> SKU_A (L3)
    ///                                   \-> SKU_B (L3, sibling)
    /// SKU_A -> Variant_A1 (L4, child)
    #[fixture]
    fn dag() -> MemoryDag<TestNode> {
        let mut dag = MemoryDag::new();
        for (key, name, level) in [
            ("dept", "Confectionery", "Department"),
            ("brand", "Carlos V", "Brand"),
            ("sku_a", "Carlos V Dark 50g", "SKU"),
            ("sku_b", "Carlos V Milk 50g", "SKU"),
            ("variant", "Carlos V Dark 50g 10-Pack", "Bundle"),
        ] {
            dag.upsert_node(TestNode::new(key, name, level));
        }
        for (parent, child, rel) in [
            ("dept", "brand", "PARENT_OF"),
            ("brand", "sku_a", "PARENT_OF"),
            ("brand", "sku_b", "PARENT_OF"),
            ("sku_a", "variant", "HAS_VARIANT"),
        ] {
            dag.add_edge_by_keys(&parent.into(), &child.into(), edge(rel))
                .unwrap();
        }
        dag
    }

    #[fixture]
    fn subgraph(dag: MemoryDag<TestNode>) -> ExpandedSubgraph<String> {
        let expander = SubgraphExpander {
            decay_gamma: 0.70,
            max_hops: 2,
            include_siblings: true,
        };
        expander.expand(&dag, &[("sku_a".to_string(), 1.0)])
    }

    #[rstest]
    #[case::seed("sku_a", 0, Some(1.0), SeedRelationship::Seed)]
    #[case::one_hop_ancestor("brand", 1, Some(0.70), SeedRelationship::Ancestor)]
    #[case::two_hop_ancestor("dept", 2, Some(0.49), SeedRelationship::Ancestor)]
    #[case::one_hop_descendant("variant", 1, Some(0.70), SeedRelationship::Descendant)]
    #[case::one_hop_sibling("sku_b", 1, None, SeedRelationship::Sibling)]
    fn expands_around_the_seed(
        subgraph: ExpandedSubgraph<String>,
        #[case] key: &str,
        #[case] hops: usize,
        #[case] weight: Option<f32>,
        #[case] relationship: SeedRelationship,
    ) {
        let node = subgraph.nodes.get(key).unwrap();
        assert_eq!(node.hop_distance, hops);
        assert_eq!(node.relationship, relationship);
        if let Some(expected) = weight {
            assert!((node.weight - expected).abs() < 1e-4);
        }
    }

    #[rstest]
    fn captures_the_edges_between_visited_nodes(subgraph: ExpandedSubgraph<String>) {
        assert!(!subgraph.edges.is_empty());
    }
}
