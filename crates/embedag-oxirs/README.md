# embedag-oxirs

`embedag-oxirs` is the native RDF triple/quad and SPARQL 1.1/1.2 storage adapter for the `embedag` workspace, built on `oxirs-core`. It solves the problem of persisting `MemoryDag<T>` nodes and reified edges as W3C-standard RDF quads with zero external database daemons, crash-safe N-Quads append logs (`data.nq`), memory-mapped snapshots (`snapshot.oxsnap`), and full `embedag_core::traits::GraphRepository` support.

## When to use it

Use `embedag-oxirs` when you need:

- An embedded, zero-daemon semantic graph store in pure Rust with either transient in-memory storage (`StorageMode::InMemory`) or durable disk persistence (`StorageMode::Persistent(PathBuf)`).
- W3C RDF quad representation of domain nodes (`rdf:type`, `rdfs:label`, `embedag:businessKey`, `embedag:entityLevel`, `embedag:attributes`, `embedag:payload`) and reified edge metadata (`embedag:Edge`, `embedag:parent`, `embedag:child`, `embedag:relationship`, `embedag:confidence`, `embedag:provenance`).
- Direct SPARQL 1.1/1.2 queries (`OxiRsTdbEngine::query_sparql`) alongside `ReadOnlyGraphRepository`, `VectorSearchRepository`, and `TransactionalGraphRepository` trait implementations.
- Fast cold starts via memory-mapped snapshots (`save_snapshot`, `build_snapshot`).

Do not use `embedag-oxirs` when your application requires relational SQL foreign keys and `WITH RECURSIVE` SQL CTEs on existing application tables (use [`embedag-diesel`](../embedag-diesel/README.md)), a multi-model document/graph server (use [`embedag-surrealdb`](../embedag-surrealdb/README.md)), or an OpenCypher/Bolt property graph server (use [`embedag-neo4j`](../embedag-neo4j/README.md)).

## Main types and functions

- `OxiRsTdbConfig` and `StorageMode`:
  - `OxiRsTdbConfig::in_memory()` and `OxiRsTdbConfig::persistent(path)`.
  - Builder methods: `with_base_iri`, `with_schema_ns`, `with_named_graph`, `with_sync_policy` (`oxirs_core::rdf_store::SyncPolicy`).
- `OxiRsTdbEngine`:
  - Construction and lifecycle: `open(config)`, `open_persistent(path)`, `in_memory()`, `flush()`, `save_snapshot(path)`, `build_snapshot(dataset_dir)`.
  - Domain node and edge operations: `insert_node`, `get_node`, `remove_node_by_key`, `insert_edge`, `remove_edge`, `get_parents`, `get_children`.
  - Whole-DAG import/export: `export_dag(&MemoryDag<T>) -> Result<usize, OxiRsStorageError>` and `load_dag<T>() -> Result<MemoryDag<T>, OxiRsStorageError>` (validates acyclicity on load).
  - Vector search: `index_embedding(key, Vec<f32>)` and `find_nearest_neighbors<T>(query, limit)`.
  - Raw RDF & SPARQL: `insert_quad`, `bulk_insert_quads`, `remove_quad`, `find_quads`, `query_sparql(sparql)`.
  - Implements `ReadOnlyGraphRepository<T>`, `VectorSearchRepository<T>`, and `TransactionalGraphRepository<T>` (and thus `GraphRepository<T>`).
- `SchemaVocab` and mapping functions (`mapping`):
  - `SchemaVocab::new(schema_ns)`, `node_to_quads`, `edge_to_quads`, `reconstruct_node`, `make_node_iri`, `make_graph_name`.

### Example

```rust
use embedag_core::dag::MemoryDag;
use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_oxirs::OxiRsTdbEngine;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct OntologyNode { key: String, label: String }
embedag_core::impl_node_payload!(OntologyNode, key = |n| &n.key, name = |n| n.label.as_str(), level = |_| "concept");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = OxiRsTdbEngine::in_memory()?;
    let mut dag = MemoryDag::new();
    dag.upsert_node(OntologyNode { key: "rust".into(), label: "Rust Language".into() });
    dag.upsert_node(OntologyNode { key: "cargo".into(), label: "Cargo Package Manager".into() });
    dag.add_edge_by_keys(&"rust".into(), &"cargo".into(), EdgePayload::new("HAS_TOOL", 1.0, ProvenanceKind::DeterministicRule)?)?;

    engine.export_dag(&dag)?;
    let restored: MemoryDag<OntologyNode> = engine.load_dag()?;
    assert_eq!(restored.node_count(), 2);
    assert_eq!(restored.edge_count(), 1);

    let results = engine.query_sparql("SELECT ?s ?label WHERE { ?s <http://www.w3.org/2000/01/rdf-schema#label> ?label }")?;
    assert_eq!(results.len(), 2);
    Ok(())
}
```

## Feature flags

`embedag-oxirs` defines no optional Cargo feature flags (`default = []`).

## Workspace relationship

Within the workspace:

- `embedag-oxirs` depends on [`embedag-core`](../embedag-core/README.md) (`MemoryDag`, `NodePayload`, `EdgePayload`, `rank_by_cosine`, repository traits) and `oxirs-core`.
- [`embedag`](../embedag/README.md) re-exports `embedag-oxirs` under `embedag::oxirs` and `embedag::prelude` when the `oxirs-tdb` feature is enabled.


## Build and test

```bash
cargo build -p embedag-oxirs
cargo test -p embedag-oxirs
```

## Specifications

- [`doc/19_External_Ecosystem_and_KitaSan_Stack.md`](../../doc/19_External_Ecosystem_and_KitaSan_Stack.md) — `oxirs-core` integration within the workspace stack.
- [`doc/20_Multi_Database_Extensibility_and_Future_Adapters.md`](../../doc/20_Multi_Database_Extensibility_and_Future_Adapters.md) — Multi-database capability traits and `embedag-oxirs` architecture.
