# Changelog

All notable changes to this crate are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0]

### Added

- `Neo4jEngine` and `Neo4jConfig` supporting Bolt protocol connections (`StorageMode::Bolt` via `neo4rs`) and zero-daemon local execution (`StorageMode::InMemoryMock`) with configurable database name, node label, and default relationship type (Ticket 8.3.1).
- `Neo4jNodeRecord` and `Neo4jEdgeRecord` property mappings with lossless JSON payload round-tripping and OpenCypher `MERGE` / `DETACH DELETE` query generation (Ticket 8.3.1).
- Full `ReadOnlyGraphRepository`, `VectorSearchRepository`, and `TransactionalGraphRepository` trait implementations, `export_dag` / `load_dag` synchronization with `MemoryDag<T>`, and containerized `neo4j:5.26.0` Testcontainers integration tests (Ticket 8.3.1).
