//! Deterministic dependency resolution, topological sorting, parallel execution levels,
//! reverse dependency impact analysis, and critical path calculation for [`MemoryDag`].

use std::collections::{BTreeMap, HashMap, HashSet};

use petgraph::graph::NodeIndex;

use crate::dag::MemoryDag;
use crate::error::DagError;
use crate::traits::NodePayload;

/// Computes a deterministic topological ordering of all nodes in `dag` using Kahn's algorithm.
///
/// Edges follow the [`MemoryDag`] convention (`parent -> child`), so every parent appears
/// before all of its children in the returned vector. Ties among ready nodes are broken
/// deterministically by the [`Display`](std::fmt::Display) string order of `T::Key`.
///
/// # Errors
/// Returns [`DagError::CycleDetected`] containing the cycle participant node indices if
/// the graph is not acyclic.
pub fn toposort<T: NodePayload>(dag: &MemoryDag<T>) -> Result<Vec<T::Key>, DagError> {
    let node_count = dag.node_count();
    if node_count == 0 {
        return Ok(Vec::new());
    }

    let mut in_degree: HashMap<NodeIndex, usize> = HashMap::with_capacity(node_count);
    let mut display_keys: HashMap<NodeIndex, String> = HashMap::with_capacity(node_count);
    let mut ready: BTreeMap<(String, usize), NodeIndex> = BTreeMap::new();

    for idx in dag.inner().node_indices() {
        let deg = dag.parent_indices(idx).len();
        in_degree.insert(idx, deg);
        if let Some(payload) = dag.node_by_index(idx) {
            let disp = payload.key().to_string();
            if deg == 0 {
                ready.insert((disp.clone(), idx.index()), idx);
            }
            display_keys.insert(idx, disp);
        }
    }

    let mut order = Vec::with_capacity(node_count);
    while let Some((_sort_key, idx)) = ready.pop_first() {
        if let Some(payload) = dag.node_by_index(idx) {
            order.push(payload.key().clone());
        }
        for child_idx in dag.child_indices(idx) {
            if let Some(deg) = in_degree.get_mut(&child_idx) {
                *deg -= 1;
                if *deg == 0
                    && let Some(disp) = display_keys.get(&child_idx)
                {
                    ready.insert((disp.clone(), child_idx.index()), child_idx);
                }
            }
        }
    }

    if order.len() != node_count {
        dag.validate_acyclic()?;
        let mut cycle_nodes: Vec<usize> = in_degree
            .into_iter()
            .filter_map(|(idx, deg)| if deg > 0 { Some(idx.index()) } else { None })
            .collect();
        cycle_nodes.sort_unstable();
        return Err(DagError::CycleDetected {
            node_indices: cycle_nodes,
        });
    }

    Ok(order)
}

/// Groups nodes in `dag` into deterministic parallel execution levels.
///
/// Level `0` contains all root nodes (nodes with no parents). Each subsequent level contains
/// nodes whose parents all appear in earlier levels. Keys within each level are sorted by
/// their [`Display`](std::fmt::Display) string representation.
///
/// # Errors
/// Returns [`DagError::CycleDetected`] if the graph contains a cycle.
pub fn levels<T: NodePayload>(dag: &MemoryDag<T>) -> Result<Vec<Vec<T::Key>>, DagError> {
    let node_count = dag.node_count();
    if node_count == 0 {
        return Ok(Vec::new());
    }

    let mut in_degree: HashMap<NodeIndex, usize> = HashMap::with_capacity(node_count);
    let mut display_keys: HashMap<NodeIndex, String> = HashMap::with_capacity(node_count);
    let mut current_ready: BTreeMap<(String, usize), NodeIndex> = BTreeMap::new();

    for idx in dag.inner().node_indices() {
        let deg = dag.parent_indices(idx).len();
        in_degree.insert(idx, deg);
        if let Some(payload) = dag.node_by_index(idx) {
            let disp = payload.key().to_string();
            if deg == 0 {
                current_ready.insert((disp.clone(), idx.index()), idx);
            }
            display_keys.insert(idx, disp);
        }
    }

    let mut result = Vec::new();
    let mut visited = 0usize;

    while !current_ready.is_empty() {
        let mut next_ready: BTreeMap<(String, usize), NodeIndex> = BTreeMap::new();
        let mut level_keys = Vec::with_capacity(current_ready.len());

        for (_sort_key, idx) in current_ready {
            visited += 1;
            if let Some(payload) = dag.node_by_index(idx) {
                level_keys.push(payload.key().clone());
            }
            for child_idx in dag.child_indices(idx) {
                if let Some(deg) = in_degree.get_mut(&child_idx) {
                    *deg -= 1;
                    if *deg == 0
                        && let Some(disp) = display_keys.get(&child_idx)
                    {
                        next_ready.insert((disp.clone(), child_idx.index()), child_idx);
                    }
                }
            }
        }

        result.push(level_keys);
        current_ready = next_ready;
    }

    if visited != node_count {
        dag.validate_acyclic()?;
        let mut cycle_nodes: Vec<usize> = in_degree
            .into_iter()
            .filter_map(|(idx, deg)| if deg > 0 { Some(idx.index()) } else { None })
            .collect();
        cycle_nodes.sort_unstable();
        return Err(DagError::CycleDetected {
            node_indices: cycle_nodes,
        });
    }

    Ok(result)
}

/// Returns every node that transitively depends on `key` (its transitive descendants),
/// ordered deterministically in topological order.
///
/// This represents the ordered rebuild set when `key` changes (excluding `key` itself).
///
/// # Errors
/// - Returns [`DagError::NodeNotFound`] if `key` does not exist in `dag`.
/// - Returns [`DagError::CycleDetected`] if the graph contains a cycle.
pub fn reverse_deps<T: NodePayload>(
    dag: &MemoryDag<T>,
    key: &T::Key,
) -> Result<Vec<T::Key>, DagError> {
    if !dag.contains_key(key) {
        return Err(DagError::NodeNotFound {
            key: key.to_string(),
        });
    }

    let topo = toposort(dag)?;
    let descendants: HashSet<T::Key> = dag
        .transitive_descendants(key)
        .into_iter()
        .map(|node| node.key().clone())
        .collect();

    Ok(topo
        .into_iter()
        .filter(|k| descendants.contains(k))
        .collect())
}

/// Computes the longest weighted path (critical path) through `dag`.
///
/// Returns `(total_weight, path_keys)` in topological order from start root to end node.
/// Ties are broken deterministically by [`Display`](std::fmt::Display) string order.
/// For an empty DAG, returns `Ok((0, Vec::new()))`.
///
/// # Errors
/// Returns [`DagError::CycleDetected`] if the graph contains a cycle.
pub fn critical_path<T: NodePayload, F: Fn(&T) -> u64>(
    dag: &MemoryDag<T>,
    weight: F,
) -> Result<(u64, Vec<T::Key>), DagError> {
    let topo_keys = toposort(dag)?;
    if topo_keys.is_empty() {
        return Ok((0, Vec::new()));
    }

    let mut dist: HashMap<NodeIndex, u64> = HashMap::with_capacity(topo_keys.len());
    let mut prev: HashMap<NodeIndex, NodeIndex> = HashMap::with_capacity(topo_keys.len());
    let mut display_keys: HashMap<NodeIndex, String> = HashMap::with_capacity(topo_keys.len());

    for key in &topo_keys {
        if let Some(idx) = dag.node_index(key) {
            display_keys.insert(idx, key.to_string());
        }
    }

    for key in &topo_keys {
        let Some(idx) = dag.node_index(key) else {
            continue;
        };
        let Some(payload) = dag.node_by_index(idx) else {
            continue;
        };
        let node_w = weight(payload);

        let mut best_parent: Option<(u64, &str, NodeIndex)> = None;
        for p_idx in dag.parent_indices(idx) {
            let Some(&p_dist) = dist.get(&p_idx) else {
                continue;
            };
            let p_disp = display_keys.get(&p_idx).map_or("", String::as_str);
            match best_parent {
                None => best_parent = Some((p_dist, p_disp, p_idx)),
                Some((cur_dist, cur_disp, _)) => {
                    if p_dist > cur_dist || (p_dist == cur_dist && p_disp < cur_disp) {
                        best_parent = Some((p_dist, p_disp, p_idx));
                    }
                }
            }
        }

        if let Some((p_dist, _, p_idx)) = best_parent {
            dist.insert(idx, p_dist.saturating_add(node_w));
            prev.insert(idx, p_idx);
        } else {
            dist.insert(idx, node_w);
        }
    }

    let mut best_end: Option<(u64, Vec<String>, NodeIndex)> = None;
    for key in &topo_keys {
        let Some(idx) = dag.node_index(key) else {
            continue;
        };
        let Some(&d) = dist.get(&idx) else {
            continue;
        };
        let path_disp = reconstruct_display_path(idx, &prev, &display_keys);
        match &best_end {
            None => best_end = Some((d, path_disp, idx)),
            Some((cur_d, cur_path, _)) => {
                if d > *cur_d || (d == *cur_d && path_disp < *cur_path) {
                    best_end = Some((d, path_disp, idx));
                }
            }
        }
    }

    let Some((max_weight, _, end_idx)) = best_end else {
        return Ok((0, Vec::new()));
    };

    let mut path_indices = Vec::new();
    let mut curr = Some(end_idx);
    while let Some(idx) = curr {
        path_indices.push(idx);
        curr = prev.get(&idx).copied();
    }
    path_indices.reverse();

    let path_keys = path_indices
        .into_iter()
        .filter_map(|idx| dag.node_by_index(idx).map(|n| n.key().clone()))
        .collect();

    Ok((max_weight, path_keys))
}

fn reconstruct_display_path(
    end_idx: NodeIndex,
    prev: &HashMap<NodeIndex, NodeIndex>,
    display_keys: &HashMap<NodeIndex, String>,
) -> Vec<String> {
    let mut path = Vec::new();
    let mut curr = Some(end_idx);
    while let Some(idx) = curr {
        if let Some(disp) = display_keys.get(&idx) {
            path.push(disp.clone());
        }
        curr = prev.get(&idx).copied();
    }
    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EdgePayload, ProvenanceKind};

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct TaskUnit {
        id: String,
        cost: u64,
    }

    impl TaskUnit {
        fn new(id: &str, cost: u64) -> Self {
            Self {
                id: id.to_string(),
                cost,
            }
        }
    }

    impl NodePayload for TaskUnit {
        type Key = String;

        fn key(&self) -> &Self::Key {
            &self.id
        }

        fn name(&self) -> &str {
            &self.id
        }

        fn entity_level(&self) -> &str {
            "Task"
        }
    }

    fn dep_edge() -> EdgePayload {
        EdgePayload::new("depends_on", 1.0, ProvenanceKind::DeterministicRule).unwrap()
    }

    #[test]
    fn test_empty_dag() {
        let dag: MemoryDag<TaskUnit> = MemoryDag::new();
        assert_eq!(toposort(&dag).unwrap(), Vec::<String>::new());
        assert_eq!(levels(&dag).unwrap(), Vec::<Vec<String>>::new());
        assert_eq!(critical_path(&dag, |t| t.cost).unwrap(), (0, Vec::new()));
        assert!(matches!(
            reverse_deps(&dag, &"missing".to_string()),
            Err(DagError::NodeNotFound { key }) if key == "missing"
        ));
    }

    #[test]
    fn test_diamond_graph() {
        let mut dag = MemoryDag::new();
        dag.upsert_node(TaskUnit::new("A", 1));
        dag.upsert_node(TaskUnit::new("B", 2));
        dag.upsert_node(TaskUnit::new("C", 3));
        dag.upsert_node(TaskUnit::new("D", 4));

        dag.add_edge_by_keys(&"A".into(), &"B".into(), dep_edge())
            .unwrap();
        dag.add_edge_by_keys(&"A".into(), &"C".into(), dep_edge())
            .unwrap();
        dag.add_edge_by_keys(&"B".into(), &"D".into(), dep_edge())
            .unwrap();
        dag.add_edge_by_keys(&"C".into(), &"D".into(), dep_edge())
            .unwrap();

        let order = toposort(&dag).unwrap();
        assert_eq!(order, vec!["A", "B", "C", "D"]);

        let lvls = levels(&dag).unwrap();
        assert_eq!(
            lvls,
            vec![
                vec!["A".to_string()],
                vec!["B".to_string(), "C".to_string()],
                vec!["D".to_string()],
            ]
        );
    }

    #[test]
    fn test_deterministic_tie_order() {
        let mut dag1 = MemoryDag::new();
        for k in ["delta", "alpha", "charlie", "bravo"] {
            dag1.upsert_node(TaskUnit::new(k, 1));
        }

        let mut dag2 = MemoryDag::new();
        for k in ["bravo", "charlie", "alpha", "delta"] {
            dag2.upsert_node(TaskUnit::new(k, 1));
        }

        let expected = vec![
            "alpha".to_string(),
            "bravo".to_string(),
            "charlie".to_string(),
            "delta".to_string(),
        ];
        assert_eq!(toposort(&dag1).unwrap(), expected);
        assert_eq!(toposort(&dag2).unwrap(), expected);
        assert_eq!(levels(&dag1).unwrap(), vec![expected.clone()]);
        assert_eq!(levels(&dag2).unwrap(), vec![expected]);
    }

    #[test]
    fn test_cycle_error_via_unguarded_add_edge() {
        let mut dag = MemoryDag::new();
        let idx_a = dag.upsert_node(TaskUnit::new("A", 1));
        let idx_b = dag.upsert_node(TaskUnit::new("B", 1));
        let idx_c = dag.upsert_node(TaskUnit::new("C", 1));

        dag.add_edge(idx_a, idx_b, dep_edge());
        dag.add_edge(idx_b, idx_c, dep_edge());
        dag.add_edge(idx_c, idx_a, dep_edge());

        let err = toposort(&dag).unwrap_err();
        match err {
            DagError::CycleDetected { node_indices } => {
                assert!(node_indices.contains(&idx_a.index()));
                assert!(node_indices.contains(&idx_b.index()));
                assert!(node_indices.contains(&idx_c.index()));
            }
            other => panic!("expected CycleDetected, got {other:?}"),
        }

        assert!(matches!(levels(&dag), Err(DagError::CycleDetected { .. })));
        assert!(matches!(
            critical_path(&dag, |n| n.cost),
            Err(DagError::CycleDetected { .. })
        ));
    }

    #[test]
    fn test_reverse_deps_chain_and_diamond() {
        // Chain: A -> B -> C -> D
        let mut chain = MemoryDag::new();
        for k in ["A", "B", "C", "D"] {
            chain.upsert_node(TaskUnit::new(k, 1));
        }
        chain
            .add_edge_by_keys(&"A".into(), &"B".into(), dep_edge())
            .unwrap();
        chain
            .add_edge_by_keys(&"B".into(), &"C".into(), dep_edge())
            .unwrap();
        chain
            .add_edge_by_keys(&"C".into(), &"D".into(), dep_edge())
            .unwrap();

        assert_eq!(
            reverse_deps(&chain, &"A".to_string()).unwrap(),
            vec!["B", "C", "D"]
        );
        assert_eq!(
            reverse_deps(&chain, &"B".to_string()).unwrap(),
            vec!["C", "D"]
        );
        assert_eq!(
            reverse_deps(&chain, &"D".to_string()).unwrap(),
            Vec::<String>::new()
        );

        // Diamond: A -> B, A -> C, B -> D, C -> D, plus unrelated E
        let mut diamond = MemoryDag::new();
        for k in ["A", "B", "C", "D", "E"] {
            diamond.upsert_node(TaskUnit::new(k, 1));
        }
        diamond
            .add_edge_by_keys(&"A".into(), &"B".into(), dep_edge())
            .unwrap();
        diamond
            .add_edge_by_keys(&"A".into(), &"C".into(), dep_edge())
            .unwrap();
        diamond
            .add_edge_by_keys(&"B".into(), &"D".into(), dep_edge())
            .unwrap();
        diamond
            .add_edge_by_keys(&"C".into(), &"D".into(), dep_edge())
            .unwrap();

        assert_eq!(
            reverse_deps(&diamond, &"A".to_string()).unwrap(),
            vec!["B", "C", "D"]
        );
        assert_eq!(reverse_deps(&diamond, &"B".to_string()).unwrap(), vec!["D"]);
        assert!(matches!(
            reverse_deps(&diamond, &"UNKNOWN".to_string()),
            Err(DagError::NodeNotFound { .. })
        ));
    }

    #[test]
    fn test_critical_path() {
        let mut dag = MemoryDag::new();
        dag.upsert_node(TaskUnit::new("A", 10));
        dag.upsert_node(TaskUnit::new("B", 25));
        dag.upsert_node(TaskUnit::new("C", 5));
        dag.upsert_node(TaskUnit::new("D", 15));

        dag.add_edge_by_keys(&"A".into(), &"B".into(), dep_edge())
            .unwrap();
        dag.add_edge_by_keys(&"A".into(), &"C".into(), dep_edge())
            .unwrap();
        dag.add_edge_by_keys(&"B".into(), &"D".into(), dep_edge())
            .unwrap();
        dag.add_edge_by_keys(&"C".into(), &"D".into(), dep_edge())
            .unwrap();

        let (total, path) = critical_path(&dag, |t| t.cost).unwrap();
        assert_eq!(total, 50);
        assert_eq!(path, vec!["A", "B", "D"]);
    }
}
