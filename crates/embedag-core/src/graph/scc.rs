//! Strongly connected components (SCCs) and acyclic condensation for directed graphs.
//!
//! Traceability: `doc/22_Dependency_Resolution_and_Merkle_Delta_API.md` (Ticket 9.5.1).

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::error::DagError;

/// The condensation of a directed graph into an acyclic graph of its strongly connected components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Condensation {
    /// Strongly connected components, each containing node indices sorted ascending.
    ///
    /// Components are returned in reverse topological order of the condensation graph:
    /// a component comes after everything it points to.
    pub components: Vec<Vec<usize>>,
    /// Component index for each node: `membership[node]` is the index into `components`.
    pub membership: Vec<usize>,
    /// Directed edges between components `(source_comp, target_comp)`, deduplicated,
    /// sorted in lexicographical order, with self-loops omitted.
    pub edges: Vec<(usize, usize)>,
    /// Topological ordering of component indices (sources first).
    pub order: Vec<usize>,
}

fn validate_node_index(index: usize, node_count: usize) -> Result<(), DagError> {
    if index >= node_count {
        Err(DagError::NodeIndexOutOfBounds { index, node_count })
    } else {
        Ok(())
    }
}

fn validate_edges(node_count: usize, edges: &[(usize, usize)]) -> Result<(), DagError> {
    for &(u, v) in edges {
        validate_node_index(u, node_count)?;
        validate_node_index(v, node_count)?;
    }
    Ok(())
}

fn build_adjacency(node_count: usize, edges: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut adj = vec![Vec::new(); node_count];
    for &(u, v) in edges {
        adj[u].push(v);
    }
    for neighbors in &mut adj {
        neighbors.sort_unstable();
        neighbors.dedup();
    }
    adj
}

struct TarjanState {
    index: Vec<usize>,
    lowlink: Vec<usize>,
    on_stack: Vec<bool>,
    scc_stack: Vec<usize>,
    timer: usize,
    components: Vec<Vec<usize>>,
    membership: Vec<usize>,
}

impl TarjanState {
    fn new(node_count: usize) -> Self {
        Self {
            index: vec![usize::MAX; node_count],
            lowlink: vec![0; node_count],
            on_stack: vec![false; node_count],
            scc_stack: Vec::new(),
            timer: 0,
            components: Vec::new(),
            membership: vec![0; node_count],
        }
    }

    fn start_node(&mut self, u: usize) {
        self.index[u] = self.timer;
        self.lowlink[u] = self.timer;
        self.timer += 1;
        self.scc_stack.push(u);
        self.on_stack[u] = true;
    }

    fn pop_scc(&mut self, root: usize) {
        let mut component = Vec::new();
        while let Some(node) = self.scc_stack.pop() {
            self.on_stack[node] = false;
            component.push(node);
            if node == root {
                break;
            }
        }
        component.sort_unstable();
        let comp_idx = self.components.len();
        for &node in &component {
            self.membership[node] = comp_idx;
        }
        self.components.push(component);
    }
}

struct DfsFrame {
    u: usize,
    edge_idx: usize,
}

fn advance_edge(u: usize, v: usize, state: &mut TarjanState, call_stack: &mut Vec<DfsFrame>) {
    if state.index[v] == usize::MAX {
        state.start_node(v);
        call_stack.push(DfsFrame { u: v, edge_idx: 0 });
    } else if state.on_stack[v] {
        state.lowlink[u] = state.lowlink[u].min(state.index[v]);
    }
}

fn finish_node(u: usize, state: &mut TarjanState, call_stack: &mut Vec<DfsFrame>) {
    call_stack.pop();
    if state.lowlink[u] == state.index[u] {
        state.pop_scc(u);
    }
    if let Some(parent_frame) = call_stack.last() {
        let parent = parent_frame.u;
        state.lowlink[parent] = state.lowlink[parent].min(state.lowlink[u]);
    }
}

fn explore_component(start: usize, adj: &[Vec<usize>], state: &mut TarjanState) {
    state.start_node(start);
    let mut call_stack = vec![DfsFrame {
        u: start,
        edge_idx: 0,
    }];
    while let Some(frame) = call_stack.last_mut() {
        let u = frame.u;
        if frame.edge_idx < adj[u].len() {
            let v = adj[u][frame.edge_idx];
            frame.edge_idx += 1;
            advance_edge(u, v, state, &mut call_stack);
        } else {
            finish_node(u, state, &mut call_stack);
        }
    }
}

fn tarjan_scc(node_count: usize, edges: &[(usize, usize)]) -> Result<TarjanState, DagError> {
    validate_edges(node_count, edges)?;
    let adj = build_adjacency(node_count, edges);
    let mut state = TarjanState::new(node_count);
    for node in 0..node_count {
        if state.index[node] == usize::MAX {
            explore_component(node, &adj, &mut state);
        }
    }
    Ok(state)
}

fn build_condensation_edges(membership: &[usize], edges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut cond_edges = Vec::new();
    for &(u, v) in edges {
        let cu = membership[u];
        let cv = membership[v];
        if cu != cv {
            cond_edges.push((cu, cv));
        }
    }
    cond_edges.sort_unstable();
    cond_edges.dedup();
    cond_edges
}

fn topological_order(
    comp_count: usize,
    components: &[Vec<usize>],
    cond_edges: &[(usize, usize)],
) -> Vec<usize> {
    let mut in_degree = vec![0usize; comp_count];
    let mut comp_adj = vec![Vec::new(); comp_count];
    for &(src, dst) in cond_edges {
        in_degree[dst] += 1;
        comp_adj[src].push(dst);
    }

    let mut ready = BinaryHeap::new();
    for c in 0..comp_count {
        if in_degree[c] == 0 {
            ready.push(Reverse((components[c][0], c)));
        }
    }

    let mut order = Vec::with_capacity(comp_count);
    while let Some(Reverse((_, c))) = ready.pop() {
        order.push(c);
        for &dst in &comp_adj[c] {
            in_degree[dst] -= 1;
            if in_degree[dst] == 0 {
                ready.push(Reverse((components[dst][0], dst)));
            }
        }
    }
    order
}

/// Computes the strongly connected components of a directed graph.
///
/// Returns components in reverse topological order of the condensation graph:
/// a component comes after everything it points to. Nodes inside each component
/// are sorted in ascending order. Ties are broken deterministically by node index.
///
/// Iterative Tarjan's algorithm is used with no recursion, preventing call-stack
/// overflow even on chains of 100,000+ nodes.
///
/// # Errors
/// Returns [`DagError::NodeIndexOutOfBounds`] if any edge references a node
/// index greater than or equal to `node_count`.
pub fn strongly_connected_components(
    node_count: usize,
    edges: &[(usize, usize)],
) -> Result<Vec<Vec<usize>>, DagError> {
    let state = tarjan_scc(node_count, edges)?;
    Ok(state.components)
}

/// Computes the condensation of a directed graph into an acyclic graph of its
/// strongly connected components.
///
/// In the condensation DAG, each strongly connected component becomes a single
/// super-node. Components are returned in reverse topological order of the
/// condensation graph (a component comes after everything it points to). Nodes
/// inside each component are sorted in ascending order.
///
/// The resulting condensation contains:
/// - `components`: strongly connected components in reverse topological order.
/// - `membership`: mapping from original node index to component index.
/// - `edges`: deduplicated, sorted directed edges between component indices without self-loops.
/// - `order`: topological ordering of component indices (sources first).
///
/// The condensation is strictly acyclic.
///
/// # Errors
/// Returns [`DagError::NodeIndexOutOfBounds`] if any edge references a node
/// index greater than or equal to `node_count`.
pub fn condense(node_count: usize, edges: &[(usize, usize)]) -> Result<Condensation, DagError> {
    let state = tarjan_scc(node_count, edges)?;
    let cond_edges = build_condensation_edges(&state.membership, edges);
    let comp_count = state.components.len();
    let order = topological_order(comp_count, &state.components, &cond_edges);

    Ok(Condensation {
        components: state.components,
        membership: state.membership,
        edges: cond_edges,
        order,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::{fixture, rstest};

    #[fixture]
    fn two_cycles_with_bridge() -> (usize, Vec<(usize, usize)>) {
        // Cycle 1: 0 -> 1 -> 0
        // Bridge:  1 -> 2
        // Cycle 2: 2 -> 3 -> 2
        (4, vec![(0, 1), (1, 0), (1, 2), (2, 3), (3, 2)])
    }

    #[fixture]
    fn chain_100k() -> (usize, Vec<(usize, usize)>) {
        let n = 100_000;
        let edges = (0..n - 1).map(|i| (i, i + 1)).collect();
        (n, edges)
    }

    #[rstest]
    fn test_empty_graph() {
        let sccs = strongly_connected_components(0, &[]).unwrap();
        assert_eq!(sccs, Vec::<Vec<usize>>::new());

        let cond = condense(0, &[]).unwrap();
        assert_eq!(cond.components, Vec::<Vec<usize>>::new());
        assert_eq!(cond.membership, Vec::<usize>::new());
        assert_eq!(cond.edges, Vec::<(usize, usize)>::new());
        assert_eq!(cond.order, Vec::<usize>::new());
    }

    #[rstest]
    #[case::single_node(1, vec![], vec![vec![0]], vec![0], vec![], vec![0])]
    #[case::self_loop(1, vec![(0, 0)], vec![vec![0]], vec![0], vec![], vec![0])]
    #[case::duplicate_edges(
        2,
        vec![(0, 1), (0, 1), (0, 1)],
        vec![vec![1], vec![0]],
        vec![1, 0],
        vec![(1, 0)],
        vec![1, 0]
    )]
    #[case::dag_input(
        3,
        vec![(0, 1), (1, 2), (0, 2)],
        vec![vec![2], vec![1], vec![0]],
        vec![2, 1, 0],
        vec![(1, 0), (2, 0), (2, 1)],
        vec![2, 1, 0]
    )]
    fn test_small_graphs(
        #[case] node_count: usize,
        #[case] edges: Vec<(usize, usize)>,
        #[case] expected_components: Vec<Vec<usize>>,
        #[case] expected_membership: Vec<usize>,
        #[case] expected_edges: Vec<(usize, usize)>,
        #[case] expected_order: Vec<usize>,
    ) {
        let sccs = strongly_connected_components(node_count, &edges).unwrap();
        assert_eq!(sccs, expected_components);

        let cond = condense(node_count, &edges).unwrap();
        assert_eq!(cond.components, expected_components);
        assert_eq!(cond.membership, expected_membership);
        assert_eq!(cond.edges, expected_edges);
        assert_eq!(cond.order, expected_order);
    }

    #[rstest]
    fn test_two_cycles_joined_by_a_bridge(two_cycles_with_bridge: (usize, Vec<(usize, usize)>)) {
        let (node_count, edges) = two_cycles_with_bridge;
        let cond = condense(node_count, &edges).unwrap();

        // Components in reverse topological order:
        // Component 0 is {2, 3} (sink), Component 1 is {0, 1} (source)
        assert_eq!(cond.components, vec![vec![2, 3], vec![0, 1]]);
        assert_eq!(cond.membership, vec![1, 1, 0, 0]);
        // Bridge connects component 1 to component 0:
        assert_eq!(cond.edges, vec![(1, 0)]);
        // Topological order (sources first):
        assert_eq!(cond.order, vec![1, 0]);

        let sccs = strongly_connected_components(node_count, &edges).unwrap();
        assert_eq!(sccs, cond.components);
    }

    #[rstest]
    fn test_chain_100k_nodes_no_stack_overflow(chain_100k: (usize, Vec<(usize, usize)>)) {
        let (node_count, edges) = chain_100k;
        let cond = condense(node_count, &edges).unwrap();

        assert_eq!(cond.components.len(), node_count);
        assert_eq!(cond.membership.len(), node_count);
        assert_eq!(cond.edges.len(), node_count - 1);
        assert_eq!(cond.order.len(), node_count);

        // First component is the sink (node 99_999)
        assert_eq!(cond.components[0], vec![99_999]);
        // Last component is the root source (node 0)
        assert_eq!(cond.components[node_count - 1], vec![0]);

        // Topological order begins with the source component (node 0)
        assert_eq!(cond.order[0], node_count - 1);
        assert_eq!(cond.order[node_count - 1], 0);

        // Membership verification
        assert_eq!(cond.membership[0], node_count - 1);
        assert_eq!(cond.membership[node_count - 1], 0);

        // Condensation edges are strictly between adjacent component indices
        assert_eq!(cond.edges[0], (1, 0));
        assert_eq!(cond.edges[node_count - 2], (node_count - 1, node_count - 2));
    }

    #[rstest]
    #[case::source_out_of_range(3, vec![(3, 0)], 3)]
    #[case::target_out_of_range(3, vec![(0, 3)], 3)]
    #[case::large_out_of_range(3, vec![(0, 1), (1, 100)], 100)]
    #[case::empty_graph_out_of_range(0, vec![(0, 0)], 0)]
    fn test_out_of_range_edge(
        #[case] node_count: usize,
        #[case] edges: Vec<(usize, usize)>,
        #[case] expected_invalid_index: usize,
    ) {
        let scc_err = strongly_connected_components(node_count, &edges).unwrap_err();
        assert_eq!(
            scc_err,
            DagError::NodeIndexOutOfBounds {
                index: expected_invalid_index,
                node_count,
            }
        );

        let cond_err = condense(node_count, &edges).unwrap_err();
        assert_eq!(
            cond_err,
            DagError::NodeIndexOutOfBounds {
                index: expected_invalid_index,
                node_count,
            }
        );
    }
}
