# Changelog

All notable changes to this crate are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0]

### Added

- `SurrealDbEngine` and `SurrealDbConfig` supporting embedded in-memory (`StorageMode::InMemory` via `mem://`) and remote WebSocket/HTTP (`StorageMode::Remote`) connections with configurable namespace, database, node table, and edge table (Ticket 8.2.1).
- `SurrealNodeRecord` and `SurrealEdgeRecord` mapping domain `NodePayload` and `EdgePayload` instances to SurrealDB records with lossless JSON payload round-tripping (Ticket 8.2.1).
- Full `ReadOnlyGraphRepository`, `VectorSearchRepository`, and `TransactionalGraphRepository` trait implementations, `export_dag` / `load_dag` synchronization with `MemoryDag<T>`, and containerized `surrealdb/surrealdb:v2.1.4` Testcontainers integration tests (Ticket 8.2.1).
