# Changelog

All notable changes to this crate are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0]

### Added

- Core domain traits (`NodePayload`, `AsGraphContext`, `GraphTarget`), `EdgePayload` with confidence range `[0.0, 1.0]` and NaN/infinity validation, `ProvenanceKind`, `GraphContext`, `MultimodalBlob`, and `DagError` (Ticket 1.2.1, Ticket 1.2.2, Ticket 3.3.1).
- Segregated async persistence capability traits (`ReadOnlyGraphRepository`, `VectorSearchRepository`, `TransactionalGraphRepository`, `HierarchyTraversalRepository`, and blanket `GraphRepository`) (Ticket 1.2.3).
- Arena-backed `MemoryDag<T>` over `petgraph::stable_graph::StableDiGraph` with O(1) business-key lookup, BFS/DFS ancestor and descendant queries, and Tarjan strongly connected components cycle detection (Ticket 1.3.1, Ticket 1.3.2).
- Typestate mutation lifecycle (`MutationPlan<T, Unverified>`, `CycleValidated`, `Committed`) with shadow-clone Tarjan validation (`validate_cycles`) and zero-clone reverse-BFS validation (`validate_cycles_differential`) (Ticket 4.3.1, Ticket 4.3.2, Ticket 7.2.1).
- Deterministic feature encoders in `embedag_core::encoder`: linear, logarithmic, and Z-score scalar scaling (`ScalarScaling`); cyclical sin/cos temporal encoding (`CyclicalTimeEncoder`); H3 hexagonal spatial indexing and grid-disk distance (`GeospatialEncoder`); and weighted `composite_distance` (Ticket 2.1.1, Ticket 2.2.1, Ticket 2.3.1, Ticket 3.2.1).
- Heavy-graph primitives: lazy `AncestorsIter` and `DescendantsIter` iterators (`dag_iter`), keyset-style neighbor pagination (`NeighborPage`, `NeighborPageRequest`), and depth/node-budgeted BFS (`ancestors_bfs_bounded`, `descendants_bfs_bounded`) (Ticket 7.1.1, Ticket 7.1.2, Ticket 7.1.3).
- Multi-format `GraphExporter` supporting Mermaid, Graphviz DOT, GraphML, JSON-LD, and RDF-Star Turtle, plus `GraphMigration` for versioned schema evolution (Ticket 8.4.1, Ticket 8.4.2).
- Deterministic dependency resolution in `embedag_core::resolve`: `toposort`, `levels`, `reverse_deps`, and weighted `critical_path` (EMBEDAG-77 / Ticket 9.1.1).
- Content-addressed BLAKE3 Merkle trees (`MerkleNode::leaf`, `MerkleNode::branch`) and structural tree diffing (`merkle::diff`, `Delta`) in `embedag_core::merkle` (EMBEDAG-79 / Ticket 9.1.2).
- Shared `rank_by_cosine` similarity helper (`similarity`), declarative boilerplate macros (`impl_node_payload!`, `impl_async_engine_repository!`, `storage_error!`, `edge_record!`), and opt-in `testing` feature with `TestNode` and `persistence_suite!`.
