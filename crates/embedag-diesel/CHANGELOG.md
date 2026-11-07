# Changelog

All notable changes to this crate are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0]

### Added

- `MigrationEngine` providing embedded DDL schema initialization for PostgreSQL (`pgvector` HNSW + GIN indexes), SQLite, and Turso / libSQL (Ticket 5.1.1).
- Generic `UpsertDescriptor` and `ConflictStrategy` (`DoNothing`, `DoUpdate`, `CustomClause`), batch node/edge upserts (`batch_upsert_nodes`, `batch_upsert_edges`), `execute_with_savepoint`, and `apply_mutation_lifecycle` (Ticket 5.1.2, Ticket 5.1.3).
- `HierarchicalTableConfig`, `HierarchyItem`, `HierarchicalNode`, `HierarchicalModel`, and `CteHierarchyBuilder` built on `diesel-cte-ext` (`with_recursive` and `with_recursive_not_all`) for single-roundtrip ancestor and descendant traversals on SQLite, PostgreSQL (`diesel-async`), and Turso / libSQL (`diesel-libsql`) (Ticket 5.2.1).
- Per-node breadth windowing (`max_breadth_per_node` via `ROW_NUMBER() OVER (PARTITION BY ...)`) in recursive CTE descendant queries (Ticket 7.1.3).
- `PgVectorHarness` Testcontainers fixture (`pgvector/pgvector:pg16`) and containerized integration test suite verifying batch upserts, savepoint rollbacks, dynamic mutation lifecycles, and recursive CTE lineage queries against real PostgreSQL (Ticket 5.3.1, Ticket 5.3.2).
