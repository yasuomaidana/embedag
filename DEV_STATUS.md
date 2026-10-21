# Project Status & Ticket Log

**Progress:** 55/71 completed (77%) — **Milestones 1–8 COMPLETE**; Milestone 9 (dependency resolution & code-graph APIs) in progress; Milestone 10 (embeddings, vector search & classification primitives) planned
**Current Focus:** All milestone development tickets completed (`Milestone 7` delivered: `embedag-surrealdb` & `embedag-neo4j`)

---

### Milestone 1: Core Domain, In-Memory DAG Arena & Derive Macros
- [x] Ticket 1.1.1: Core Traits & Data Representations (`embedag-core`)
- [x] Ticket 1.1.2: In-Memory DAG Arena (`MemoryDag`) via `petgraph` (`embedag-core`)
- [x] Ticket 1.1.3: Cycle Detection & Acyclic Guardrails (`embedag-core`)
- [x] Ticket 1.1.4: Typestate Mutation Plan (`embedag-core`)
- [x] Ticket 1.2.1: Continuous Scalar Encoders & Log Scaling (`embedag-core`)
- [x] Ticket 1.2.2: Cyclical Temporal Encoders (`embedag-core`)
- [x] Ticket 1.2.3: Uber H3 Geospatial Indexing via `h3o` (`embedag-core`)
- [x] Ticket 1.2.4: Composite Hybrid Distance Metric (`embedag-core`)
- [x] Ticket 1.3.1: `#[derive(AsGraphContext)]` (`embedag-derive`)
- [x] Ticket 1.3.2: `#[derive(GraphTarget)]` (`embedag-derive`)

### Milestone 2: Constrained LLM Extraction & Waterfall Resolution
- [x] Ticket 2.1.1: Universal JSON Schema Generation with `schemars` (`embedag-llm`)
- [x] Ticket 2.1.2: GBNF Grammar Generation & Pushdown Automata (`embedag-llm`)
- [x] Ticket 2.1.3: Draft-Then-Decode Envelope & Prompt Formatting with Field Context (`embedag-llm`)
- [x] Ticket 2.1.4: Evaluation of `oxibonsai` vs sovereign `oxillama` & `candle` (`embedag-llm`)
- [x] Ticket 2.1.5: Structure-Agnostic Semantic Delta Synthesizer (`ScalarRatioRule`, `DeltaSynthesizerConfig`) (`embedag-llm`)
- [x] Ticket 2.1.6: Sovereign `oxillama` GGUF & `candle-core` Inference Providers (`embedag-llm`)
- [x] Ticket 2.1.7: Developer-Defined Entity Properties & Schema Macro (`entity_properties!`, `EntityProperties`) (`embedag-llm`)
- [x] Ticket 2.2.1: Tier 1 Deterministic Alias Store & Inverted Index (`embedag-llm`)
- [x] Ticket 2.2.2: Tier 2 Hybrid Similarity Matching & Pure Rust Embeddings (`embedag-llm`)
- [x] Ticket 2.2.3: In-Memory Changeset Diffing Engine (`embedag-llm`)

### Milestone 3: Relational Persistence with Diesel & Dynamic Graph Updates
- [x] Ticket 3.1.1: Relational Schema Migrations (PostgreSQL & SQLite) (`embedag-diesel`)
- [x] Ticket 3.1.2: Asynchronous Transactions & Savepoints with `diesel-async` (`embedag-diesel`)
- [x] Ticket 3.1.3: High-Throughput Batch Upserts (`embedag-diesel`)
- [x] Ticket 3.1.4: Dynamic Graph Mutation Lifecycle Execution (`embedag-diesel`)
- [x] Ticket 3.1.5: Mandatory Testcontainers Harness for Persistence Layer (`embedag-diesel`)
- [x] Ticket 3.2.1: Typed `WITH RECURSIVE` Traversal Builder via `diesel-cte-ext` (`embedag-diesel`)
- [x] Ticket 3.2.2: SQL Cycle Guardrails & Deduplication via `with_recursive_not_all` (`embedag-diesel`)
- [x] Ticket 3.3.1: Capability Sub-Traits Refactoring (`embedag-core` / `embedag-diesel`)
- [x] Ticket 3.4.1: Structure-Agnostic Upsert Processor & Custom Conflict Strategies (`embedag-diesel`)
- [x] Ticket 3.4.2: Generic Hierarchical CTE Traversal via `diesel-cte-ext` for SQLite & PostgreSQL (`embedag-diesel`)
- [x] Ticket 3.4.3: External Domain Ingestion & Diamond Deduplication Demo (`embedag-example-receipts`)
- [x] Ticket 3.4.4: Dynamic On-the-Fly LLM Taxonomy Pipeline & Deduplication (`embedag-example-receipts`)
- [x] Ticket 3.4.5: Finance Invoice Graph Pipeline with Dual-Mode Density Categorization, Time-Dependency, and Dynamic User Reclassification (Candle & OxiLLaMa) (`embedag-example-receipts`)
- [x] Ticket 3.5.1: Turso / libSQL Distributed Persistence via Diesel & Diesel-Async (`embedag-diesel`)

### Milestone 4: Two-Stage Hybrid Graph-RAG Query Engine
- [x] Ticket 4.1.1: Dense Vector & Sparse FTS Indexing with `oxirag` Echo Store (`embedag-rag`)
- [x] Ticket 4.1.2: Reciprocal Rank Fusion (RRF) Implementation (`embedag-rag`)
- [x] Ticket 4.2.1: Bounded Subgraph Expansion & Distance Decay (`oxirag` Layer 4: Graph) (`embedag-rag`)
- [x] Ticket 4.2.2: Grounded Prompt Evidence Packager with Citations (`embedag-rag`)
- [x] Ticket 4.2.3: Formal SMT-Based Logic Verification via `oxirag` OxiZ Solver (`embedag-rag`)

### Milestone 5: End-to-End Integration, Facade Crate & Verification
- [x] Ticket 5.1.1: Root Facade Crate API Re-exports (`embedag`)
- [x] Ticket 5.1.2: End-to-End Pipeline Integration Test (`embedag`)

### Milestone 6: High-Value Relevant Features
- [x] Ticket 6.1.1: Mermaid & Graphviz DOT Exporters (`embedag-core`)
- [x] Ticket 6.1.2: GraphML & RDF-Star / JSON-LD Serialization via `oxirs` (`embedag-core`)
- [x] Ticket 6.2.1: Graph Migration Engine (`GraphMigration`) (`embedag-core`)
- [x] Ticket 6.3.1: In-Memory Community Detection (`embedag-rag`)
- [x] Ticket 6.4.1: Temporal Edge Versioning (`valid_from`, `valid_to`) (`embedag-diesel`)
- [x] Ticket 6.5.1: Pure-Rust Multimedia Ingestion & Forensic Vision via `oximedia` (`embedag-core`)
- [x] Ticket 6.5.2: SPARQL 1.2 Semantic Query & Reasoning Interface via `oxirs` (`embedag-rag`)

### Milestone 7: Multi-Model & Native Graph Adapters (Future Horizon)
- [x] Ticket 7.1.1: SurrealDB Multi-Model Storage Implementation (`embedag-surrealdb`)
- [x] Ticket 7.2.1: OpenCypher Bolt Protocol Adapter (`embedag-neo4j`)
- [x] Ticket 7.3.1: Native OxiRS TDB2 Persistent Storage Engine (`embedag-oxirs`)

### Milestone 8: Heavy Graph Handling & Topological Pagination
- [x] Ticket 8.1.1: Streaming Traversal Iterators & Early Termination (`embedag-core`)
- [x] Ticket 8.1.2: Degree Pagination & Hub Neighbor Slicing (`embedag-core`)
- [x] Ticket 8.1.3: Localized Differential Cycle Validation (`embedag-core`)
- [x] Ticket 8.2.1: Hub-Bounded GraphRAG Subgraph Expansion (`embedag-rag`)
- [x] Ticket 8.3.1: Windowed & Degree-Bounded Recursive CTE Traversals (`embedag-diesel`)

### Milestone 9: Dependency Resolution, Change Detection & Code-Graph APIs
Slim, dependency-light graph APIs for dependency schedulers, change detectors and code-analysis tools (`doc/22_Dependency_Resolution_and_Merkle_Delta_API.md`).
- [x] Ticket 9.0.1: Specification `22_Dependency_Resolution_and_Merkle_Delta_API` (`doc`)
- [x] Ticket 9.1.1: Slim facade — `embedag::dag` (toposort, levels, reverse_deps, critical_path), `embedag::dag::merkle` (BLAKE3 Merkle delta trees), `embedag-matcher` (AST similarity graphs); features `core,matcher` (`embedag-core`, `embedag-matcher`, `embedag`)
- [ ] Ticket 9.2.1: Multi-root dependency manifests — build a `MemoryDag` from per-directory manifests (`go.mod`, `Cargo.toml`, `package.json`, `pyproject.toml`), cycle hazards and deterministic build order (`embedag-core`)
- [ ] Ticket 9.2.2: Work-item prioritisation — downstream unblock weights with distance decay plus critical-path scheduling over item/milestone DAGs (`embedag-core`)
- [ ] Ticket 9.3.1: Merkle journal verification — recompute and compare Merkle roots of append-only state journals, with a canonical wire format for trees (`embedag-core`)
- [ ] Ticket 9.4.1: Leiden community clustering and god-node (high in-degree) index for code-knowledge graphs (`embedag-rag`)
- [ ] Ticket 9.5.1: Public SCC detection & condensation API for possibly-cyclic graphs (cyclic input → condensed DAG + component membership) (`embedag-core`)

### Milestone 10: Text Embeddings, Vector Search & Classification Primitives
- [ ] Ticket 10.1.1: Embedder trait + real text embeddings per spec 08 (trustformers/oxirag pure-Rust; fastembed/ONNX optional) (`embedag-core`)
- [ ] Ticket 10.1.2: Diesel VectorSearchRepository for pgvector and SQLite (BLOB brute-force / sqlite-vec) (`embedag-diesel`)
- [ ] Ticket 10.2.1: Fuzzy merchant-descriptor normalization (bank descriptor cleaning + trigram/Jaro-Winkler) (`embedag-llm`)
- [ ] Ticket 10.2.2: kNN categorizer over labelled history producing CandidateMatch (`embedag-llm`)
- [ ] Ticket 10.2.3: Recurring/subscription series detection + amount-drift alerts (`embedag-core`)
- [ ] Ticket 10.3.1: Real HTTP LlmProvider (Anthropic / OpenAI / Ollama-compatible) for Tier-3 (`embedag-llm`)
- [ ] Ticket 10.3.2: Lean default features (test-harness and full off by default) (`embedag`)
- [ ] Ticket 10.4.1: HierarchicalModel: discriminator-scoped hierarchies + configurable ON DELETE (`embedag-derive`)
- [ ] Ticket 10.4.2: Brand hierarchy + trait facet with inheritance in the finance categorization engine (`embedag-example-receipts`)
- [ ] Ticket 10.5.1: Incremental Merkle trees with prefix-subtree digests for replica anti-entropy (builds on 9.3.1 wire format) (`embedag-core`)
