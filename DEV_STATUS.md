# Project Status & Ticket Log

**Progress:** 45/48 completed (94%) — **Milestones 1, 2, 3, 4, 5 & 6 COMPLETE**  
**Current Focus:** `Ticket 7.1.1` (SurrealDB Multi-Model Storage Implementation (`embedag-surrealdb`))

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
- [x] Ticket 2.1.5: Structure-Agnostic Semantic Delta Synthesizer (`embedag-llm`)
- [x] Ticket 2.1.6: Sovereign `oxillama` GGUF & `candle-core` Inference Providers (`embedag-llm`)
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
- [ ] Ticket 7.1.1: SurrealDB Multi-Model Storage Implementation (`embedag-surrealdb`) *(IN PROGRESS)*
- [ ] Ticket 7.2.1: OpenCypher Bolt Protocol Adapter (`embedag-neo4j`)
- [ ] Ticket 7.3.1: Native OxiRS TDB2 Persistent Storage Engine (`embedag-oxirs`)
