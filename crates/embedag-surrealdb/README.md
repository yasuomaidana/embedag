# embedag-surrealdb

`embedag-surrealdb` is the multi-model document and graph storage adapter for the `embedag` workspace, built on the official `surrealdb` SDK (`kv-mem` and `protocol-ws`). It solves the problem of persisting `MemoryDag<T>` domain nodes and directed edges in either an embedded in-process SurrealDB instance (`mem://`) or a remote SurrealDB server over WebSocket/HTTP, while implementing the `embedag-core` repository capability traits (`ReadOnlyGraphRepository`, `VectorSearchRepository`, `TransactionalGraphRepository`).

## When to use it

Use `embedag-surrealdb` when your application targets SurrealDB as its multi-model document and graph store and you want:

- Zero-daemon in-process execution (`SurrealDbEngine::in_memory()`) for local development and tests, with seamless switching to a remote WebSocket/HTTP server (`SurrealDbConfig::remote("ws://127.0.0.1:8000")`) in production.
- Configurable namespace, database, node table, and edge table names (`SurrealDbConfig`).
- Bidirectional `MemoryDag<T>` synchronization (`export_dag`, `load_dag`) and `GraphRepository<T>` trait compatibility.

Do not use `embedag-surrealdb` when you need SQL `WITH RECURSIVE` CTEs over existing PostgreSQL/SQLite tables (use [`embedag-diesel`](https://docs.rs/embedag-diesel)), W3C RDF/SPARQL queries (use [`embedag-oxirs`](https://docs.rs/embedag-oxirs)), or OpenCypher over Bolt (use [`embedag-neo4j`](https://docs.rs/embedag-neo4j)).

## Main types and functions

- `SurrealDbConfig` and `StorageMode`:
  - `SurrealDbConfig::in_memory()` (defaults to namespace `"embedag"`, database `"knowledge_graph"`, node table `"node"`, edge table `"graph_edge"`).
  - `SurrealDbConfig::remote(endpoint)`.
  - Builder methods: `with_namespace`, `with_database`, `with_node_table`, `with_edge_table`, `with_credentials(user, pass)`.
- `SurrealDbEngine`:
  - Connection: `SurrealDbEngine::open(config)` and `SurrealDbEngine::in_memory()`.
  - Node and edge CRUD: `insert_node`, `get_node`, `remove_node_by_key`, `insert_edge`, `remove_edge`, `get_parents`, `get_children`.
  - Vector search: `index_embedding(key, Vec<f32>)` and `find_nearest_neighbors<T>(query, limit)`.
  - Whole-DAG synchronization: `export_dag(&MemoryDag<T>)`, `load_dag<T>()`, and `TransactionalGraphRepository::apply_mutation_batch(&[MutationDirective<T>])`.
  - Implements `ReadOnlyGraphRepository<T>`, `VectorSearchRepository<T>`, and `TransactionalGraphRepository<T>` via `embedag_core::impl_async_engine_repository!`.
- `SurrealNodeRecord` and `SurrealEdgeRecord` (`mapping`):
  - `SurrealNodeRecord::from_node`, `from_node_with_embedding`, and `to_node`.
  - `SurrealEdgeRecord::from_edge`.

### Example

```rust,no_run
use embedag_core::dag::MemoryDag;
use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_surrealdb::SurrealDbEngine;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ServiceNode { id: String, name: String }
embedag_core::impl_node_payload!(ServiceNode, key = |n| &n.id, name = |n| n.name.as_str(), level = |_| "service");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = SurrealDbEngine::in_memory().await?;
    let mut dag = MemoryDag::new();
    dag.upsert_node(ServiceNode { id: "auth".into(), name: "Auth Service".into() });
    dag.upsert_node(ServiceNode { id: "api".into(), name: "API Gateway".into() });
    dag.add_edge_by_keys(&"auth".into(), &"api".into(), EdgePayload::new("UPSTREAM_OF", 1.0, ProvenanceKind::DeterministicRule)?)?;

    engine.export_dag(&dag).await?;
    let loaded: MemoryDag<ServiceNode> = engine.load_dag().await?;
    assert_eq!(loaded.node_count(), 2);
    assert_eq!(loaded.edge_count(), 1);

    let children: Vec<ServiceNode> = engine.get_children(&"auth".to_string()).await?;
    assert_eq!(children[0].id, "api");
    Ok(())
}
```

## Feature flags

`embedag-surrealdb` defines no optional Cargo feature flags (`default = []`). The underlying `surrealdb` dependency is compiled with `kv-mem` and `protocol-ws` enabled at the workspace level.

## Workspace relationship

Within the workspace:

- `embedag-surrealdb` depends on [`embedag-core`](https://docs.rs/embedag-core) (`MemoryDag`, `NodePayload`, `EdgePayload`, `MutationDirective`, `rank_by_cosine`, repository traits) and `surrealdb`.
- [`embedag`](https://docs.rs/embedag) re-exports `embedag-surrealdb` under `embedag::surrealdb` and `embedag::prelude` when the `surrealdb` feature is enabled.


## Build and test

```bash
# Run unit and in-memory SurrealDB (`kv-mem`) tests
cargo test -p embedag-surrealdb

# Run containerized SurrealDB (`surrealdb/surrealdb:v2.1.4`) integration tests (requires Docker)
cargo test -p embedag-surrealdb --test surrealdb_testcontainers -- --include-ignored
```

## Specifications

- [`doc/20_Multi_Database_Extensibility_and_Future_Adapters.md`](https://github.com/yasuomaidana/embedag/blob/main/doc/20_Multi_Database_Extensibility_and_Future_Adapters.md) — Section 2 ("SurrealDB Multi-Model Engine (`embedag-surrealdb`)").
