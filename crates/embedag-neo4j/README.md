# embedag-neo4j

`embedag-neo4j` is the OpenCypher and Bolt protocol property-graph storage adapter for the `embedag` workspace, built on `neo4rs`. It solves the problem of persisting `MemoryDag<T>` nodes and typed relationships in a Neo4j or Memgraph database over the Bolt binary protocol, while also providing an in-process memory mode for local tests without a running Bolt server and implementing the `embedag-core` repository capability traits.

## When to use it

Use `embedag-neo4j` when your infrastructure runs Neo4j or Memgraph and you want to:

- Synchronize `MemoryDag<T>` nodes and edges with labeled property-graph nodes (`MERGE (n:EmbedagNode { business_key: $key })`) and typed Cypher relationships (`MERGE (p)-[r:REL_TYPE]->(c)`).
- Use the standard `ReadOnlyGraphRepository`, `VectorSearchRepository`, and `TransactionalGraphRepository` traits from `embedag-core` against a Bolt endpoint (`Neo4jConfig::bolt(uri, user, password)`).
- Run fast local unit tests without a live Neo4j daemon via `Neo4jEngine::in_memory()`.

Do not use `embedag-neo4j` when you want embedded file-backed storage without an external database server (use [`embedag-diesel`](https://docs.rs/embedag-diesel) with SQLite/Turso or [`embedag-oxirs`](https://docs.rs/embedag-oxirs) with `StorageMode::Persistent`).

## Main types and functions

- `Neo4jConfig` and `StorageMode`:
  - `Neo4jConfig::in_memory()` (defaults to database `"neo4j"`, node label `"EmbedagNode"`, default relationship `"EMBEDAG_RELATION"`).
  - `Neo4jConfig::bolt(uri, user, password)` (configures `StorageMode::Bolt(uri)`).
  - Builder methods: `with_database`, `with_node_label`, `with_default_relation`.
- `Neo4jEngine`:
  - Connection: `Neo4jEngine::open(config)` and `Neo4jEngine::in_memory()`.
  - Node and edge CRUD: `insert_node`, `get_node`, `remove_node_by_key` (`DETACH DELETE`), `insert_edge`, `remove_edge`, `get_parents`, `get_children`.
  - Vector search: `index_embedding(key, Vec<f32>)` and `find_nearest_neighbors<T>(query, limit)`.
  - Whole-DAG synchronization: `export_dag(&MemoryDag<T>)`, `load_dag<T>()`, and `TransactionalGraphRepository::apply_mutation_batch(&[MutationDirective<T>])`.
  - Implements `ReadOnlyGraphRepository<T>`, `VectorSearchRepository<T>`, and `TransactionalGraphRepository<T>` via `embedag_core::impl_async_engine_repository!`.
- `Neo4jNodeRecord` and `Neo4jEdgeRecord` (`mapping`):
  - `Neo4jNodeRecord::from_node`, `from_node_with_embedding`, and `to_node`.
  - `Neo4jEdgeRecord::from_edge`.

### Example

```rust
use embedag_core::dag::MemoryDag;
use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_neo4j::{Neo4jConfig, Neo4jEngine};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct AssetNode { key: String, name: String }
embedag_core::impl_node_payload!(AssetNode, key = |n| &n.key, name = |n| n.name.as_str(), level = |_| "asset");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Neo4jConfig::in_memory().with_node_label("Asset");
    let engine = Neo4jEngine::open(config).await?;

    let mut dag = MemoryDag::new();
    dag.upsert_node(AssetNode { key: "db".into(), name: "Primary Database".into() });
    dag.upsert_node(AssetNode { key: "replica".into(), name: "Read Replica".into() });
    dag.add_edge_by_keys(&"db".into(), &"replica".into(), EdgePayload::new("REPLICATES_TO", 1.0, ProvenanceKind::DeterministicRule)?)?;

    engine.export_dag(&dag).await?;
    let loaded: MemoryDag<AssetNode> = engine.load_dag().await?;
    assert_eq!(loaded.node_count(), 2);
    assert_eq!(loaded.edge_count(), 1);
    Ok(())
}
```

## Feature flags

`embedag-neo4j` defines no optional Cargo feature flags (`default = []`).

## Workspace relationship

Within the workspace:

- `embedag-neo4j` depends on [`embedag-core`](https://docs.rs/embedag-core) (`MemoryDag`, `NodePayload`, `EdgePayload`, `MutationDirective`, `rank_by_cosine`, repository traits) and `neo4rs`.
- [`embedag`](https://docs.rs/embedag) re-exports `embedag-neo4j` under `embedag::neo4j` and `embedag::prelude` when the `neo4j` feature is enabled.


## Build and test

```bash
# Run unit and in-memory tests
cargo test -p embedag-neo4j

# Run containerized Neo4j (`neo4j:5.26.0`) Bolt integration tests (requires Docker)
cargo test -p embedag-neo4j --test neo4j_testcontainers -- --include-ignored
```

## Specifications

- [`doc/20_Multi_Database_Extensibility_and_Future_Adapters.md`](https://github.com/yasuomaidana/embedag/blob/main/doc/20_Multi_Database_Extensibility_and_Future_Adapters.md) — Section 3 ("Neo4j & OpenCypher Property Graph Adapter (`embedag-neo4j`)").
