# embedag-core

`embedag-core` provides the foundational data structures, traits, deterministic feature encoders, and graph algorithms for the `embedag` workspace. It solves the problem of maintaining an in-memory directed acyclic graph (`MemoryDag<T>`) with compile-time and runtime acyclicity guarantees, deterministic dependency ordering, content-addressed BLAKE3 Merkle diffing, and zero-LLM numeric/temporal/geospatial feature normalization for downstream graph persistence and retrieval crates.

## When to use it

Use `embedag-core` when you need:

- An arena-backed directed acyclic graph (`MemoryDag<T>`) keyed by business identifiers (`NodePayload::Key`) with O(1) index lookups and O(V + E) cycle detection.
- Compile-time enforced mutation lifecycles (`MutationPlan<T, Unverified>` -> `MutationPlan<T, CycleValidated>` -> `MutationPlan<T, Committed>`) using either full Tarjan SCC validation or differential reverse-BFS validation.
- Deterministic topological sorting, parallel wave scheduling, transitive reverse-dependency impact sets, or weighted critical-path analysis (`embedag_core::resolve`).
- Content-addressed BLAKE3 Merkle trees and structural tree diffs (`embedag_core::merkle`).
- Deterministic scalar normalization (min-max linear, logarithmic, Z-score), cyclical time encoding (sin/cos hour-of-day and day-of-week), H3 geospatial cell indexing and distance decay, or weighted multi-modal distance computation (`embedag_core::encoder`).
- Storage-agnostic repository traits (`ReadOnlyGraphRepository`, `VectorSearchRepository`, `TransactionalGraphRepository`, `HierarchyTraversalRepository`, `GraphRepository`) or graph export to Mermaid, Graphviz DOT, GraphML, JSON-LD, and RDF-Star Turtle (`embedag_core::export`).

Do not use `embedag-core` alone when you need database persistence (use [`embedag-diesel`](https://docs.rs/embedag-diesel), [`embedag-oxirs`](https://docs.rs/embedag-oxirs), [`embedag-surrealdb`](https://docs.rs/embedag-surrealdb), or [`embedag-neo4j`](https://docs.rs/embedag-neo4j)), LLM extraction (use [`embedag-llm`](https://docs.rs/embedag-llm)), or hybrid vector/lexical retrieval (use [`embedag-rag`](https://docs.rs/embedag-rag)).

### Implemented vs. planned capabilities

- **Neural text embeddings (`Embedder` trait)**: `GraphContext` carries text, scalar, temporal, geospatial, categorical, sparse-token, and `MultimodalBlob` fields (where `MultimodalBlob` stores a `mime_type`, `storage_uri`, and precomputed `Vec<f32>` vector), and `similarity::rank_by_cosine` ranks dense vectors. A first-class `Embedder` trait with deterministic n-gram hashing and local `candle` BERT/BGE inference is specified in [`doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md`](https://github.com/yasuomaidana/embedag/blob/main/doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md) (Ticket 10.1.1) and is not yet implemented.
- **Milestone 9 & 10 extensions**: Multi-root dependency manifests (`resolve_subgraph`, Ticket 9.2.1), work-item prioritisation (`prioritise_ready`, Ticket 9.2.2), Merkle journal verification (Ticket 9.3.1), recurring time-series cadence detection (Ticket 10.2.3), and incremental DAG-backed Merkle trees (`MerkleDag`, Ticket 10.5.1) are planned in [`doc/22_Dependency_Resolution_and_Merkle_Delta_API.md`](https://github.com/yasuomaidana/embedag/blob/main/doc/22_Dependency_Resolution_and_Merkle_Delta_API.md) and [`doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md`](https://github.com/yasuomaidana/embedag/blob/main/doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md).

## Main types and functions

- **Domain contracts (`model`, `traits`, `error`)**:
  - `NodePayload`: Trait mapping a domain struct to its `Key`, `entity_level()`, `name()`, and JSON `attributes()`. Implementable via `impl_node_payload!`.
  - `AsGraphContext` and `GraphTarget`: Trait for extracting multi-modal record features (`context_key`, `text_repr`, `scalar_features`, `timestamp_utc`, `geographic_coordinates`, `categorical_tags`, `multimodal_blobs`) and placement target enum (`ExactMatch`, `FuzzyCandidate`, `NewChildOf`, `NewMultiParentChild`, `NewRoot`, `Ambiguous`).
  - `EdgePayload` and `ProvenanceKind`: Validated directed edge metadata (`relationship`, `confidence` in `[0.0, 1.0]` with NaN/infinity rejection, `provenance`, `created_at`).
  - `DagError`: Error enum covering `CycleDetected`, `InvalidConfidence`, `NodeNotFound`, `DuplicateNodeKey`, `EncodingError`, and `StorageError`.
- **In-memory DAG and typestate mutations (`dag`, `mutation`, `dag_iter`, `pagination`)**:
  - `MemoryDag<T>`: Arena-backed graph over `petgraph::stable_graph::StableDiGraph<T, EdgePayload>` with `upsert_node`, `add_edge_by_keys`, `remove_edge_by_keys`, `parents_of`, `children_of`, `transitive_ancestors`, `transitive_descendants`, `iter_ancestors`, `iter_descendants`, `parents_paginated`, `children_paginated`, `find_cycles`, and `validate_acyclic`.
  - `MutationPlan<T, State>`: Typestate builder (`Unverified` -> `CycleValidated` -> `Committed`) with `validate_cycles`, `validate_cycles_differential`, and `apply`.
- **Strongly connected components (`scc`)**:
  - `strongly_connected_components(node_count, &[(usize, usize)])`: iterative Tarjan over an index graph that may contain cycles.
  - `condense(node_count, &edges) -> Condensation`: components, `membership` per node, deduplicated component `edges` without self loops, and a topological `order` of the components (Ticket 9.5.1).
- **Dependency resolution (`resolve`)**:
  - `toposort(&MemoryDag<T>) -> Result<Vec<T::Key>, DagError>`: Deterministic Kahn topological sort tie-broken lexicographically by `T::Key::to_string()`.
  - `levels(&MemoryDag<T>) -> Result<Vec<Vec<T::Key>>, DagError>`: Groups nodes into parallel execution waves where wave `0` has in-degree `0`.
  - `reverse_deps(&MemoryDag<T>, &T::Key) -> Result<Vec<T::Key>, DagError>`: Transitive downstream dependents affected by a change to `key`, in topological order.
  - `critical_path(&MemoryDag<T>, Fn(&T) -> u64) -> Result<(u64, Vec<T::Key>), DagError>`: Longest node-weighted path `(total_weight, path_keys)` via dynamic programming over topological order.
- **Merkle trees and structural diffs (`merkle`)**:
  - `MerkleNode::leaf(key, content)`, `MerkleNode::branch(key, children)`, and `MerkleNode::from_paths(root, files)`: Domain-separated BLAKE3 Merkle nodes (`"l"` prefix for leaves, `"b"` prefix for branches with children sorted by key).
  - `merkle::diff(&MerkleNode, &MerkleNode) -> Delta`: Computes `Delta { added, removed, changed }` leaf paths in O(changed subtrees), pruning identical hashes in O(1).
- **Deterministic encoders (`encoder`)**:
  - `encode_linear`, `encode_log`, `encode_standard_score`, `ScalarScaling`.
  - `CyclicalTimeEncoder` (`encode_cyclical_time`) and `temporal_distance`.
  - `GeospatialEncoder::new(resolution)`, `h3_encode`, `h3_distance`, `h3_normalized_distance` (enabled by the `h3o` feature).
  - `composite_distance` and `ModalityWeights`.
- **Export and migration (`export`, `migration`)**:
  - `GraphExporter` supporting Mermaid, Graphviz DOT, GraphML, JSON-LD, and RDF-Star Turtle.
  - `GraphMigration<T>` and `MigrationStep<T>` for versioned node/edge transforms with post-migration cycle checks.

### Example

```rust
use embedag_core::dag::MemoryDag;
use embedag_core::error::DagError;
use embedag_core::merkle::{self, MerkleNode};
use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_core::mutation::MutationPlan;
use embedag_core::resolve;

#[derive(Debug, Clone)]
struct CrateNode { name: String, cost: u64 }

embedag_core::impl_node_payload!(CrateNode, key = |n| &n.name, name = |n| n.name.as_str(), level = |_| "crate");

fn main() -> Result<(), DagError> {
    let mut dag = MemoryDag::new();
    let edge = EdgePayload::new("DEPENDS_ON", 1.0, ProvenanceKind::DeterministicRule)?;

    MutationPlan::new()
        .upsert_node(CrateNode { name: "core".into(), cost: 10 })
        .upsert_node(CrateNode { name: "rag".into(), cost: 25 })
        .upsert_node(CrateNode { name: "cli".into(), cost: 5 })
        .add_edge("core".into(), "rag".into(), edge.clone())
        .add_edge("rag".into(), "cli".into(), edge)
        .validate_cycles_differential(&dag)?
        .apply(&mut dag)?;

    assert_eq!(resolve::levels(&dag)?.len(), 3);

    let (total_cost, path) = resolve::critical_path(&dag, |n| n.cost)?;
    assert_eq!(path, vec!["core", "rag", "cli"]);
    assert_eq!(total_cost, 40);

    let before = MerkleNode::branch("root", vec![MerkleNode::leaf("core", b"v1")]);
    let after = MerkleNode::branch("root", vec![MerkleNode::leaf("core", b"v2")]);
    assert_eq!(merkle::diff(&before, &after).changed, vec!["core"]);
    Ok(())
}
```

## Feature flags

| Feature | Default | Description |
|---|---|---|
| `std` | Yes | Standard library support |
| `h3o` | Yes | Enables H3 hexagonal geospatial indexing (`dep:h3o`) in `embedag_core::encoder::geospatial` |
| `testing` | No | Exposes shared test fixtures (`embedag_core::testing::{TestNode, StoredNode}`) and the `persistence_suite!` macro for adapter test suites |

## Workspace relationship

Every other crate in the `embedag` workspace (except the `embedag-derive` proc-macro crate) depends on `embedag-core` for `MemoryDag`, `NodePayload`, `EdgePayload`, `DagError`, and repository traits.


## Build and test

```bash
cargo build -p embedag-core
cargo test -p embedag-core --all-features
```

## Specifications

- [`doc/04_Core_Domain_and_Entity_Contracts.md`](https://github.com/yasuomaidana/embedag/blob/main/doc/04_Core_Domain_and_Entity_Contracts.md) — `NodePayload`, `EdgePayload`, `GraphContext`, and repository capability traits.
- [`doc/05_In_Memory_DAG_and_Arena_Topology.md`](https://github.com/yasuomaidana/embedag/blob/main/doc/05_In_Memory_DAG_and_Arena_Topology.md) — `MemoryDag<T>` arena architecture and BFS/DFS traversals.
- [`doc/07_Deterministic_Feature_Encoding.md`](https://github.com/yasuomaidana/embedag/blob/main/doc/07_Deterministic_Feature_Encoding.md) — Scalar, cyclical temporal, and H3 geospatial encoders.
- [`doc/08_Neural_and_Multimodal_Embeddings.md`](https://github.com/yasuomaidana/embedag/blob/main/doc/08_Neural_and_Multimodal_Embeddings.md) and [`doc/09_Mixed_Modality_Representation_and_Similarity.md`](https://github.com/yasuomaidana/embedag/blob/main/doc/09_Mixed_Modality_Representation_and_Similarity.md) — Multi-modal representation and composite distance metrics.
- [`doc/13_Topological_Cycle_Detection_and_Guards.md`](https://github.com/yasuomaidana/embedag/blob/main/doc/13_Topological_Cycle_Detection_and_Guards.md) — Typestate mutation plans, Tarjan SCC, and differential reverse-BFS cycle validation.
- [`doc/21_Heavy_Graph_Handling_and_Topological_Pagination.md`](https://github.com/yasuomaidana/embedag/blob/main/doc/21_Heavy_Graph_Handling_and_Topological_Pagination.md) — Lazy iterators, paginated neighbors, and bounded BFS.
- [`doc/22_Dependency_Resolution_and_Merkle_Delta_API.md`](https://github.com/yasuomaidana/embedag/blob/main/doc/22_Dependency_Resolution_and_Merkle_Delta_API.md) — `resolve` and `merkle` modules.
- [`doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md`](https://github.com/yasuomaidana/embedag/blob/main/doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md) — Planned `Embedder` trait and incremental Merkle extensions.
