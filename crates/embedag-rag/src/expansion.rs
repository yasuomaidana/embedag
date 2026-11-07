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

    fn edge(rel: &str) -> EdgePayload {
        EdgePayload::new(rel, 1.0, ProvenanceKind::DeterministicRule).unwrap()
    }

    #[test]
    fn test_subgraph_expansion_with_decay() {
        let mut dag = MemoryDag::new();

        // Hierarchy: Dept (L1) -> Brand (L2) -> SKU_A (L3)
        //                                   \-> SKU_B (L3, sibling)
        // SKU_A -> Variant_A1 (L4, child)
        let dept = TestNode::new("dept", "Confectionery", "Department");
        let brand = TestNode::new("brand", "Carlos V", "Brand");
        let sku_a = TestNode::new("sku_a", "Carlos V Dark 50g", "SKU");
        let sku_b = TestNode::new("sku_b", "Carlos V Milk 50g", "SKU");
        let variant = TestNode::new("variant", "Carlos V Dark 50g 10-Pack", "Bundle");

        dag.upsert_node(dept);
        dag.upsert_node(brand);
        dag.upsert_node(sku_a);
        dag.upsert_node(sku_b);
        dag.upsert_node(variant);

        dag.add_edge_by_keys(&"dept".into(), &"brand".into(), edge("PARENT_OF"))
            .unwrap();
        dag.add_edge_by_keys(&"brand".into(), &"sku_a".into(), edge("PARENT_OF"))
            .unwrap();
        dag.add_edge_by_keys(&"brand".into(), &"sku_b".into(), edge("PARENT_OF"))
            .unwrap();
        dag.add_edge_by_keys(&"sku_a".into(), &"variant".into(), edge("HAS_VARIANT"))
            .unwrap();

        let expander = SubgraphExpander {
            decay_gamma: 0.70,
            max_hops: 2,
            include_siblings: true,
        };

        let seed_candidates = vec![("sku_a".to_string(), 1.0)];
        let subgraph = expander.expand(&dag, &seed_candidates);

        // 1. Seed
        let seed_node = subgraph.nodes.get("sku_a").unwrap();
        assert_eq!(seed_node.hop_distance, 0);
        assert_eq!(seed_node.weight, 1.0);
        assert_eq!(seed_node.relationship, SeedRelationship::Seed);

        // 2. 1-hop Ancestor (brand)
        let brand_node = subgraph.nodes.get("brand").unwrap();
        assert_eq!(brand_node.hop_distance, 1);
        assert!((brand_node.weight - 0.70).abs() < 1e-4);
        assert_eq!(brand_node.relationship, SeedRelationship::Ancestor);

        // 3. 2-hop Ancestor (dept)
        let dept_node = subgraph.nodes.get("dept").unwrap();
        assert_eq!(dept_node.hop_distance, 2);
        assert!((dept_node.weight - 0.49).abs() < 1e-4);
        assert_eq!(dept_node.relationship, SeedRelationship::Ancestor);

        // 4. 1-hop Descendant (variant)
        let var_node = subgraph.nodes.get("variant").unwrap();
        assert_eq!(var_node.hop_distance, 1);
        assert!((var_node.weight - 0.70).abs() < 1e-4);
        assert_eq!(var_node.relationship, SeedRelationship::Descendant);

        // 5. 1-hop Sibling (sku_b)
        let sib_node = subgraph.nodes.get("sku_b").unwrap();
        assert_eq!(sib_node.hop_distance, 1);
        assert_eq!(sib_node.relationship, SeedRelationship::Sibling);

        // 6. Edges captured
        assert!(!subgraph.edges.is_empty());
    }
}
