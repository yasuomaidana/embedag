# embedag

Deterministic building blocks for graph-aware retrieval in Rust: an in-memory DAG with cycle guards, feature encoders for mixed modalities, constrained LLM extraction, relational and graph-database persistence, and a two-stage hybrid Graph-RAG engine. Everything that decides something is deterministic and testable; the LLM only fills in what a schema allows.

Version 0.1.0. Milestones 1 to 8 are complete. Milestone 9 (dependency resolution and code-graph APIs) is partly done and Milestone 10 (text embeddings, vector search, classification) is specified only: see [DEV_STATUS.md](DEV_STATUS.md) and [CHANGELOG.md](CHANGELOG.md) for exactly what is and is not there.

## Crates

| Crate | Role |
|---|---|
| `embedag-core` | domain contracts, the `MemoryDag` arena, cycle guards, typestate mutation plans, deterministic scalar, temporal and H3 encoders, DAG algorithms (toposort, levels, reverse dependencies, critical path), BLAKE3 Merkle delta trees |
| `embedag-derive` | derive macros `AsGraphContext`, `GraphTarget`, `HierarchicalModel` |
| `embedag-llm` | JSON-schema and GBNF derivation, constrained extraction, candle / oxillama / HTTP inference, multi-tier waterfall resolution, delta synthesis |
| `embedag-diesel` | relational persistence: PostgreSQL with pgvector, SQLite and Turso / libSQL, recursive-CTE traversals, graph mutations |
| `embedag-rag` | two-stage hybrid Graph-RAG: reciprocal rank fusion, topological distance decay, community detection |
| `embedag-matcher` | language-agnostic structural AST similarity and similarity graphs |
| `embedag-oxirs`, `embedag-surrealdb`, `embedag-neo4j` | graph and RDF stores behind the same `GraphRepository` contract |
| `embedag` | the facade that re-exports the crates behind features |
| `embedag-example-receipts` | a worked example: receipt categorization with custom migrations and LLM-generated graphs |

## Use

```toml
# a slim consumer: only the DAG algorithms and the AST matcher
embedag = { git = "https://github.com/yasuomaidana/embedag", branch = "develop", default-features = false, features = ["core", "matcher"] }
```

Features of the `embedag` facade: `core`, `matcher`, `derive`, `llm`, `diesel`, `turso`, `turso-async`, `rag`, `oxirs-tdb`, `surrealdb`, `neo4j`; `full` (the default) turns all of them on. `dendrite-core-rs` and `dendrite-rag-rs` use `core` and `matcher`.

```rust
use embedag::dag; // toposort, levels, reverse_deps, critical_path; embedag::dag::merkle for delta trees
```

## Build and check

```bash
cargo build --workspace
cargo test --workspace            # tests that start a Postgres container are ignored by default
cargo test --workspace -- --include-ignored   # with Docker available
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings
```

Quality gates (`loom-gate loc`, `clones`, `rstest`) run in CI; tests use the `rstest` crate. Known upstream advisories are listed in `.cargo/audit.toml`.

## Specifications

`doc/00_MASTER_SPECIFICATIONS.md` indexes the specifications in `doc/`; `doc/22_Dependency_Resolution_and_Merkle_Delta_API.md` defines the slim DAG facade other projects consume.
