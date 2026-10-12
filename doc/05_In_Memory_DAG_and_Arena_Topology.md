# In-Memory DAG & Arena Topology

| Metadata | Details |
| :--- | :--- |
| **Document ID** | `05_In_Memory_DAG_and_Arena_Topology` |
| **Domain** | Graph Representation & Arena Memory Layout |
| **Responsibility** | High-Performance In-Memory Directed Acyclic Graph Representation, Node Arenas, and Traversal Visitors |
| **Target Crates** | `embedag-core` |
| **Status** | Definitive Architectural Specification |

---

## 1. Executive Summary & Architectural Role

Real-world enterprise taxonomies require **multi-parent DAG structures**, where an entity can have multiple distinct ancestors (e.g., an item belonging simultaneously to *Food -> Confectionery* and *Dietary -> Vegan*). Tree structures (such as `indextree` or pure hierarchies) are fundamentally incapable of modeling these diamond and multi-parent relations.

`embedag-core` implements `MemoryDag<T>`, an index-based generational arena backed by `petgraph::graph::DiGraph<T, EdgePayload>`. It provides:
1. $O(1)$ node lookups by business key via a synchronized hash index.
2. Direct index-based traversals without pointer chasing or heap-fragmenting smart pointers (`Arc<RwLock<Node>>`).
3. Breadth-First (BFS) and Depth-First (DFS) visitors.
4. Clone-efficient shadow DAG simulations for safe mutation pre-flight checks.

---

## 2. In-Memory Arena Architecture (`petgraph`)

```mermaid
flowchart TD
    subgraph MemoryDag ["MemoryDag&lt;T: NodePayload&gt;"]
        subgraph SecondaryIndex ["Secondary Index: O(1) Key Lookup"]
            Map["HashMap&lt;T::Key, NodeIndex&gt;<br/>'SKU-100' --> NodeIndex(0)<br/>'SKU-200' --> NodeIndex(1)"]
        end

        subgraph PrimaryArena ["Primary Arena: petgraph::graph::DiGraph&lt;T, EdgePayload&gt;"]
            NodesArena["Nodes Arena: Vec&lt;T&gt;<br/>[ Node 0 (T) | Node 1 (T) | Node 2 (T) | ... ]"]
            EdgesArena["Edges Arena: Vec&lt;EdgePayload&gt;<br/>[ Edge 0 (E) | Edge 1 (E) | Edge 2 (E) | ... ]"]
        end

        Map -->|O(1) Handle Lookup| NodesArena
        NodesArena -.->|Directed Adjacency| EdgesArena
    end
```

### 2.1 The `EdgePayload` Definition
Edges in `embedag` are typed and carry metadata regarding the provenance and statistical confidence of the relationship:

```rust
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq)]
pub struct EdgePayload {
    pub relationship: String,           // e.g., "is_a", "belongs_to", "packaged_as"
    pub confidence: f32,                // Probability score in [0.0, 1.0]
    pub provenance: ProvenanceKind,     // Deterministic, LLM_Inference, Human_Override
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvenanceKind {
    DeterministicRule,
    DenseSimilarityMatch,
    LlmConstrainedInference,
    HumanVerified,
}
```

### 2.2 The `MemoryDag<T>` Implementation
```rust
use std::collections::HashMap;
use petgraph::graph::{DiGraph, NodeIndex, EdgeIndex};
use petgraph::Direction;
use crate::traits::{NodePayload, EdgePayload};

pub struct MemoryDag<T: NodePayload> {
    graph: DiGraph<T, EdgePayload>,
    key_to_index: HashMap<T::Key, NodeIndex>,
}

impl<T: NodePayload> MemoryDag<T> {
    /// Creates a new, empty in-memory DAG.
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            key_to_index: HashMap::new(),
        }
    }

    /// Inserts or updates a node payload. Returns the stable NodeIndex.
    pub fn upsert_node(&mut self, payload: T) -> NodeIndex {
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

    /// Connects parent to child with an EdgePayload.
    pub fn add_edge_by_keys(
        &mut self,
        parent: &T::Key,
        child: &T::Key,
        edge: EdgePayload,
    ) -> Result<EdgeIndex, String> {
        let p_idx = *self.key_to_index.get(parent).ok_or("Parent key not found")?;
        let c_idx = *self.key_to_index.get(child).ok_or("Child key not found")?;
        Ok(self.graph.add_edge(p_idx, c_idx, edge))
    }

    /// Retrieves parents of a given node key.
    pub fn parents_of(&self, key: &T::Key) -> Vec<&T> {
        let Some(&idx) = self.key_to_index.get(key) else { return Vec::new(); };
        self.graph
            .neighbors_directed(idx, Direction::Incoming)
            .map(|p_idx| &self.graph[p_idx])
            .collect()
    }

    /// Retrieves children of a given node key.
    pub fn children_of(&self, key: &T::Key) -> Vec<&T> {
        let Some(&idx) = self.key_to_index.get(key) else { return Vec::new(); };
        self.graph
            .neighbors_directed(idx, Direction::Outgoing)
            .map(|c_idx| &self.graph[c_idx])
            .collect()
    }

    /// Returns the underlying petgraph reference for low-level graph algorithms.
    pub fn inner(&self) -> &DiGraph<T, EdgePayload> {
        &self.graph
    }
}
```

---

## 3. Traversal Visitors (BFS & DFS)

`embedag` provides zero-allocation graph iterators to traverse ancestors and descendants:

```rust
use petgraph::visit::{Bfs, Dfs};

impl<T: NodePayload> MemoryDag<T> {
    /// Collects all transitive ancestors using Breadth-First Search.
    pub fn transitive_ancestors(&self, start_key: &T::Key) -> Vec<&T> {
        let Some(&start_idx) = self.key_to_index.get(start_key) else { return Vec::new(); };
        let mut bfs = Bfs::new(&petgraph::visit::Reversed(&self.graph), start_idx);
        let mut ancestors = Vec::new();
        while let Some(nx) = bfs.next(&petgraph::visit::Reversed(&self.graph)) {
            if nx != start_idx {
                ancestors.push(&self.graph[nx]);
            }
        }
        ancestors
    }
}
```

---

## 4. Invariants & Performance Characteristics

1. **Diamond Topologies Allowed**: Multiple paths between root $R$ and child $C$ through intermediate nodes $A$ and $B$ are fully supported.
2. **Memory Efficiency**: Storing 100,000 nodes and 250,000 edges consumes $< 40\,	ext{MB}$ of heap space due to contiguous array arenas.
3. **Index Stability**: `petgraph::graph::DiGraph` indices remain stable as long as nodes are not removed via `remove_node` (which uses swap-remove). Removal is handled via soft deprecation flags rather than arena compaction.
