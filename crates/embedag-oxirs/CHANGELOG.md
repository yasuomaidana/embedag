# Changelog

All notable changes to this crate are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0]

### Added

- `OxiRsTdbEngine` and `OxiRsTdbConfig` supporting `StorageMode::InMemory` and `StorageMode::Persistent(PathBuf)` with configurable `SyncPolicy`, base IRI, schema namespace, and named graph (Ticket 8.1.1).
- Bidirectional RDF quad mapping (`SchemaVocab`, `node_to_quads`, `edge_to_quads`, `reconstruct_node`) with lossless JSON payload round-tripping and reified edge metadata (`embedag:Edge`, confidence, provenance) (Ticket 8.1.1).
- SPARQL 1.1/1.2 query execution (`query_sparql`), memory-mapped snapshot persistence (`save_snapshot`, `build_snapshot`), `MemoryDag<T>` `export_dag` / `load_dag`, and `ReadOnlyGraphRepository`, `VectorSearchRepository`, and `TransactionalGraphRepository` trait implementations (Ticket 8.1.1).
