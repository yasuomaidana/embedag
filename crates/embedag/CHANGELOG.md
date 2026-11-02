# Changelog

All notable changes to this crate are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0]

### Added

- Top-level workspace facade re-exporting `embedag-core`, `embedag-derive`, `embedag-matcher`, `embedag-llm`, `embedag-diesel`, `embedag-rag`, `embedag-oxirs`, `embedag-surrealdb`, and `embedag-neo4j` behind granular Cargo feature flags and a default `full` feature (Ticket 1.1.1, Ticket 8.1.1).
- Unified `embedag::prelude` module exposing core traits, typestate mutation builders, derive macros, structural AST matchers, LLM waterfall extractors, CTE hierarchy builders, hybrid Graph-RAG engines, and storage adapters.
- End-to-end integration test suite (`tests/e2e_pipeline.rs`) verifying mixed-modality feature encoding, constrained LLM extraction, waterfall resolution, typestate cycle validation, and hybrid Graph-RAG retrieval.
