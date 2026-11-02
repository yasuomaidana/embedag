# embedag-diesel

`embedag-diesel` is the relational persistence and hierarchical query adapter for the `embedag` workspace. It solves the problem of storing semantic graph nodes and edges in PostgreSQL (with `pgvector`), SQLite, and Turso / libSQL, executing single-roundtrip `WITH RECURSIVE` ancestor and descendant lineage traversals via `diesel-cte-ext`, and applying batch upserts and savepoint-isolated graph mutations.

## When to use it

Use `embedag-diesel` when your application stores graph nodes, edges, or adjacency-list hierarchies in a relational SQL database (PostgreSQL, SQLite, or Turso / libSQL) and needs:

- Dialect-aware `WITH RECURSIVE` CTE ancestor and descendant traversals (`CteHierarchyBuilder`, `HierarchicalTableConfig`, `HierarchicalModel`) with depth bounds (`max_depth`), per-node breadth caps (`max_breadth_per_node`), and cycle deduplication (`fetch_ancestors_dedup_sqlite`).
- Structure-agnostic `INSERT ... ON CONFLICT` SQL generation (`UpsertDescriptor`, `ConflictStrategy`) and batch node/edge upserts (`batch_upsert_nodes`, `batch_upsert_edges`, `batch_upsert_nodes_turso`, `batch_upsert_edges_turso`).
- Savepoint-wrapped transaction execution (`execute_with_savepoint`, `execute_with_savepoint_turso`) and atomic mutation lifecycles (`apply_mutation_lifecycle`, `apply_mutation_lifecycle_turso`).
- Embedded DDL schema initialization (`MigrationEngine`) and containerized PostgreSQL `pgvector/pgvector:pg16` integration testing (`PgVectorHarness`).

Do not use `embedag-diesel` if you want a pure RDF/SPARQL triple store (use [`embedag-oxirs`](../embedag-oxirs/README.md)), a document-graph store with `RELATE` edges (use [`embedag-surrealdb`](../embedag-surrealdb/README.md)), or a Bolt/OpenCypher property graph (use [`embedag-neo4j`](../embedag-neo4j/README.md)).

### Implemented vs. planned capabilities

- **`VectorSearchRepository` on PostgreSQL / SQLite (Ticket 10.1.2)**: While `MigrationEngine::POSTGRES_UP_SQL` provisions `embedding vector(768)` and an HNSW `vector_cosine_ops` index on `semantic_nodes` (and `NodeUpsertRecord` carries `embedding: Option<Vec<f32>>`), `embedag-diesel` does not yet implement `embedag_core::traits::VectorSearchRepository<T>` (`ORDER BY embedding <=> $1 LIMIT $2` on PostgreSQL and brute-force cosine scan over BLOB vectors on SQLite/Turso). That implementation is planned in [`doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md`](../../doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md) (§4, Ticket 10.1.2).

## Main types and functions

- **Recursive CTE hierarchy traversals (`cte`, `traversal`)**:
  - `HierarchicalTableConfig`: Configures any adjacency-list table (`table_name`, `id_column`, `parent_column`, `name_column`, optional `description` and `max_breadth_per_node`).
  - `HierarchyItem`: Returned lineage entry (`id: String`, `depth: i32`, `name: String`, where `depth >= 1`).
  - `HierarchicalNode` and `HierarchicalModel`: Traits implemented manually or via `#[derive(HierarchicalModel)]` from [`embedag-derive`](../embedag-derive/README.md).
  - `CteHierarchyBuilder`:
    - SQLite (`feature = "sqlite"`): `fetch_ancestors_sqlite`, `fetch_ancestors_dedup_sqlite`, `fetch_descendants_sqlite`, `fetch_model_ancestors_sqlite`, `fetch_model_descendants_sqlite`, `fetch_model_ancestors_dedup_sqlite`.
    - PostgreSQL async (`features = ["postgres-async", "diesel-async"]`): `fetch_ancestors_pg_async`, `fetch_descendants_pg_async`, `fetch_model_ancestors_pg_async`, `fetch_model_descendants_pg_async`.
    - Turso / libSQL async (`feature = "turso-async"`): `fetch_ancestors_turso_async`, `fetch_descendants_turso_async`, `fetch_model_ancestors_turso_async`, `fetch_model_descendants_turso_async`.
  - `RecursiveTraversalBuilder`: Cycle-guarded recursive CTE queries (`postgres_ancestor_query`, `postgres_descendant_query`, `fetch_ancestors`, `fetch_descendants`, `fetch_ancestors_sqlite`, `fetch_descendants_sqlite`).
- **Batch upserts and mutations (`ops`)**:
  - `UpsertDescriptor`, `UpsertProcessor`, and `ConflictStrategy` (`DoNothing`, `DoUpdate`, `CustomClause`).
  - `NodeUpsertRecord`, `EdgeUpsertRecord`, `MutationDirectiveRecord`.
  - PostgreSQL (`feature = "postgres-async"`): `execute_with_savepoint`, `batch_upsert_nodes`, `batch_upsert_edges`, `apply_mutation_lifecycle`.
  - Turso / libSQL (`feature = "turso-async"`): `execute_with_savepoint_turso`, `batch_upsert_nodes_turso`, `batch_upsert_edges_turso`, `apply_mutation_lifecycle_turso`.
- **Migrations and Testcontainers harness (`migrations`, `harness`)**:
  - `POSTGRES_SETUP`, `SQLITE_SETUP`, `TURSO_SETUP`, `setup_sqlite`, `run_sqlite_migration`, `setup_postgres`, `run_postgres_migration`, `TursoConnectionMode`.
  - `PostgresHarness` (`features = ["postgres-async", "test-harness"]`) and `TursoHarness` (`feature = "turso"`).

### Example

```rust
use diesel::Connection;
use diesel::sqlite::SqliteConnection;
use embedag_diesel::cte::{CteHierarchyBuilder, HierarchicalTableConfig};
use embedag_diesel::migrations::run_sqlite_migration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = SqliteConnection::establish(":memory:")?;
    run_sqlite_migration(
        &mut conn,
        "CREATE TABLE semantic_nodes (id TEXT PRIMARY KEY, parent_id TEXT, name TEXT NOT NULL);
         INSERT INTO semantic_nodes (id, parent_id, name) VALUES
         ('root', NULL, 'Food'),
         ('child', 'root', 'Chocolate'),
         ('leaf', 'child', 'Dark Chocolate');",
    )?;

    let config = HierarchicalTableConfig::default_semantic();
    let ancestors = CteHierarchyBuilder::fetch_ancestors_sqlite(&mut conn, &config, "leaf", 5)?;
    assert_eq!(ancestors.len(), 2);
    assert_eq!(ancestors[0].id, "child");
    assert_eq!(ancestors[1].id, "root");

    let descendants = CteHierarchyBuilder::fetch_descendants_sqlite(&mut conn, &config, "root", 5)?;
    assert_eq!(descendants.len(), 2);
    assert_eq!(descendants[0].id, "child");
    assert_eq!(descendants[1].id, "leaf");
    Ok(())
}
```

## Feature flags

| Feature | Default | Description |
|---|---|---|
| `postgres-async` | Yes | Enables `diesel-async`, `pgvector`, `tokio-postgres`, and `diesel-cte-ext/postgres` + `async` |
| `sqlite` | Yes | Enables bundled SQLite (`libsqlite3-sys`) and `diesel-cte-ext/sqlite` |
| `test-harness` | Yes | Enables `testcontainers` and `PgVectorHarness` for containerized PostgreSQL 16 + `pgvector` tests |
| `turso` | No | Enables `diesel-libsql`, `libsql`, and `tempfile` for Turso / libSQL support |
| `turso-async` | No | Enables `turso`, `diesel-async`, and `diesel-libsql/async` for async Turso / libSQL CTEs and mutations |

## Workspace and Dendrite relationship

Within the workspace:

- `embedag-diesel` depends on [`embedag-core`](../embedag-core/README.md).
- [`embedag-derive`](../embedag-derive/README.md) generates implementations of `embedag_diesel::cte::HierarchicalModel` and `HierarchicalNode`.
- [`embedag-example-receipts`](../embedag-example-receipts/README.md) uses `embedag-diesel` to persist and traverse receipt and financial category hierarchies across SQLite and PostgreSQL.
- [`embedag`](../embedag/README.md) re-exports `embedag-diesel` under `embedag::diesel` and forwards the `turso` and `turso-async` feature flags.

Across the Dendrite repositories (`dendrite-core-rs`, `dendrite-git-rs`, `dendrite-rag-rs`, `dendrite-plane-rs`, and the Dendrite control plane), `embedag-diesel` provides the relational persistence and recursive CTE traversal engine for control-plane hierarchies stored in SQLite, Turso / libSQL, or PostgreSQL.

## Build and test

```bash
# Run unit and in-memory SQLite / Turso tests
cargo test -p embedag-diesel --all-features

# Run containerized PostgreSQL + pgvector integration tests (requires Docker daemon)
cargo test -p embedag-diesel --test pgvector_integration -- --include-ignored
```

## Specifications

- [`doc/15_Relational_Persistence_and_Schema_Engine.md`](../../doc/15_Relational_Persistence_and_Schema_Engine.md) — Schema DDL, `UpsertDescriptor`, batch upserts, and savepoints.
- [`doc/16_Hierarchical_Recursive_CTE_Traversals.md`](../../doc/16_Hierarchical_Recursive_CTE_Traversals.md) — `CteHierarchyBuilder`, `HierarchicalTableConfig`, and `diesel-cte-ext` integration.
- [`doc/21_Heavy_Graph_Handling_and_Topological_Pagination.md`](../../doc/21_Heavy_Graph_Handling_and_Topological_Pagination.md) — Bounded CTE depth and per-node breadth windowing (`ROW_NUMBER() OVER (PARTITION BY ...)`).
- [`doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md`](../../doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md) — Planned `VectorSearchRepository` implementation for PostgreSQL `pgvector` and SQLite (Ticket 10.1.2).
