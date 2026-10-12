//! In-memory topological community detection for macro-level routing.
//!
//! Traceability: `doc/01_Development_Plan.md` § Ticket 6.3.1.

use embedag_core::dag::MemoryDag;
use embedag_core::traits::NodePayload;
use petgraph::Direction;
use petgraph::graph::NodeIndex;
use std::collections::{HashMap, HashSet, VecDeque};

/// Partition of nodes into topologically coherent communities.
#[derive(Debug, Clone)]
pub struct CommunityPartition<K> {
    pub node_to_community: HashMap<K, usize>,
    pub community_to_nodes: HashMap<usize, Vec<K>>,
}

impl<K: std::hash::Hash + Eq + Clone> CommunityPartition<K> {
    /// Returns the number of distinct communities discovered.
    #[must_use]
    pub fn community_count(&self) -> usize {
        self.community_to_nodes.len()
    }

    /// Looks up the community ID for a given node key.
    pub fn community_of(&self, key: &K) -> Option<usize> {
        self.node_to_community.get(key).copied()
    }

    /// Returns all member node keys belonging to the given community ID.
    pub fn members(&self, community_id: usize) -> Option<&[K]> {
        self.community_to_nodes
            .get(&community_id)
            .map(|v| v.as_slice())
    }
}

/// Community detector identifying topological clusters and connected subgraphs over [`MemoryDag`].
#[derive(Debug, Default, Clone)]
pub struct CommunityDetector;

impl CommunityDetector {
    pub fn new() -> Self {
        Self
    }

    /// Detects communities across nodes in the DAG via topological component clustering.
    pub fn detect<T: NodePayload>(&self, dag: &MemoryDag<T>) -> CommunityPartition<T::Key>
    where
        T::Key: std::hash::Hash + Eq + Clone + Ord,
    {
        let inner = dag.inner();
        let mut visited: HashSet<NodeIndex> = HashSet::new();
        let mut node_to_community = HashMap::new();
        let mut community_to_nodes: HashMap<usize, Vec<T::Key>> = HashMap::new();

        let mut community_counter = 0;

        // Sort indices for deterministic community ID assignment
        let mut all_indices: Vec<NodeIndex> = inner.node_indices().collect();
        all_indices.sort_by_key(|n| n.index());

        for start_idx in all_indices {
            if visited.contains(&start_idx) {
                continue;
            }

            let community_id = community_counter;
            community_counter += 1;

            let mut queue = VecDeque::new();
            queue.push_back(start_idx);
            visited.insert(start_idx);

            while let Some(curr) = queue.pop_front() {
                if let Some(payload) = inner.node_weight(curr) {
                    let key = payload.key().clone();
                    node_to_community.insert(key.clone(), community_id);
                    community_to_nodes
                        .entry(community_id)
                        .or_default()
                        .push(key);
                }

                // Traverse both incoming (parents) and outgoing (children) for undirected cluster reachability
                for p_idx in inner.neighbors_directed(curr, Direction::Incoming) {
                    if visited.insert(p_idx) {
                        queue.push_back(p_idx);
                    }
                }
                for c_idx in inner.neighbors_directed(curr, Direction::Outgoing) {
                    if visited.insert(c_idx) {
                        queue.push_back(c_idx);
                    }
                }
            }
        }

        CommunityPartition {
            node_to_community,
            community_to_nodes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedag_core::model::{EdgePayload, ProvenanceKind};

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct MockClusterNode {
        id: String,
        label: String,
    }

    impl MockClusterNode {
        fn new(id: &str, label: &str) -> Self {
            Self {
                id: id.into(),
                label: label.into(),
            }
        }
    }

    impl NodePayload for MockClusterNode {
        type Key = String;
        fn key(&self) -> &Self::Key {
            &self.id
        }
        fn name(&self) -> &str {
            &self.label
        }
        fn entity_level(&self) -> &str {
            "ClusterItem"
        }
    }

    fn edge() -> EdgePayload {
        EdgePayload::new("CONNECTS", 1.0, ProvenanceKind::DeterministicRule).unwrap()
    }

    #[test]
    fn test_community_clustering_accuracy() {
        let mut dag = MemoryDag::new();

        // Community 1: Beverage cluster (B1 -> B2 -> B3)
        let b1 = MockClusterNode::new("B1", "Beverages");
        let b2 = MockClusterNode::new("B2", "Coffee");
        let b3 = MockClusterNode::new("B3", "Espresso Beans");
        dag.upsert_node(b1);
        dag.upsert_node(b2);
        dag.upsert_node(b3);
        dag.add_edge_by_keys(&"B1".into(), &"B2".into(), edge())
            .unwrap();
        dag.add_edge_by_keys(&"B2".into(), &"B3".into(), edge())
            .unwrap();

        // Community 2: Apparel cluster (A1 -> A2 -> A3)
        let a1 = MockClusterNode::new("A1", "Apparel");
        let a2 = MockClusterNode::new("A2", "Outerwear");
        let a3 = MockClusterNode::new("A3", "Winter Jacket");
        dag.upsert_node(a1);
        dag.upsert_node(a2);
        dag.upsert_node(a3);
        dag.add_edge_by_keys(&"A1".into(), &"A2".into(), edge())
            .unwrap();
        dag.add_edge_by_keys(&"A2".into(), &"A3".into(), edge())
            .unwrap();

        let detector = CommunityDetector::new();
        let partition = detector.detect(&dag);

        assert_eq!(
            partition.community_count(),
            2,
            "Must detect exactly 2 isolated communities"
        );

        let b1_comm = partition.community_of(&"B1".to_string()).unwrap();
        let b2_comm = partition.community_of(&"B2".to_string()).unwrap();
        let b3_comm = partition.community_of(&"B3".to_string()).unwrap();
        assert_eq!(b1_comm, b2_comm);
        assert_eq!(b2_comm, b3_comm);

        let a1_comm = partition.community_of(&"A1".to_string()).unwrap();
        let a2_comm = partition.community_of(&"A2".to_string()).unwrap();
        let a3_comm = partition.community_of(&"A3".to_string()).unwrap();
        assert_eq!(a1_comm, a2_comm);
        assert_eq!(a2_comm, a3_comm);

        assert_ne!(
            b1_comm, a1_comm,
            "Beverage and Apparel communities must be distinct"
        );
    }
}
