# Project Status & Ticket Log

**Progress:** 0/39 completed (0%)  
**Current Focus:** `Ticket 1.1.1` (Core Traits & Data Representations)

---

### Milestone 1: Core Domain, In-Memory DAG Arena & Derive Macros
- [ ] Ticket 1.1.1: Core Traits & Data Representations (`embedag-core`) *(IN PROGRESS)*
- [ ] Ticket 1.1.2: In-Memory DAG Arena (`MemoryDag`) via `petgraph` (`embedag-core`)
- [ ] Ticket 1.1.3: Cycle Detection & Acyclic Guardrails (`embedag-core`)
- [ ] Ticket 1.1.4: Typestate Mutation Plan (`embedag-core`)\n- [ ] Ticket 1.2.1: Continuous Scalar Encoders & Log Scaling (`embedag-core`)
- [ ] Ticket 1.2.2: Cyclical Temporal Encoders (`embedag-core`)
- [ ] Ticket 1.2.3: Uber H3 Geospatial Indexing via `h3o` (`embedag-core`)
- [ ] Ticket 1.2.4: Composite Hybrid Distance Metric (`embedag-core`)
- [ ] Ticket 1.3.1: `#[derive(AsGraphContext)]` (`embedag-derive`)
- [ ] Ticket 1.3.2: `#[derive(GraphTarget)]` (`embedag-derive`)

### Milestone 2: Constrained LLM Extraction & Waterfall Resolution
- [ ] Ticket 2.1.1: Universal JSON Schema Generation with `schemars` (`embedag-llm`)
- [ ] Ticket 2.1.2: GBNF Grammar Generation & Pushdown Automata (`embedag-llm`)
- [ ] Ticket 2.1.3: Draft-Then-Decode Envelope & Prompt Formatting (`embedag-llm`)
- [ ] Ticket 2.1.4: Priority 1 `oxibonsai` Provider & Fallback Backends (`embedag-llm`)
- [ ] Ticket 2.1.5: Semantic Delta Prompt Synthesizer for Mixed-Modality (`embedag-llm`)
- [ ] Ticket 2.2.1: Tier 1 Deterministic Alias Store & Inverted Index (`embedag-llm`)
- [ ] Ticket 2.2.2: Tier 2 Hybrid Similarity Matching & Pure Rust Embeddings (`embedag-llm`)
- [ ] Ticket 2.2.3: In-Memory Changeset Diffing Engine (`embedag-llm`)

### Milestone 3: Relational Persistence with Diesel & Dynamic Graph Updates
- [ ] Ticket 3.1.1: Relational Schema Migrations (PostgreSQL & SQLite) (`embedag-diesel`)
- [ ] Ticket 3.1.2: Asynchronous Transactions & Savepoints with `diesel-async` (`embedag-diesel`)
- [ ] Ticket 3.1.3: High-Throughput Batch Upserts (`embedag-diesel`)
- [ ] Ticket 3.1.4: Dynamic Graph Mutation Lifecycle Execution (`embedag-diesel`)
- [ ] Ticket 3.1.5: Mandatory Testcontainers Harness for Persistence Layer (`embedag-diesel`)
- [ ] Ticket 3.2.1: Typed `WITH RECURSIVE` Traversal Builder (`diesel-cte-ext`)
- [ ] Ticket 3.2.2: SQL Cycle Guardrails (`diesel-cte-ext`)
- [ ] Ticket 3.3.1: Capability Sub-Traits Refactoring (`embedag-core` / `embedag-diesel`)

### Milestone 4: Two-Stage Hybrid Graph-RAG Query Engine
- [ ] Ticket 4.1.1: Dense Vector & Sparse FTS Indexing (`embedag-rag`)
- [ ] Ticket 4.1.2: Reciprocal Rank Fusion (RRF) Implementation (`embedag-rag`)
- [ ] Ticket 4.2.1: Bounded Subgraph Expansion & Distance Decay (`embedag-rag`)
- [ ] Ticket 4.2.2: Grounded Prompt Evidence Packager with Citations (`embedag-rag`)

### Milestone 5: End-to-End Integration, Facade Crate & Verification
- [ ] Ticket 5.1.1: Root Facade Crate API Re-exports (`embedag`)
- [ ] Ticket 5.1.2: End-to-End Pipeline Integration Test (`embedag`)

### Milestone 6: High-Value Relevant Features
- [ ] Ticket 6.1.1: Mermaid & Graphviz DOT Exporters (`embedag-core`)
- [ ] Ticket 6.1.2: GraphML & JSON-LD Serialization (`embedag-core`)
- [ ] Ticket 6.2.1: Graph Migration Engine (`GraphMigration`) (`embedag-core`)
- [ ] Ticket 6.3.1: In-Memory Community Detection (`embedag-rag`)
- [ ] Ticket 6.4.1: Temporal Edge Versioning (`valid_from`, `valid_to`) (`embedag-diesel`)

### Milestone 7: Multi-Model & Native Graph Adapters (Future Horizon)
- [ ] Ticket 7.1.1: SurrealDB Multi-Model Storage Implementation (`embedag-surrealdb`)
- [ ] Ticket 7.2.1: OpenCypher Bolt Protocol Adapter (`embedag-neo4j`)
