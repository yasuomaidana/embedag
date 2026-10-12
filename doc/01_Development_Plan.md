  - Comprehensive documentation with runnable doc-tests.
- **Tests**:
  - `cargo test --doc` across all crates.

#### Ticket 5.1.2: End-to-End Pipeline Integration Test
- **Description**: Implement full workflow test: Input struct $\to$ `#[derive(AsGraphContext)]` $\to$ extraction $\to$ `MemoryDag` $\to$ cycle check $\to$ Diesel Postgres/SQLite persistence $\to$ hybrid Graph-RAG query.
- **Acceptance Criteria**:
  - Complete round-trip executes without errors.
  - Persisted graph matches expected topological relationships.
  - Exercises multi-typed domain struct (scalars, booleans, timestamps, UUIDs, H3 cells, multimodal media blobs, options).
  - Validates active pure-Rust LLM inference with both `oxillama` (GGUF sampling) and `oxibonsai` (sub-2-bit ternary PDA masking).
  - Validates topological DAG invariants (multi-parent diamond DAG convergence, cycle prevention guardrails).
  - Validates two-stage hybrid GraphRAG (dense + sparse lexical FTS, RRF fusion, topological expansion with decay, grounded context packaging with citations).
  - Validates relational persistence layers on both PostgreSQL (`pgvector` via `testcontainers`) and SQLite (in-memory migrations up/down, foreign key constraint enforcement).
- **Tests**:
  - `tests/e2e_pipeline_test.rs`: Full automated test suite against ephemeral PostgreSQL container via `testcontainers` (https://rust.testcontainers.org) and in-memory SQLite.
  - `tests/test_inference_and_multitype_e2e.rs`: Multi-type, `oxillama` + `oxibonsai` inference, diamond DAG, hybrid Graph-RAG, and dual-database (Postgres + SQLite) integration suite.

---

## Milestone 6: High-Value Relevant Features (Visual Exporters, Migrations, Communities, Bitemporal, Multimedia & Semantic Web)
*Traceability: [20_Multi_Database_Extensibility_and_Future_Adapters.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/20_Multi_Database_Extensibility_and_Future_Adapters.md)*  
**Goal**: Implement schema evolution, visual exporters, topological community detection, bitemporal history, pure-Rust multimedia processing via `oximedia`, and W3C RDF-Star / SPARQL 1.2 via `oxirs`.  
**Target Crates**: `embedag-core`, `embedag-rag`, `embedag-diesel`.
