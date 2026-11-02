# embedag-derive

`embedag-derive` is the procedural macro crate for the `embedag` workspace. It eliminates hand-written boilerplate when mapping Rust domain structs and enums into `embedag-core` multi-modal feature contexts (`#[derive(AsGraphContext)]`), LLM extraction target schemas (`#[derive(GraphTarget)]`), and `embedag-diesel` recursive CTE and relational CRUD implementations (`#[derive(HierarchicalModel)]`).

## When to use it

Use `embedag-derive` when you define domain structs or enums that participate in feature encoding, LLM prompt/schema generation, or relational adjacency-list hierarchies.

Do not use `#[derive(HierarchicalModel)]` on structs that do not live in a crate with access to `embedag-core`, `embedag-diesel`, `diesel`, `serde`, and `serde_json`, because the generated code references `embedag_diesel::cte::HierarchicalModel`, `embedag_core::traits::NodePayload`, and `diesel::sql_query`. If you only need `NodePayload` on a simple in-memory struct without relational persistence, use `embedag_core::impl_node_payload!` instead.

### Planned capabilities

- **Discriminator-scoped hierarchies and configurable `ON DELETE` (Ticket 10.4.1)**: Extending `#[hierarchical(...)]` with `scope = "<column>"` (for multi-tenant or per-project trees sharing a table) and `on_delete = "cascade" | "restrict" | "set_null"` is specified in [`doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md`](../../doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md) and is not yet implemented.

## Main macros and attributes

### 1. `#[derive(AsGraphContext)]`

Implements `embedag_core::traits::AsGraphContext` for a struct with named fields. Supported field attributes under `#[graph(...)]`:

- `#[graph(key)]`: Marks the unique business identifier field (returned by `context_key()` via `.to_string()`). Exactly one `#[graph(key)]` field is required.
- `#[graph(text)]`: Marks one or more fields combined into `text_repr()` (via `.to_string()`, joined with `" "` when multiple fields are annotated). At least one `#[graph(text)]` field is required.
- `#[graph(scalar)]` or `#[graph(scalar = "custom_name")]`: Inserts a numeric field (cast with `as f64`) into `scalar_features()`.
- `#[graph(time)]` or `#[graph(temporal)]`: Sets `timestamp_utc()` from a `chrono::DateTime<chrono::Utc>` field.
- `#[graph(geo)]` or `#[graph(geospatial)]`: Sets `geographic_coordinates()` from an `(f64, f64)` tuple field.
- `#[graph(tag)]` or `#[graph(tag = "custom_name")]`: Inserts a displayable field into `categorical_tags()`.
- `#[graph(blob)]` or `#[graph(blobs)]`: Collects `MultimodalBlob` fields into `multimodal_blobs()`.
- `#[graph(tokens)]` or `#[graph(sparse)]`: Clones a `Vec<(u32, f32)>` field for `sparse_lexical_tokens()`.

### 2. `#[derive(GraphTarget)]`

Generates conversion helpers (`into_graph_target`, `from_graph_target`, `From`, and `TryFrom`) between a domain enum or struct and `embedag_core::model::GraphTarget<String>`:

- On an enum, variants are matched by `#[target(exact | fuzzy | child | multi_child | root | ambiguous)]` or by variant name (`Exact*`, `Fuzzy*`, `Multi*`, `Child*`, `Root*`, `Ambiguous*`).
- On a struct with named fields, a `target` field (or field annotated with `#[target]`) generates `into_graph_target(self) -> GraphTarget<String>`.

### 3. `#[derive(HierarchicalModel)]`

Configured via `#[hierarchical(table = "...", id = "...", parent = "...", name = "...", description = "...")]` on a struct with named fields. Generates:

- `embedag_diesel::cte::HierarchicalModel` and `embedag_diesel::cte::HierarchicalNode`.
- `embedag_core::traits::NodePayload` (`Key = String`).
- `table_config()` and `table_context()`.
- Parameterized Diesel CRUD helpers:
  - SQLite (`diesel::SqliteConnection`): `insert_sqlite`, `insert_batch_sqlite`, `delete_by_id_sqlite`, `update_sqlite`.
  - PostgreSQL (`diesel::pg::PgConnection` and `diesel_async::AsyncPgConnection`): `insert_pg`, `insert_batch_pg`, `delete_by_id_pg`, `update_pg`, `insert_pg_async`, `insert_batch_pg_async`, `delete_by_id_pg_async`, `update_pg_async`.

### Example

```rust
use embedag_core::model::GraphTarget;
use embedag_core::traits::AsGraphContext;
use embedag_derive::{AsGraphContext, GraphTarget};

#[derive(AsGraphContext)]
struct PurchaseReceipt {
    #[graph(key)] receipt_id: String,
    #[graph(text)] merchant_name: String,
    #[graph(scalar = "total_usd")] total: f64,
    #[graph(geo)] coords: (f64, f64),
    #[graph(tag = "channel")] store_type: String,
}

#[derive(Debug, PartialEq, GraphTarget)]
enum PlacementDecision {
    ExactMatch(String),
    NewRoot,
}

fn main() {
    let receipt = PurchaseReceipt {
        receipt_id: "rcpt-01".into(),
        merchant_name: "Corner Market".into(),
        total: 42.50,
        coords: (37.7749, -122.4194),
        store_type: "grocery".into(),
    };
    assert_eq!(receipt.context_key(), "rcpt-01");
    assert_eq!(receipt.text_repr(), "Corner Market");
    assert_eq!(receipt.scalar_features().get("total_usd"), Some(&42.50));
    assert_eq!(receipt.geographic_coordinates(), Some((37.7749, -122.4194)));
    assert_eq!(receipt.categorical_tags().get("channel").map(String::as_str), Some("grocery"));

    let target: GraphTarget<String> = PlacementDecision::ExactMatch("cat_food".into()).into_graph_target();
    assert_eq!(target, GraphTarget::ExactMatch("cat_food".into()));
    assert_eq!(PlacementDecision::from_graph_target(GraphTarget::NewRoot), Ok(PlacementDecision::NewRoot));
}
```

## Feature flags

`embedag-derive` is a `proc-macro = true` crate and defines no Cargo feature flags. Note that the SQLite and PostgreSQL CRUD helper methods emitted by `#[derive(HierarchicalModel)]` are gated on `#[cfg(feature = "sqlite")]` and `#[cfg(feature = "postgres-async")]` in the **consumer** crate.

## Workspace and Dendrite relationship

Within the workspace:

- `embedag-derive` generates trait implementations defined in [`embedag-core`](../embedag-core/README.md) (`AsGraphContext`, `GraphTarget`, `NodePayload`) and [`embedag-diesel`](../embedag-diesel/README.md) (`HierarchicalModel`, `HierarchicalNode`).
- [`embedag-example-receipts`](../embedag-example-receipts/README.md) uses `#[derive(AsGraphContext)]`, `#[derive(GraphTarget)]`, and `#[derive(HierarchicalModel)]` across its receipt and multi-DAG finance taxonomy models.
- [`embedag`](../embedag/README.md) re-exports all three derive macros under `embedag::derive` and `embedag::prelude`.

Across the Dendrite repositories (`dendrite-core-rs`, `dendrite-git-rs`, `dendrite-rag-rs`, `dendrite-plane-rs`, and the Dendrite control plane), `embedag-derive` is used by control-plane and RAG domain models that project task, ticket, or document metadata into `GraphContext` or relational adjacency tables.

## Build and test

```bash
cargo build -p embedag-derive
cargo test -p embedag-derive
```

Macro expansion behavior is also tested in `embedag-core`, `embedag-diesel`, and `embedag-example-receipts`.

## Specifications

- [`doc/06_Procedural_Derive_Macros.md`](../../doc/06_Procedural_Derive_Macros.md) — Macro syntax, AST parsing rules, and generated trait implementations.
- [`doc/16_Hierarchical_Recursive_CTE_Traversals.md`](../../doc/16_Hierarchical_Recursive_CTE_Traversals.md) — `#[derive(HierarchicalModel)]` and `HierarchicalTableConfig` integration.
- [`doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md`](../../doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md) — Planned `#[hierarchical(scope = ..., on_delete = ...)]` extensions (Ticket 10.4.1).
