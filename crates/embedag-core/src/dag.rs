//! In-memory directed acyclic graph arena backed by `petgraph::graph::DiGraph`.
//!
//! Traceability: `doc/05_In_Memory_DAG_and_Arena_Topology.md` and
//! `doc/13_Topological_Cycle_Detection_and_Guards.md`.

use std::collections::HashMap;

pub use petgraph::Direction;
use petgraph::algo::{has_path_connecting, tarjan_scc};
use petgraph::graph::DiGraph;
pub use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::visit::{Bfs, EdgeRef, Reversed};

use crate::error::DagError;
use crate::model::EdgePayload;
use crate::traits::NodePayload;

pub use crate::dag_iter::{AncestorsIter, DescendantsIter, TraversalIter};
pub use crate::pagination::{NeighborPage, NeighborPageRequest};

/// In-memory DAG arena with secondary O(1) hash index for fast key-based lookups.
///
/// Wraps a [`petgraph::graph::DiGraph`] where edges point from parent to child
/// (`parent -> child`).
#[derive(Clone)]
pub struct MemoryDag<T: NodePayload> {
    graph: DiGraph<T, EdgePayload>,
    key_to_index: HashMap<T::Key, NodeIndex>,
    revision: u64,
}

impl<T: NodePayload + std::fmt::Debug> std::fmt::Debug for MemoryDag<T>
where
    T::Key: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryDag")
            .field("node_count", &self.graph.node_count())
            .field("edge_count", &self.graph.edge_count())
            .field("revision", &self.revision)
            .field("keys", &self.key_to_index.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl<T: NodePayload> Default for MemoryDag<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Defines, in a `MemoryDag` impl, the unbounded and depth-bounded iterator methods of each
/// traversal direction.
macro_rules! traversal_methods {
    ($(
        $(#[$unbounded_doc:meta])* $unbounded:ident,
        $(#[$bounded_doc:meta])* $bounded:ident,
        $iter:ident, $dir:ident;
    )*) => {$(
        $(#[$unbounded_doc])*
        #[must_use]
        pub fn $unbounded<'a>(&'a self, start_key: &T::Key) -> $iter<'a, T> {
            self.traversal(start_key, Direction::$dir, None)
        }

        $(#[$bounded_doc])*
        #[must_use]
        pub fn $bounded<'a>(&'a self, start_key: &T::Key, max_depth: usize) -> $iter<'a, T> {
            self.traversal(start_key, Direction::$dir, Some(max_depth))
        }
    )*};
}

impl<T: NodePayload> MemoryDag<T> {
    /// Creates a new, empty in-memory DAG at revision 0.
    #[must_use]
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            key_to_index: HashMap::new(),
            revision: 0,
        }
    }

    /// Creates an empty DAG with pre-allocated capacities.
    #[must_use]
    pub fn with_capacity(nodes: usize, edges: usize) -> Self {
        Self {
            graph: DiGraph::with_capacity(nodes, edges),
            key_to_index: HashMap::with_capacity(nodes),
            revision: 0,
        }
    }

    /// Returns the current monotonic revision number of the DAG.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the number of nodes stored in the arena.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Returns the number of directed edges stored in the arena.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Returns `true` if the graph contains no nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.graph.node_count() == 0
    }

    /// Returns `true` if a node with the given business key exists.
    #[must_use]
    pub fn contains_key(&self, key: &T::Key) -> bool {
        self.key_to_index.contains_key(key)
    }

    /// Returns the internal [`NodeIndex`] corresponding to the business key.
    #[must_use]
    pub fn node_index(&self, key: &T::Key) -> Option<NodeIndex> {
        self.key_to_index.get(key).copied()
    }

    /// Inserts or updates a node payload. Returns the stable [`NodeIndex`].
    ///
    /// If a node with the same business key already exists, its payload is
    /// overwritten in place without creating a duplicate graph node.
    pub fn upsert_node(&mut self, payload: T) -> NodeIndex {
        self.revision += 1;
        let key = payload.key().clone();
        if let Some(&idx) = self.key_to_index.get(&key) {
            self.graph[idx] = payload;
            idx
        } else {
            let idx = self.graph.add_node(payload);
            self.key_to_index.insert(key, idx);
            idx
        }
    }

    /// Returns a reference to the node payload with the specified business key.
    #[must_use]
    pub fn node(&self, key: &T::Key) -> Option<&T> {
        let idx = self.node_index(key)?;
        self.graph.node_weight(idx)
    }

    /// Returns a mutable reference to the node payload with the specified business key.
    pub fn node_mut(&mut self, key: &T::Key) -> Option<&mut T> {
        let idx = self.node_index(key)?;
        self.graph.node_weight_mut(idx)
    }

    /// Returns a reference to the node payload at the given [`NodeIndex`].
    #[must_use]
    pub fn node_by_index(&self, idx: NodeIndex) -> Option<&T> {
        self.graph.node_weight(idx)
    }

    /// Returns a mutable reference to the node payload at the given [`NodeIndex`].
    pub fn node_by_index_mut(&mut self, idx: NodeIndex) -> Option<&mut T> {
        self.graph.node_weight_mut(idx)
    }

    /// Adds a directed edge from `parent_idx` to `child_idx`.
    pub fn add_edge(
        &mut self,
        parent_idx: NodeIndex,
        child_idx: NodeIndex,
        edge: EdgePayload,
    ) -> EdgeIndex {
        self.revision += 1;
        self.graph.add_edge(parent_idx, child_idx, edge)
    }

    /// Adds a directed edge connecting `parent` to `child` by their business keys.
    ///
    /// # Errors
    /// Returns [`DagError::NodeNotFound`] if either parent or child does not exist in the DAG.
    pub fn add_edge_by_keys(
        &mut self,
        parent: &T::Key,
        child: &T::Key,
        edge: EdgePayload,
    ) -> Result<EdgeIndex, DagError> {
        let p_idx =
            self.key_to_index
                .get(parent)
                .copied()
                .ok_or_else(|| DagError::NodeNotFound {
                    key: parent.to_string(),
                })?;
        let c_idx =
            self.key_to_index
                .get(child)
                .copied()
                .ok_or_else(|| DagError::NodeNotFound {
                    key: child.to_string(),
                })?;
        self.revision += 1;
        Ok(self.graph.add_edge(p_idx, c_idx, edge))
    }

    /// Removes an edge connecting `parent` to `child`, optionally filtered by relationship type.
    ///
    /// Returns `true` if an edge was found and removed, `false` otherwise.
    pub fn remove_edge_by_keys(
        &mut self,
        parent: &T::Key,
        child: &T::Key,
        relationship: Option<&str>,
    ) -> bool {
        let (Some(&p_idx), Some(&c_idx)) =
            (self.key_to_index.get(parent), self.key_to_index.get(child))
        else {
            return false;
        };

        let mut found = None;
        for edge_ref in self.graph.edges_connecting(p_idx, c_idx) {
            if let Some(rel) = relationship {
                if edge_ref.weight().relationship == rel {
                    found = Some(edge_ref.id());
                    break;
                }
            } else {
                found = Some(edge_ref.id());
                break;
            }
        }

        if let Some(e_idx) = found {
            self.revision += 1;
            self.graph.remove_edge(e_idx);
            true
        } else {
            false
        }
    }

    /// Checks if adding a directed edge from `from_key` to `to_key` would create a cycle.
    ///
    /// Returns `true` if:
    /// - `from_key == to_key` (self-loop), or
    /// - a directed path already exists from `to_key` to `from_key`.
    #[must_use]
    pub fn would_create_cycle(&self, from_key: &T::Key, to_key: &T::Key) -> bool {
        if from_key == to_key {
            return true;
        }
        let (Some(&from_idx), Some(&to_idx)) = (
            self.key_to_index.get(from_key),
            self.key_to_index.get(to_key),
        ) else {
            return false;
        };
        self.would_create_cycle_indices(from_idx, to_idx)
    }

    /// Checks if adding a directed edge from `from_idx` to `to_idx` would create a cycle.
    #[must_use]
    pub fn would_create_cycle_indices(&self, from_idx: NodeIndex, to_idx: NodeIndex) -> bool {
        if from_idx == to_idx {
            return true;
        }
        has_path_connecting(&self.graph, to_idx, from_idx, None)
    }

    /// Adds a directed edge with pre-flight cycle validation.
    ///
    /// # Errors
    /// Returns [`DagError::CycleDetected`] if adding this edge would create a cycle.
    pub fn add_edge_guarded(
        &mut self,
        parent_idx: NodeIndex,
        child_idx: NodeIndex,
        edge: EdgePayload,
    ) -> Result<EdgeIndex, DagError> {
        if self.would_create_cycle_indices(parent_idx, child_idx) {
            return Err(DagError::CycleDetected {
                node_indices: vec![parent_idx.index(), child_idx.index()],
            });
        }
        Ok(self.add_edge(parent_idx, child_idx, edge))
    }

    /// Adds a directed edge by business keys with pre-flight cycle validation.
    ///
    /// # Errors
    /// - Returns [`DagError::NodeNotFound`] if parent or child key is missing.
    /// - Returns [`DagError::CycleDetected`] if adding this edge would create a cycle.
    pub fn add_edge_by_keys_guarded(
        &mut self,
        parent: &T::Key,
        child: &T::Key,
        edge: EdgePayload,
    ) -> Result<EdgeIndex, DagError> {
        let p_idx =
            self.key_to_index
                .get(parent)
                .copied()
                .ok_or_else(|| DagError::NodeNotFound {
                    key: parent.to_string(),
                })?;
        let c_idx =
            self.key_to_index
                .get(child)
                .copied()
                .ok_or_else(|| DagError::NodeNotFound {
                    key: child.to_string(),
                })?;

        if self.would_create_cycle_indices(p_idx, c_idx) {
            return Err(DagError::CycleDetected {
                node_indices: vec![p_idx.index(), c_idx.index()],
            });
        }
        self.revision += 1;
        Ok(self.graph.add_edge(p_idx, c_idx, edge))
    }

    /// Validates that the entire graph is strictly acyclic using Tarjan's Strongly Connected Components algorithm.
    ///
    /// # Errors
    /// Returns [`DagError::CycleDetected`] containing the node indices of any cycle participants.
    pub fn validate_acyclic(&self) -> Result<(), DagError> {
        let sccs = tarjan_scc(&self.graph);
        for scc in sccs {
            if scc.len() > 1 {
                return Err(DagError::CycleDetected {
                    node_indices: scc.iter().map(|n| n.index()).collect(),
                });
            }
            if scc.len() == 1 {
                let single = scc[0];
                if self.graph.contains_edge(single, single) {
                    return Err(DagError::CycleDetected {
                        node_indices: vec![single.index()],
                    });
                }
            }
        }
        Ok(())
    }

    /// Returns `true` if the DAG is currently acyclic.
    #[must_use]
    pub fn is_acyclic(&self) -> bool {
        self.validate_acyclic().is_ok()
    }

    /// Returns the edge payload for the given [`EdgeIndex`].
    #[must_use]
    pub fn edge_weight(&self, edge: EdgeIndex) -> Option<&EdgePayload> {
        self.graph.edge_weight(edge)
    }

    /// Retrieves direct parents of a given node key.
    #[must_use]
    pub fn parents_of(&self, key: &T::Key) -> Vec<&T> {
        self.neighbours_of(key, Direction::Incoming)
    }

    /// Retrieves direct children of a given node key.
    #[must_use]
    pub fn children_of(&self, key: &T::Key) -> Vec<&T> {
        self.neighbours_of(key, Direction::Outgoing)
    }

    /// The payloads one edge away from `key` in direction `dir`.
    fn neighbours_of(&self, key: &T::Key, dir: Direction) -> Vec<&T> {
        let Some(&idx) = self.key_to_index.get(key) else {
            return Vec::new();
        };
        self.graph
            .neighbors_directed(idx, dir)
            .filter_map(|n_idx| self.graph.node_weight(n_idx))
            .collect()
    }

    /// The payloads of the nodes with no edge in direction `dir`.
    fn nodes_without(&self, dir: Direction) -> Vec<&T> {
        self.graph
            .node_indices()
            .filter(|&idx| self.graph.neighbors_directed(idx, dir).next().is_none())
            .filter_map(|idx| self.graph.node_weight(idx))
            .collect()
    }

    /// A traversal from the node `key` names, along `dir`, bounded by `max_depth` when given.
    fn traversal(
        &self,
        key: &T::Key,
        dir: Direction,
        max_depth: Option<usize>,
    ) -> TraversalIter<'_, T> {
        let idx = self.key_to_index.get(key).copied();
        TraversalIter::new(&self.graph, idx, dir, max_depth)
    }

    /// Returns the node indices of all immediate parents of `idx`.
    #[must_use]
    pub fn parent_indices(&self, idx: NodeIndex) -> Vec<NodeIndex> {
        self.graph
            .neighbors_directed(idx, Direction::Incoming)
            .collect()
    }

    /// Returns the node indices of all immediate children of `idx`.
    #[must_use]
    pub fn child_indices(&self, idx: NodeIndex) -> Vec<NodeIndex> {
        self.graph
            .neighbors_directed(idx, Direction::Outgoing)
            .collect()
    }

    /// Collects all transitive ancestors using Breadth-First Search (BFS).
    #[must_use]
    pub fn transitive_ancestors(&self, start_key: &T::Key) -> Vec<&T> {
        let Some(&start_idx) = self.key_to_index.get(start_key) else {
            return Vec::new();
        };
        let reversed = Reversed(&self.graph);
        let mut bfs = Bfs::new(&reversed, start_idx);
        let mut ancestors = Vec::new();
        while let Some(nx) = bfs.next(&reversed) {
            if nx == start_idx {
                continue;
            }
            if let Some(node) = self.graph.node_weight(nx) {
                ancestors.push(node);
            }
        }
        ancestors
    }

    /// Collects all transitive descendants using Breadth-First Search (BFS).
    #[must_use]
    pub fn transitive_descendants(&self, start_key: &T::Key) -> Vec<&T> {
        let Some(&start_idx) = self.key_to_index.get(start_key) else {
            return Vec::new();
        };
        let mut bfs = Bfs::new(&self.graph, start_idx);
        let mut descendants = Vec::new();
        while let Some(nx) = bfs.next(&self.graph) {
            if nx == start_idx {
                continue;
            }
            if let Some(node) = self.graph.node_weight(nx) {
                descendants.push(node);
            }
        }
        descendants
    }

    traversal_methods! {
        /// Returns a pull-based streaming iterator over transitive ancestors in BFS order.
        ancestors_iter,
        /// Returns a pull-based streaming iterator over transitive ancestors bounded by `max_depth`.
        ancestors_iter_bounded,
        AncestorsIter, Incoming;
        /// Returns a pull-based streaming iterator over transitive descendants in BFS order.
        descendants_iter,
        /// Returns a pull-based streaming iterator over transitive descendants bounded by `max_depth`.
        descendants_iter_bounded,
        DescendantsIter, Outgoing;
    }

    /// Returns a paginated slice of adjacent neighbors for a node.
    #[must_use]
    pub fn paginate_neighbors<'a>(
        &'a self,
        req: &NeighborPageRequest<'_, T::Key>,
    ) -> NeighborPage<'a, T> {
        let Some(&idx) = self.key_to_index.get(req.key) else {
            return NeighborPage {
                items: Vec::new(),
                total_count: 0,
                next_offset: None,
                has_more: false,
            };
        };

        let mut matches = Vec::new();
        for edge_ref in self.graph.edges_directed(idx, req.direction) {
            let edge = edge_ref.weight();
            if let Some(rel) = req.relationship
                && edge.relationship != rel
            {
                continue;
            }
            let target_idx = match req.direction {
                Direction::Incoming => edge_ref.source(),
                Direction::Outgoing => edge_ref.target(),
            };
            if let Some(payload) = self.graph.node_weight(target_idx) {
                matches.push((payload, edge));
            }
        }

        let total_count = matches.len();
        let page_items: Vec<(&'a T, &'a EdgePayload)> = matches
            .into_iter()
            .skip(req.offset)
            .take(req.limit)
            .collect();

        let has_more = req.offset + page_items.len() < total_count;
        let next_offset = if has_more {
            Some(req.offset + page_items.len())
        } else {
            None
        };

        NeighborPage {
            items: page_items,
            total_count,
            next_offset,
            has_more,
        }
    }

    /// Returns all root nodes in the DAG (nodes with in-degree 0).
    #[must_use]
    pub fn roots(&self) -> Vec<&T> {
        self.nodes_without(Direction::Incoming)
    }

    /// Returns all leaf nodes in the DAG (nodes with out-degree 0).
    #[must_use]
    pub fn leaves(&self) -> Vec<&T> {
        self.nodes_without(Direction::Outgoing)
    }

    /// Returns a shared reference to the underlying [`DiGraph`].
    #[must_use]
    pub fn inner(&self) -> &DiGraph<T, EdgePayload> {
        &self.graph
    }

    /// Clears the DAG, removing all nodes and edges and bumping the revision.
    pub fn clear(&mut self) {
        self.revision += 1;
        self.graph.clear();
        self.key_to_index.clear();
    }

    /// Serializes this DAG into a Mermaid flowchart string (`graph TD`).
    #[must_use]
    pub fn to_mermaid(&self) -> String {
        crate::export::GraphExporter::to_mermaid(self)
    }

    /// Serializes this DAG into a Graphviz DOT string (`digraph G`).
    #[must_use]
    pub fn to_dot(&self) -> String {
        crate::export::GraphExporter::to_dot(self)
    }

    /// Serializes this DAG into a GraphML XML string.
    #[must_use]
    pub fn to_graphml(&self) -> String {
        crate::export::GraphExporter::to_graphml(self)
    }

    /// Serializes this DAG into a W3C JSON-LD JSON value.
    #[must_use]
    pub fn to_json_ld(&self) -> serde_json::Value {
        crate::export::GraphExporter::to_json_ld(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ProvenanceKind;
    use crate::testing::TestNode;

    fn test_edge(rel: &str) -> EdgePayload {
        EdgePayload::new(rel, 1.0, ProvenanceKind::DeterministicRule).unwrap()
    }

    #[test]
    fn test_insert_and_lookup_by_key() {
        let mut dag = MemoryDag::new();
        assert!(dag.is_empty());
        assert_eq!(dag.node_count(), 0);
        assert_eq!(dag.revision(), 0);

        let node = TestNode::new("SKU-100", "Organic Apple", "Product");
        let idx = dag.upsert_node(node.clone());

        assert_eq!(dag.node_count(), 1);
        assert_eq!(dag.revision(), 1);
        assert!(!dag.is_empty());
        assert!(dag.contains_key(&"SKU-100".to_string()));
        assert_eq!(dag.node_index(&"SKU-100".to_string()), Some(idx));

        let retrieved = dag.node(&"SKU-100".to_string()).unwrap();
        assert_eq!(retrieved.name(), "Organic Apple");
        assert_eq!(retrieved.entity_level(), "Product");

        let by_idx = dag.node_by_index(idx).unwrap();
        assert_eq!(by_idx.key(), "SKU-100");
    }

    #[test]
    fn test_upsert_updates_payload_without_duplicate_index() {
        let mut dag = MemoryDag::new();
        let initial = TestNode::new("SKU-100", "Organic Apple", "Product");
        let idx1 = dag.upsert_node(initial);

        let updated = TestNode::new("SKU-100", "Fuji Organic Apple", "Product");
        let idx2 = dag.upsert_node(updated);

        assert_eq!(idx1, idx2, "index must remain stable across updates");
        assert_eq!(dag.node_count(), 1);
        assert_eq!(
            dag.node(&"SKU-100".to_string()).unwrap().name(),
            "Fuji Organic Apple"
        );
    }

    #[test]
    fn test_multi_parent_edge_insertion() {
        let mut dag = MemoryDag::new();
        let cat_food = TestNode::new("CAT-FOOD", "Food & Beverage", "Category");
        let cat_organic = TestNode::new("CAT-ORGANIC", "Certified Organic", "Taxonomy");
        let prod_apple = TestNode::new("PROD-APPLE", "Honeycrisp Apple", "Product");

        dag.upsert_node(cat_food);
        dag.upsert_node(cat_organic);
        dag.upsert_node(prod_apple);

        let e1 = dag
            .add_edge_by_keys(
                &"CAT-FOOD".to_string(),
                &"PROD-APPLE".to_string(),
                test_edge("contains"),
            )
            .expect("first parent edge should succeed");
        let e2 = dag
            .add_edge_by_keys(
                &"CAT-ORGANIC".to_string(),
                &"PROD-APPLE".to_string(),
                test_edge("certified_by"),
            )
            .expect("second parent edge should succeed");

        assert_eq!(dag.edge_count(), 2);
        assert_eq!(dag.edge_weight(e1).unwrap().relationship, "contains");
        assert_eq!(dag.edge_weight(e2).unwrap().relationship, "certified_by");

        let parents = dag.parents_of(&"PROD-APPLE".to_string());
        assert_eq!(parents.len(), 2);
        let parent_keys: Vec<_> = parents.into_iter().map(|p| p.key().as_str()).collect();
        assert!(parent_keys.contains(&"CAT-FOOD"));
        assert!(parent_keys.contains(&"CAT-ORGANIC"));

        let food_children = dag.children_of(&"CAT-FOOD".to_string());
        assert_eq!(food_children.len(), 1);
        assert_eq!(food_children[0].key(), "PROD-APPLE");
    }

    #[test]
    fn test_add_edge_by_keys_missing_nodes() {
        let mut dag = MemoryDag::new();
        let cat = TestNode::new("CAT-1", "Root Category", "Category");
        dag.upsert_node(cat);

        let err_missing_child = dag.add_edge_by_keys(
            &"CAT-1".to_string(),
            &"MISSING".to_string(),
            test_edge("rel"),
        );
        assert!(matches!(
            err_missing_child,
            Err(DagError::NodeNotFound { key }) if key == "MISSING"
        ));

        let err_missing_parent = dag.add_edge_by_keys(
            &"MISSING".to_string(),
            &"CAT-1".to_string(),
            test_edge("rel"),
        );
        assert!(matches!(
            err_missing_parent,
            Err(DagError::NodeNotFound { key }) if key == "MISSING"
        ));
    }

    #[test]
    fn test_streaming_traversal_iterators_and_early_termination() {
        let mut dag = MemoryDag::new();
        let a = TestNode::new("A", "Root", "L0");
        let b = TestNode::new("B", "Left", "L1");
        let c = TestNode::new("C", "Right", "L1");
        let d = TestNode::new("D", "Bottom", "L2");

        dag.upsert_node(a);
        dag.upsert_node(b);
        dag.upsert_node(c);
        dag.upsert_node(d);

        dag.add_edge_by_keys(&"A".to_string(), &"B".to_string(), test_edge("sub"))
            .unwrap();
        dag.add_edge_by_keys(&"A".to_string(), &"C".to_string(), test_edge("sub"))
            .unwrap();
        dag.add_edge_by_keys(&"B".to_string(), &"D".to_string(), test_edge("sub"))
            .unwrap();
        dag.add_edge_by_keys(&"C".to_string(), &"D".to_string(), test_edge("sub"))
            .unwrap();

        let ancestors: Vec<_> = dag.ancestors_iter(&"D".to_string()).collect();
        assert_eq!(ancestors.len(), 3);

        let first_only: Vec<_> = dag.ancestors_iter(&"D".to_string()).take(1).collect();
        assert_eq!(first_only.len(), 1);

        let descendants: Vec<_> = dag.descendants_iter(&"A".to_string()).collect();
        assert_eq!(descendants.len(), 3);

        let depth_one_desc: Vec<_> = dag.descendants_iter_bounded(&"A".to_string(), 1).collect();
        assert_eq!(depth_one_desc.len(), 2);
    }

    #[test]
    fn test_paginate_neighbors() {
        let mut dag = MemoryDag::new();
        let root = TestNode::new("ROOT", "Root Category", "L0");
        dag.upsert_node(root);

        for i in 1..=10 {
            let id = format!("CHILD-{}", i);
            let child = TestNode::new(&id, &id, "L1");
            dag.upsert_node(child);
            let rel = if i % 2 == 0 { "even" } else { "odd" };
            dag.add_edge_by_keys(&"ROOT".to_string(), &id, test_edge(rel))
                .unwrap();
        }

        let root_key = "ROOT".to_string();

        let req1 = NeighborPageRequest::children(&root_key, 0, 4);
        let page1 = dag.paginate_neighbors(&req1);
        assert_eq!(page1.total_count, 10);
        assert_eq!(page1.items.len(), 4);
        assert!(page1.has_more);
        assert_eq!(page1.next_offset, Some(4));

        let req2 = NeighborPageRequest::children(&root_key, 4, 4);
        let page2 = dag.paginate_neighbors(&req2);
        assert_eq!(page2.items.len(), 4);
        assert!(page2.has_more);
        assert_eq!(page2.next_offset, Some(8));

        let req3 = NeighborPageRequest::children(&root_key, 8, 4);
        let page3 = dag.paginate_neighbors(&req3);
        assert_eq!(page3.items.len(), 2);
        assert!(!page3.has_more);
        assert_eq!(page3.next_offset, None);

        let req_odd = NeighborPageRequest::children(&root_key, 0, 10).with_relationship("odd");
        let page_odd = dag.paginate_neighbors(&req_odd);
        assert_eq!(page_odd.total_count, 5);
        assert_eq!(page_odd.items.len(), 5);
    }

    #[test]
    fn test_transitive_ancestors_and_descendants() {
        let mut dag = MemoryDag::new();
        // A -> B -> D
        // A -> C -> D (Diamond)
        let a = TestNode::new("A", "Root", "L0");
        let b = TestNode::new("B", "Left", "L1");
        let c = TestNode::new("C", "Right", "L1");
        let d = TestNode::new("D", "Bottom", "L2");

        dag.upsert_node(a);
        dag.upsert_node(b);
        dag.upsert_node(c);
        dag.upsert_node(d);

        dag.add_edge_by_keys(&"A".to_string(), &"B".to_string(), test_edge("sub"))
            .unwrap();
        dag.add_edge_by_keys(&"A".to_string(), &"C".to_string(), test_edge("sub"))
            .unwrap();
        dag.add_edge_by_keys(&"B".to_string(), &"D".to_string(), test_edge("sub"))
            .unwrap();
        dag.add_edge_by_keys(&"C".to_string(), &"D".to_string(), test_edge("sub"))
            .unwrap();

        let ancestors = dag.transitive_ancestors(&"D".to_string());
        assert_eq!(ancestors.len(), 3);
        let anc_keys: Vec<_> = ancestors.into_iter().map(|n| n.key().as_str()).collect();
        assert!(anc_keys.contains(&"A"));
        assert!(anc_keys.contains(&"B"));
        assert!(anc_keys.contains(&"C"));

        let descendants = dag.transitive_descendants(&"A".to_string());
        assert_eq!(descendants.len(), 3);
        let desc_keys: Vec<_> = descendants.into_iter().map(|n| n.key().as_str()).collect();
        assert!(desc_keys.contains(&"B"));
        assert!(desc_keys.contains(&"C"));
        assert!(desc_keys.contains(&"D"));

        // Roots and leaves
        let roots = dag.roots();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].key(), "A");

        let leaves = dag.leaves();
        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].key(), "D");
    }

    #[test]
    fn test_direct_cycle_detection() {
        let mut dag = MemoryDag::new();
        dag.upsert_node(TestNode::new("A", "A", "L0"));
        dag.upsert_node(TestNode::new("B", "B", "L0"));

        dag.add_edge_by_keys(&"A".to_string(), &"B".to_string(), test_edge("rel"))
            .unwrap();

        // Adding B -> A would create a cycle
        assert!(dag.would_create_cycle(&"B".to_string(), &"A".to_string()));
        let res =
            dag.add_edge_by_keys_guarded(&"B".to_string(), &"A".to_string(), test_edge("reverse"));
        assert!(matches!(res, Err(DagError::CycleDetected { .. })));
        assert_eq!(dag.edge_count(), 1);
    }

    #[test]
    fn test_indirect_cycle_detection() {
        let mut dag = MemoryDag::new();
        dag.upsert_node(TestNode::new("A", "A", "L0"));
        dag.upsert_node(TestNode::new("B", "B", "L0"));
        dag.upsert_node(TestNode::new("C", "C", "L0"));

        dag.add_edge_by_keys(&"A".to_string(), &"B".to_string(), test_edge("rel"))
            .unwrap();
        dag.add_edge_by_keys(&"B".to_string(), &"C".to_string(), test_edge("rel"))
            .unwrap();

        // Adding C -> A would close an indirect 3-node cycle
        assert!(dag.would_create_cycle(&"C".to_string(), &"A".to_string()));
        let res =
            dag.add_edge_by_keys_guarded(&"C".to_string(), &"A".to_string(), test_edge("cycle"));
        assert!(matches!(res, Err(DagError::CycleDetected { .. })));
    }

    #[test]
    fn test_self_loop_detection() {
        let mut dag = MemoryDag::new();
        dag.upsert_node(TestNode::new("A", "A", "L0"));

        assert!(dag.would_create_cycle(&"A".to_string(), &"A".to_string()));
        let res = dag.add_edge_by_keys_guarded(
            &"A".to_string(),
            &"A".to_string(),
            test_edge("self_loop"),
        );
        assert!(matches!(res, Err(DagError::CycleDetected { .. })));
    }

    #[test]
    fn test_multi_parent_acyclic_valid() {
        let mut dag = MemoryDag::new();
        // Diamond: A -> B -> D, A -> C -> D
        dag.upsert_node(TestNode::new("A", "A", "L0"));
        dag.upsert_node(TestNode::new("B", "B", "L1"));
        dag.upsert_node(TestNode::new("C", "C", "L1"));
        dag.upsert_node(TestNode::new("D", "D", "L2"));

        assert!(!dag.would_create_cycle(&"A".to_string(), &"B".to_string()));
        dag.add_edge_by_keys_guarded(&"A".to_string(), &"B".to_string(), test_edge("rel"))
            .unwrap();

        assert!(!dag.would_create_cycle(&"A".to_string(), &"C".to_string()));
        dag.add_edge_by_keys_guarded(&"A".to_string(), &"C".to_string(), test_edge("rel"))
            .unwrap();

        assert!(!dag.would_create_cycle(&"B".to_string(), &"D".to_string()));
        dag.add_edge_by_keys_guarded(&"B".to_string(), &"D".to_string(), test_edge("rel"))
            .unwrap();

        assert!(!dag.would_create_cycle(&"C".to_string(), &"D".to_string()));
        dag.add_edge_by_keys_guarded(&"C".to_string(), &"D".to_string(), test_edge("rel"))
            .unwrap();

        assert!(dag.is_acyclic());
        assert!(dag.validate_acyclic().is_ok());
    }

    #[test]
    fn test_validate_acyclic_detects_existing_cycle() {
        let mut dag = MemoryDag::new();
        let a = dag.upsert_node(TestNode::new("A", "A", "L0"));
        let b = dag.upsert_node(TestNode::new("B", "B", "L0"));

        // Force cycle via unguarded add_edge
        dag.add_edge(a, b, test_edge("forward"));
        dag.add_edge(b, a, test_edge("backward"));

        assert!(!dag.is_acyclic());
        let res = dag.validate_acyclic();
        assert!(
            matches!(res, Err(DagError::CycleDetected { node_indices }) if node_indices.len() == 2)
        );
    }

    #[test]
    fn test_remove_edge_by_keys() {
        let mut dag = MemoryDag::new();
        dag.upsert_node(TestNode::new("A", "A", "L0"));
        dag.upsert_node(TestNode::new("B", "B", "L0"));

        dag.add_edge_by_keys(&"A".to_string(), &"B".to_string(), test_edge("rel1"))
            .unwrap();
        assert_eq!(dag.edge_count(), 1);

        assert!(!dag.remove_edge_by_keys(
            &"A".to_string(),
            &"B".to_string(),
            Some("different_rel")
        ));
        assert_eq!(dag.edge_count(), 1);

        assert!(dag.remove_edge_by_keys(&"A".to_string(), &"B".to_string(), Some("rel1")));
        assert_eq!(dag.edge_count(), 0);
    }

    #[test]
    fn test_memory_dag_visual_export_convenience() {
        let mut dag = MemoryDag::new();
        dag.upsert_node(TestNode::new("A", "Alpha", "L0"));
        dag.upsert_node(TestNode::new("B", "Beta", "L1"));
        dag.add_edge_by_keys(&"A".to_string(), &"B".to_string(), test_edge("contains"))
            .unwrap();

        let mermaid = dag.to_mermaid();
        assert!(mermaid.starts_with("graph TD\n"));
        assert!(mermaid.contains("Alpha"));

        let dot = dag.to_dot();
        assert!(dot.starts_with("digraph G {\n"));

        let graphml = dag.to_graphml();
        assert!(graphml.contains("<graphml xmlns="));

        let json_ld = dag.to_json_ld();
        assert!(json_ld["@graph"].as_array().is_some());
    }
}
