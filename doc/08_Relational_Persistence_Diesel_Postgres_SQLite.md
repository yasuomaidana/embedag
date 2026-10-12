# Relational Persistence with Diesel: PostgreSQL & SQLite

## 1. Relational Schema Architecture for DAGs
In a strict tree, each node contains a single nullable `parent_id`. However, in a Directed Acyclic Graph (DAG), nodes can have multiple parents (e.g. multi-taxonomy classifications). 

`embedag` normalizes persistence into two primary tables (`semantic_nodes` and `semantic_edges`) plus an auxiliary alias cache table (`semantic_aliases`).

```sql
-- PostgreSQL Schema
CREATE TABLE semantic_nodes (
    id UUID PRIMARY KEY,
    entity_level VARCHAR(64) NOT NULL,
    name VARCHAR(255) NOT NULL,
    attributes JSONB NOT NULL DEFAULT '{}'::jsonb,
    embedding vector(1024), -- pgvector for dense text embeddings
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE semantic_edges (
    parent_id UUID NOT NULL REFERENCES semantic_nodes(id) ON DELETE CASCADE,
    child_id UUID NOT NULL REFERENCES semantic_nodes(id) ON DELETE CASCADE,
    relationship_type VARCHAR(64) NOT NULL,
    confidence REAL NOT NULL DEFAULT 1.0,
    provenance VARCHAR(32) NOT NULL DEFAULT 'EXTRACTED',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (parent_id, child_id, relationship_type)
);

CREATE TABLE semantic_aliases (
    normalized_alias VARCHAR(255) PRIMARY KEY,
    target_node_id UUID NOT NULL REFERENCES semantic_nodes(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indices for high-speed graph traversal
CREATE INDEX idx_edges_child ON semantic_edges (child_id);
CREATE INDEX idx_edges_parent ON semantic_edges (parent_id);
CREATE INDEX idx_nodes_name ON semantic_nodes (name);
```

### SQLite Equivalent Schema
For SQLite (edge/offline environments):
- UUIDs are stored as `TEXT` or 16-byte `BLOB`.
- `attributes` is stored as `TEXT` (validated via `json_valid()`).
- `embedding` is stored as `BLOB` (serialized via `zerocopy`), with nearest neighbors queried either through in-memory cosine or vector virtual tables.
- SQLite foreign key constraints are enforced via `PRAGMA foreign_keys = ON;`.

---

## 2. Multi-DB Strategy & Asynchronous Transactions

### 2.1 Asynchronous PostgreSQL with `diesel-async`
For high-throughput server backends:
- Uses `diesel-async` with connection pools (`bb8` or `deadpool`).
- Non-blocking I/O over Tokio event loops prevents blocking thread pools.
- Enforces strict transactional integrity via `conn.transaction(|conn| async move { ... })`.

### 2.2 Subtransactions via SAVEPOINT
PostgreSQL does not permit nested `BEGIN` statements. `diesel-async`’s `TransactionManager` abstracts nested calls into `SAVEPOINT`:
- If an inner node insertion fails, only the savepoint is rolled back, preventing connection poisoning.

### 2.3 SQLite Operations
For embedded edge deployments:
- Standard `diesel::sqlite::SqliteConnection` or pooled connections via `r2d2`.
- When called from async contexts, SQLite operations are safely executed on dedicated blocking worker threads (`tokio::task::spawn_blocking`).
- Turso / libSQL integration is supported as an optional downstream feature.

---

## 3. High-Throughput Batch Upsert Strategies
To eliminate the network and planning overhead of serial row inserts:
1. **Nodes Upsert**:
   ```rust
   diesel::insert_into(semantic_nodes::table)
       .values(&flattened_nodes)
       .on_conflict(semantic_nodes::id)
       .do_update()
       .set((
           semantic_nodes::attributes.eq(excluded(semantic_nodes::attributes)),
           semantic_nodes::updated_at.eq(chrono::Utc::now()),
       ))
       .execute(conn)
       .await?;
   ```
2. **Edges Upsert**:
   ```rust
   diesel::insert_into(semantic_edges::table)
       .values(&flattened_edges)
       .on_conflict((semantic_edges::parent_id, semantic_edges::child_id, semantic_edges::relationship_type))
       .do_nothing()
       .execute(conn)
       .await?;
   ```

---

## 4. Arbitrary-Depth Recursive CTE Traversals via `diesel-cte-ext`
To query entire subgraphs (descendants or ancestors) in a single round-trip without application-level N+1 query loops, `embedag` leverages recursive Common Table Expressions (`WITH RECURSIVE`).

### 4.1 Integration with `diesel-cte-ext`
Rather than formatting fragile raw SQL strings, `embedag-diesel` utilizes `diesel-cte-ext` to construct type-checked recursive queries compatible with both PostgreSQL and SQLite:

```rust
use diesel_cte_ext::{RecursiveParts, RecursiveCTEExt, seed_query, step_query, cte_query};

// Anchor: Find starting node
let seed = semantic_nodes::table
    .filter(semantic_nodes::id.eq(root_node_id))
    .select((
        semantic_nodes::id,
        0.into_sql::<diesel::sql_types::Integer>(),
    ));

// Recursive Step: Follow directed edges downwards
let step = semantic_edges::table
    .inner_join(dag_cte::table.on(semantic_edges::parent_id.eq(dag_cte::id)))
    .inner_join(semantic_nodes::table.on(semantic_edges::child_id.eq(semantic_nodes::id)))
    .select((
        semantic_nodes::id,
        dag_cte::depth + 1,
    ));

// Outer Selection with max depth limit
let parts = RecursiveParts::new(
    seed_query!(seed),
    step_query!(step),
    cte_query!(dag_cte::table.filter(dag_cte::depth.le(max_depth as i32))),
);
```

### 4.2 Cycle-Defensive CTEs in SQL
Even though the in-memory arena validates DAG acyclic properties before writes, defensive SQL prevents infinite loops in cases of external manual edits:
- **PostgreSQL**: Accumulates an `id_path` array: `WHERE NOT (child_id = ANY(id_path))`.
- **SQLite**: Appends to a comma-separated path string: `WHERE instr(path, child_id) = 0`.

---

## 5. The Storage Contract: `GraphRepository` Trait
The persistence layer is completely decoupled from domain logic via the `GraphRepository` trait defined in `embedag-core`:

```rust
#[async_trait::async_trait]
pub trait GraphRepository: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    async fn get_node(&self, id: &uuid::Uuid) -> Result<Option<PersistedNode>, Self::Error>;
    
    async fn fetch_subgraph(
        &self,
        root_id: &uuid::Uuid,
        direction: TraversalDirection,
        max_depth: usize,
    ) -> Result<petgraph::graph::DiGraph<PersistedNode, PersistedEdge>, Self::Error>;

    async fn persist_graph_delta(
        &self,
        nodes: &[PersistedNode],
        edges: &[PersistedEdge],
    ) -> Result<(), Self::Error>;

    async fn find_similar_nodes(
        &self,
        embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<(PersistedNode, f32)>, Self::Error>;

    async fn lookup_alias(&self, alias: &str) -> Result<Option<uuid::Uuid>, Self::Error>;
    async fn register_alias(&self, alias: &str, node_id: uuid::Uuid) -> Result<(), Self::Error>;
}
```
