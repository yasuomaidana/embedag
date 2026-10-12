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
        let mut visited: HashMap<NodeIndex, (usize, f32, SeedRelationship)> = HashMap::new();
        let mut seeds = Vec::new();

        // Direction indicator for BFS exploration
        #[derive(Clone, Copy)]
        enum Dir {
            Both,
            AncestorsOnly,
            DescendantsOnly,
        }

        let mut queue: VecDeque<(NodeIndex, usize, f32, Dir)> = VecDeque::new();

        for (seed_key, weight) in seed_candidates {
            if let Some(idx) = dag.node_index(seed_key) {
                seeds.push(seed_key.clone());
                visited.insert(idx, (0, *weight, SeedRelationship::Seed));
                queue.push_back((idx, 0, *weight, Dir::Both));
            }
        }

        while let Some((curr_idx, curr_hops, base_weight, dir)) = queue.pop_front() {
            if curr_hops >= self.max_hops {
                continue;
            }

            let next_hops = curr_hops + 1;
            let next_weight = self.compute_weight(base_weight, next_hops);

            // 1. Ancestors (parents)
            if matches!(dir, Dir::Both | Dir::AncestorsOnly) {
                for parent_idx in dag.parent_indices(curr_idx) {
                    let should_visit = match visited.get(&parent_idx) {
                        Some(&(existing_hops, existing_w, _)) => {
                            next_hops < existing_hops
                                || (next_hops == existing_hops && next_weight > existing_w)
                        }
                        None => true,
                    };

                    if should_visit {
                        visited.insert(
                            parent_idx,
                            (next_hops, next_weight, SeedRelationship::Ancestor),
                        );
                        queue.push_back((parent_idx, next_hops, base_weight, Dir::AncestorsOnly));
                    }

                    // 1-Hop Sibling expansion: other children of direct parent
                    if self.include_siblings && curr_hops == 0 {
                        let sib_weight = next_weight * 0.70; // attenuated sibling factor
                        for sib_idx in dag.child_indices(parent_idx) {
                            if sib_idx != curr_idx && !visited.contains_key(&sib_idx) {
                                visited.insert(sib_idx, (1, sib_weight, SeedRelationship::Sibling));
                            }
                        }
                    }
                }
            }

            // 2. Descendants (children)
            if matches!(dir, Dir::Both | Dir::DescendantsOnly) {
                for child_idx in dag.child_indices(curr_idx) {
                    let should_visit = match visited.get(&child_idx) {
                        Some(&(existing_hops, existing_w, _)) => {
                            next_hops < existing_hops
                                || (next_hops == existing_hops && next_weight > existing_w)
                        }
                        None => true,
                    };

                    if should_visit {
                        visited.insert(
                            child_idx,
                            (next_hops, next_weight, SeedRelationship::Descendant),
                        );
                        queue.push_back((child_idx, next_hops, base_weight, Dir::DescendantsOnly));
                    }
                }
            }
        }

        // Map collected NodeIndices back to business keys and build nodes map
        let mut nodes: HashMap<T::Key, ExpandedNode<T::Key>> = HashMap::new();
        for (&idx, &(hop_distance, weight, relationship)) in &visited {
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

        // Collect all edges connecting visited nodes
        let mut edges = Vec::new();
        for edge in dag.inner().edge_references() {
            let src = edge.source();
            let target = edge.target();
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

        ExpandedSubgraph {
            seeds,
            nodes,
            edges,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedag_core::model::EdgePayload;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct ProductNode {
        id: String,
        name: String,
        level: String,
    }

    impl ProductNode {
        fn new(id: &str, name: &str, level: &str) -> Self {
            Self {
                id: id.into(),
                name: name.into(),
                level: level.into(),
            }
        }
    }

    impl NodePayload for ProductNode {
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

    fn edge(rel: &str) -> EdgePayload {
        EdgePayload::new(rel, 1.0, ProvenanceKind::DeterministicRule).unwrap()
    }

    #[test]
    fn test_subgraph_expansion_with_decay() {
        let mut dag = MemoryDag::new();

        // Hierarchy: Dept (L1) -> Brand (L2) -> SKU_A (L3)
        //                                   \-> SKU_B (L3, sibling)
        // SKU_A -> Variant_A1 (L4, child)
        let dept = ProductNode::new("dept", "Confectionery", "Department");
        let brand = ProductNode::new("brand", "Carlos V", "Brand");
        let sku_a = ProductNode::new("sku_a", "Carlos V Dark 50g", "SKU");
        let sku_b = ProductNode::new("sku_b", "Carlos V Milk 50g", "SKU");
        let variant = ProductNode::new("variant", "Carlos V Dark 50g 10-Pack", "Bundle");

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
