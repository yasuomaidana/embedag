# Changelog

All notable changes are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Security
- Audit: `RUSTSEC-2026-0459` (`libsqlite3-sys` 0.37, fixed in 0.39) is ignored with a reason in `.cargo/audit.toml` because diesel 2.3.14, the latest release, requires `libsqlite3-sys <0.39`. Remove the entry when diesel accepts 0.39 (EMBEDAG-113).

### Added
- CI: job `db-access (diesel only)` runs `scripts/check-db-access.sh` on every pull request and push: SQL is reached only through Diesel; the exceptions that exist today are in `.github/db-access-allow.txt` and removed by EMBEDAG-112 (EMBEDAG-110).
- CI: `.github/workflows/complexity.yml` cross-checks function complexity with clippy `cognitive_complexity` and `lizard`; report only while EMBEDAG-86 is open (EMBEDAG-85).

### Changed
- `embedag-example-receipts`: `run_receipt_categories_postgres_migration` takes `&mut diesel_async::AsyncPgConnection` instead of a `tokio_postgres::Client`, and the crate no longer depends on `tokio-postgres` (EMBEDAG-110).

## [0.1.0]

First release. Milestones 1 to 8 of the development plan are complete; Milestone 9 is partly done.

### Added
- `embedag-core`: domain contracts, the `MemoryDag` arena with cycle guards and typestate mutation plans, deterministic scalar, temporal and H3 geospatial encoders with a hybrid distance metric, an `embedag_core::testing` fixture module.
- `embedag::dag`: toposort, levels, reverse dependencies and critical path, and `embedag::dag::merkle` BLAKE3 Merkle delta trees (Milestone 9, ticket 9.1.1).
- `embedag-matcher`: structural AST similarity and similarity graphs.
- `embedag-core`: Iterative Tarjan strongly connected components (`strongly_connected_components`) and acyclic condensation API (`condense`, `Condensation`) for plain index graphs, re-exported under `embedag::dag` (Ticket 9.5.1).
- `embedag-derive`: `AsGraphContext`, `GraphTarget` and `HierarchicalModel` derives.
- `embedag-llm`: schema and GBNF derivation, constrained extraction, candle (GPU first), Ollama-compatible HTTP and oxillama inference, waterfall resolution, delta synthesizer.
- `embedag-diesel`: PostgreSQL (pgvector), SQLite and Turso / libSQL persistence, recursive-CTE traversals, dynamic graph mutations.
- `embedag-rag`: hybrid Graph-RAG with reciprocal rank fusion, topological distance decay and community detection.
- `embedag-oxirs`, `embedag-surrealdb`, `embedag-neo4j`: graph and RDF stores behind `GraphRepository`.
- `embedag-example-receipts`: receipt and invoice categorization example.
- Specifications for the dependency resolution API (`doc/22`) and for text embeddings and classification primitives (`doc/23`).

### Changed
- Repeated code removed across the workspace (clone groups 87 to 0): shared macros (`impl_node_payload!`, `impl_async_engine_repository!`), one SQL builder per dialect, one provider-parametrized receipt suite, tests on `rstest` 0.28.0.
- Files over 500 lines split into modules with the same public API (diesel `cte` and `ops`, receipts `finance`).
- CI runs on `develop` with a quality job (`loom-gate loc`, `clones`, `rstest`); tests that need Docker are ignored by default and run in the persistence jobs.
- Compatible dependency bumps (libsqlite3-sys, syn 3, parking_lot, jsonschema, diesel, tokio-postgres, pgvector, proptest, tempfile).

### Not in this release
- Milestone 9 open tickets: multi-root dependency manifests, work-item prioritisation, Merkle journal verification, Leiden clustering with a god-node index.
- Milestone 10 (text embeddings, vector search, classification primitives) is specified only.
- Rust test coverage is not gated yet.

### Security
- `.cargo/audit.toml` documents 15 advisories pinned by upstream crates (libsql, candle-core, oxirs-core, surrealdb, oxitls, ...); `libsql` stays at 0.9 and `testcontainers` at 0.27 because `diesel-libsql` and `testcontainers-modules` require them. A decision on the remaining ones is open (issue #2).
