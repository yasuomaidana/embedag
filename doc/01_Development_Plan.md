# Development Plan: Milestones, Epics, Stories & Tickets

## 1. Overview & Scrum Governance
This document establishes the comprehensive engineering roadmap for `embedag`. Following the governance model defined in [AGENTS.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/AGENTS.md) and governed by the `scrum-master-alignment` skill, every ticket defines:
- **Technical Description**
- **Explicit Acceptance Criteria**
- **Automated Verification & Test Specifications** (Unit, Integration, or Property tests)
- **Specification Traceability** linking directly to the technical documentation in `doc/` (sequenced consecutively from `00` to `12`).

---

## Milestone 1: Core Domain, In-Memory DAG Arena & Derive Macros
**Goal**: Establish the zero-cost in-memory DAG representation, cycle detection, deterministic feature encoders, and procedural derive macros without external database or network dependencies.
**Target Crates**: `embedag-core`, `embedag-derive`.

### Epic 1.1: Core Domain Contracts & Memory Management
*Traceability: [04_Core_Domain_and_DAG_Abstraction.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/04_Core_Domain_and_DAG_Abstraction.md), [12_Dynamic_Graph_Updates_and_Mixed_Modality_Resolution.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/12_Dynamic_Graph_Updates_and_Mixed_Modality_Resolution.md)*

#### Ticket 1.1.1: Core Traits & Data Representations
- **Description**: Define `AsGraphContext`, `GraphTarget`, `NodePayload`, `EdgePayload`, `ProvenanceKind`, `FeatureMap`, and `MultimodalBlob` in `embedag-core`.
- **Acceptance Criteria**:
  - Trait definitions are object-safe where required and support `Send + Sync + 'static`.
  - Default trait methods allow incremental implementation without boilerplate.
  - Custom error types are declared using `thiserror`.
- **Tests**:
  - `test_default_as_graph_context_impl`: Verify default methods return empty feature maps and blobs.
  - `test_edge_payload_provenance_equality`: Verify `EdgePayload` equality and serialization invariants.

#### Ticket 1.1.2: In-Memory DAG Arena (`MemoryDag`) via `petgraph`
- **Description**: Implement `MemoryDag<T: NodePayload>` wrapping `petgraph::graph::DiGraph<T, EdgePayload>`. Maintain an internal O(1) hash index mapping `T::Key` to `NodeIndex`.
- **Acceptance Criteria**:
  - Inserting a node returns a stable `NodeIndex`.
  - Adding a directed edge connects two nodes with typed relationship and confidence.
  - Duplicate insertions by business key update the existing node payload without allocating duplicate nodes.
- **Tests**:
  - `test_insert_and_lookup_by_key`: Assert node insertion and retrieval by business key.
  - `test_multi_parent_edge_insertion`: Assert a single child node successfully connects to multiple parents.

#### Ticket 1.1.3: Cycle Detection & Acyclic Guardrails
- **Description**: Implement `MemoryDag::would_create_cycle(from, to) -> bool` and `MemoryDag::validate_acyclic() -> Result<(), DagError>` using `petgraph::algo::is_cyclic_directed` and Tarjan's SCC.
- **Acceptance Criteria**:
  - Any proposed edge that forms a closed loop ($A \to B \to A$ or $A \to B \to C \to A$) is detected and rejected with `DagError::CycleDetected`.
  - Multi-parent DAG topologies without cycles pass validation.
- **Tests**:
  - `test_diamond_dag_passes`: Verify a diamond DAG ($A \to B, A \to C, B \to D, C \to D$) passes without error.
  - `test_cycle_rejection`: Construct a cycle ($A \to B \to C \to A$) and verify `would_create_cycle` returns `true`.

#### Ticket 1.1.4: Typestate Mutation Plan
- **Description**: Implement `MutationPlan<T, State>` with zero-sized states `Unverified`, `CycleValidated`, and `Committed`.
- **Acceptance Criteria**:
  - Calling `.commit()` on `MutationPlan<T, Unverified>` is a compile-time error.
  - `.validate_cycles()` transitions `MutationPlan<T, Unverified>` into `MutationPlan<T, CycleValidated>`.
- **Tests**:
  - `test_typestate_transition_flow`: Verify state transitions compile and validate accurately.

---

### Epic 1.2: Deterministic Feature Encoders & Mixed-Modality Distance
*Traceability: [05_Deterministic_Feature_Encoding_and_Multimodal.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/05_Deterministic_Feature_Encoding_and_Multimodal.md), [12_Dynamic_Graph_Updates_and_Mixed_Modality_Resolution.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/12_Dynamic_Graph_Updates_and_Mixed_Modality_Resolution.md)*

#### Ticket 1.2.1: Continuous Scalar Encoders & Log Scaling
- **Description**: Implement continuous normalization functions: `min_max_scale`, `log_scale`, and `relative_difference` for floating-point metrics.
- **Acceptance Criteria**:
  - Outputs are strictly bounded within $[0.0, 1.0]$.
  - Handles edge cases: $\epsilon$ guardrails prevent division by zero when both inputs are $0.0$.
- **Tests**:
  - `test_min_max_scale_bounds`: Property tests verifying results remain in $[0.0, 1.0]$ across $10,000$ random floats.
  - `test_relative_difference_zeros`: Verify distance between $0.0$ and $0.0$ is $0.0$ without NaN.

#### Ticket 1.2.2: Cyclical Temporal Encoders
- **Description**: Implement `CyclicTimeEncoder` transforming `chrono::DateTime<Utc>` into 2D circular coordinates for hour-of-day, day-of-week, and day-of-year.
- **Acceptance Criteria**:
  - Euclidean distance between $23:59$ and $00:01$ is continuous and adjacent ($\approx 0$).
  - Outputs satisfy $\sin^2(\theta) + \cos^2(\theta) \approx 1.0$.
- **Tests**:
  - `test_hour_boundary_continuity`: Assert distance between $23:59$ and $00:01$ is significantly smaller than distance between $12:00$ and $00:00$.

#### Ticket 1.2.3: Uber H3 Geospatial Indexing via `h3o`
- **Description**: Implement `H3SpatialEncoder` converting latitude/longitude coordinates into discrete 64-bit cell indices and computing integer topological grid distance.
- **Acceptance Criteria**:
  - Converts valid coordinates to 64-bit cell indices at selectable resolutions (0–15).
  - Returns grid ring distance using pure integer arithmetic.
- **Tests**:
  - `test_h3_cell_generation_and_grid_distance`: Compute distance between two known points in Mexico City and assert expected grid ring count.

#### Ticket 1.2.4: Composite Hybrid Distance Metric (Gower + Angular + Spatial + Categorical)
- **Description**: Implement `compute_composite_distance` fusing text cosine (via `trustformers`), scalar Gower distance, H3 spatial distance, cyclical time distance, and categorical tag matches.
- **Acceptance Criteria**:
  - Composite score is normalized in $[0.0, 1.0]$.
  - If identical records are compared, distance is exactly $0.0$.
- **Tests**:
  - `test_composite_distance_identity`: Comparing an entity with itself yields $0.0$.
  - `test_mixed_modality_composite_similarity`: Comparing 50g vs 500g product with identical text correctly yields high distance ($> 0.70$).

---

### Epic 1.3: Procedural Derive Macros (`embedag-derive`)
*Traceability: [04_Core_Domain_and_DAG_Abstraction.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/04_Core_Domain_and_DAG_Abstraction.md)*

#### Ticket 1.3.1: `#[derive(AsGraphContext)]`
- **Description**: Create procedural macro parsing `#[graph(text)]`, `#[graph(scalar)]`, `#[graph(time)]`, and `#[graph(image)]`. Generate the `AsGraphContext` trait implementation.
- **Acceptance Criteria**:
  - String fields marked `#[graph(text)]` are concatenated into `text_context()`.
  - Numeric fields marked `#[graph(scalar, normalize = "min..max")]` are projected into `scalar_features()`.
  - Compiles clean AST code with informative compile-error diagnostics on invalid attributes.
- **Tests**:
  - `trybuild` compile-pass and compile-fail test suites verifying macro behavior.

#### Ticket 1.3.2: `#[derive(GraphTarget)]`
- **Description**: Create procedural macro parsing `#[node(key)]` and `#[node(label)]` on structs, generating `NodePayload` implementation.
- **Acceptance Criteria**:
  - Validates that exactly one field is tagged with `#[node(key)]`.
  - Implements `NodePayload::node_key()` and `NodePayload::display_label()`.
- **Tests**:
  - `trybuild` test suite for valid and missing key attributes.

---

## Milestone 2: Constrained LLM Extraction & Waterfall Resolution
**Goal**: Integrate `schemars`, local GBNF grammar generation, the **Priority 1 pure-Rust `oxibonsai` inference engine**, and the 4-tier waterfall resolution pipeline.
**Target Crates**: `embedag-llm`, `embedag-core`.

### Epic 2.1: Schema Generation & Constrained Decoding
*Traceability: [06_LLM_Extraction_and_Constrained_Decoding.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/06_LLM_Extraction_and_Constrained_Decoding.md), [10_External_Repositories_and_KitaSan_Stack.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/10_External_Repositories_and_KitaSan_Stack.md)*

#### Ticket 2.1.1: Universal JSON Schema Generation with `schemars`
- **Description**: Derive `schemars::JsonSchema` on `ExtractionDirective` and `ExtractionEnvelope<T>`. Implement schema extraction utilities in `embedag-llm`.
- **Acceptance Criteria**:
  - Emits valid JSON Schema draft-07 / 2020-12 specifications.
  - Nested types, enums, and optional fields match Rust serialization expectations.
- **Tests**:
  - `test_json_schema_validation`: Validate generated schema against the official JSON Schema metaschema.

#### Ticket 2.1.2: GBNF Grammar Generation & Pushdown Automata
- **Description**: Implement a JSON Schema to GBNF grammar compiler compatible with `oxibonsai`'s pure Rust sampler.
- **Acceptance Criteria**:
  - Generates valid GBNF syntax conforming to Pushdown Automaton specifications.
  - Guarantees that local sampler cannot emit invalid JSON tokens.
- **Tests**:
  - `test_gbnf_grammar_syntax`: Verify generated GBNF compiles and parses without errors.

#### Ticket 2.1.3: Draft-Then-Decode Envelope & Prompt Formatting
- **Description**: Implement `ExtractionEnvelope<T>` containing `chain_of_thought` scratchpad ahead of payload fields.
- **Acceptance Criteria**:
  - Serializes `chain_of_thought` as the first JSON key.
  - Deserializes model outputs into typed `ExtractionDirective` while retaining reasoning notes in provenance metadata.
- **Tests**:
  - `test_draft_then_decode_ordering`: Assert `chain_of_thought` key appears first in serialized schema.

#### Ticket 2.1.4: Priority 1 `oxibonsai` Provider & Fallback Backends
- **Description**: Implement `OxibonsaiProvider` (Priority 1 pure-Rust local engine using `oxibonsai-runtime` and `oxibonsai-tokenizer`). Implement secondary fallback providers (`CandleProvider`, `CloudProvider`).
- **Acceptance Criteria**:
  - `OxibonsaiProvider` executes model inference locally with zero C/C++ FFI dependencies.
  - Applies GBNF token logit masking during autoregressive token generation.
- **Tests**:
  - `test_oxibonsai_sampler_structured_output`: Run pure Rust in-memory token generation and assert output strictly deserializes into `ExtractionEnvelope<ExtractionDirective>`.

#### Ticket 2.1.5: Semantic Delta Prompt Synthesizer for Mixed-Modality
- **Description**: Implement utility formatting candidate nodes and incoming items into pre-computed numerical delta comparison cards (weight matching, scale factor, H3 proximity) for `oxibonsai` prompts.
- **Acceptance Criteria**:
  - Prevents raw float dumping; emits structured mathematical comparison summaries.
- **Tests**:
  - `test_semantic_delta_prompt_synthesis`: Assert comparison card accurately calculates 12x multiplier between 50g single and 600g pack.

---

### Epic 2.2: Multi-Tiered Waterfall Resolution Pipeline
*Traceability: [07_Graph_Resolution_Diffing_and_Cycle_Guards.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/07_Graph_Resolution_Diffing_and_Cycle_Guards.md), [12_Dynamic_Graph_Updates_and_Mixed_Modality_Resolution.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/12_Dynamic_Graph_Updates_and_Mixed_Modality_Resolution.md)*

#### Ticket 2.2.1: Tier 1 Deterministic Alias Store & Inverted Index
- **Description**: Implement in-memory and repository-backed alias store mapping normalized tokens to canonical node IDs.
- **Acceptance Criteria**:
  - Exact or normalized matches resolve in $< 1\text{ms}$ with zero model tokens.
  - Human corrections write back to the store immediately.
- **Tests**:
  - `test_alias_resolution_fast_path`: Assert alias lookups bypass Tier 2 and Tier 3.

#### Ticket 2.2.2: Tier 2 Hybrid Similarity Matching & Pure Rust Embeddings
- **Description**: Integrate `trustformers` / `oxirag` to generate dense embeddings in pure Rust. Compute composite similarity against candidate nodes.
- **Acceptance Criteria**:
  - Similarity scores $\ge 0.85$ attach the node directly.
  - Scores $< 0.85$ route to Tier 3.
- **Tests**:
  - `test_hybrid_threshold_routing`: Assert routing decisions match threshold criteria.

#### Ticket 2.2.3: In-Memory Changeset Diffing Engine
- **Description**: Implement diffing engine comparing LLM proposals against active `MemoryDag`.
- **Acceptance Criteria**:
  - Emits partitioned changesets (`new_nodes`, `updated_nodes`, `new_edges`).
  - Idempotent: re-running with existing nodes emits zero changes.
- **Tests**:
  - `test_diffing_idempotency`: Assert identical inputs produce empty changesets.

---

## Milestone 3: Relational Persistence with Diesel & Dynamic Graph Updates
**Goal**: Implement `GraphRepository` using Diesel, `diesel-async`, and `diesel-cte-ext` for PostgreSQL and SQLite, while supporting live dynamic graph updates.
**Target Crate**: `embedag-diesel`, `embedag-core`.

### Epic 3.1: Schema Migrations & Transaction Architecture
*Traceability: [08_Relational_Persistence_Diesel_Postgres_SQLite.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/08_Relational_Persistence_Diesel_Postgres_SQLite.md), [12_Dynamic_Graph_Updates_and_Mixed_Modality_Resolution.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/12_Dynamic_Graph_Updates_and_Mixed_Modality_Resolution.md)*

#### Ticket 3.1.1: Relational Schema Migrations (PostgreSQL & SQLite)
- **Description**: Author migrations for `semantic_nodes`, `semantic_edges`, and `semantic_aliases` with appropriate foreign keys and indexes.
- **Acceptance Criteria**:
  - PostgreSQL migration includes `pgvector` extension and `vector(1024)` column.
  - SQLite migration includes valid foreign key pragmas and BLOB columns.
- **Tests**:
  - Run migrations up and down on real Postgres and SQLite test databases.

#### Ticket 3.1.2: Asynchronous Transactions & Savepoints with `diesel-async`
- **Description**: Implement transactional batch writes using `AsyncPgConnection::transaction` with automatic savepoint rollback on error.
- **Acceptance Criteria**:
  - Partial batch failure rolls back to savepoint without poisoning connection pool.
  - Successful operations commit atomically.
- **Tests**:
  - `test_transaction_rollback_on_constraint_violation`: Simulate foreign key violation and assert no orphaned records persist.

#### Ticket 3.1.3: High-Throughput Batch Upserts
- **Description**: Implement batch upsert methods using Diesel's `.on_conflict()` APIs.
- **Acceptance Criteria**:
  - Nodes table executes `INSERT ... ON CONFLICT (id) DO UPDATE`.
  - Edges table executes `INSERT ... ON CONFLICT DO NOTHING`.
- **Tests**:
  - `test_batch_upsert_performance`: Verify inserting $1,000$ nodes executes in a single round-trip.

#### Ticket 3.1.4: Dynamic Graph Mutation Lifecycle Execution
- **Description**: Implement end-to-end execution of `MutationDirective::UpdateNodePayload`, `InsertNode`, `AttachParent`, and `RetargetEdge` within an active transaction.
- **Acceptance Criteria**:
  - Mutating an existing node updates dynamic JSONB attributes while preserving existing edge relationships.
  - Live in-memory LRU cache and Tier 1 alias maps are refreshed after commit.
- **Tests**:
  - `test_dynamic_graph_mutation_lifecycle`: Seed existing graph, submit variant item with weight update, and assert database and cache reflect updated node and new child edge.

---

### Epic 3.2: Recursive CTE Traversals via `diesel-cte-ext`
*Traceability: [08_Relational_Persistence_Diesel_Postgres_SQLite.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/08_Relational_Persistence_Diesel_Postgres_SQLite.md), [10_External_Repositories_and_KitaSan_Stack.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/10_External_Repositories_and_KitaSan_Stack.md)*

#### Ticket 3.2.1: Typed `WITH RECURSIVE` Traversal Builder
- **Description**: Integrate `diesel-cte-ext` to construct type-checked recursive queries traversing DAG ancestors and descendants up to `max_depth`.
- **Acceptance Criteria**:
  - Compiles and executes on both PostgreSQL (`diesel-async`) and SQLite.
  - Eliminates raw string SQL concatenation.
- **Tests**:
  - `test_cte_subgraph_traversal`: Retrieve 3-hop hierarchy and assert all descendants are returned.

#### Ticket 3.2.2: SQL Cycle Guardrails
- **Description**: Implement path tracking inside recursive CTE queries (`ARRAY[id]` in Postgres, path string in SQLite) to prevent infinite loops.
- **Acceptance Criteria**:
  - If a cycle exists in database records, CTE terminates safely when it encounters an already-visited ID.
- **Tests**:
  - `test_cte_cycle_guard_termination`: Seed cyclic records in test DB and assert query terminates within expected depth without timeout.

---

### Epic 3.3: Storage Extensibility & Capability Sub-Traits
*Traceability: [11_Multi_Database_Extensibility_and_Future_Adapters.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/11_Multi_Database_Extensibility_and_Future_Adapters.md)*

#### Ticket 3.3.1: Capability Sub-Traits Refactoring
- **Description**: Refactor persistence into composed traits: `GraphStorage` (topology), `VectorIndex` (similarity), and `AliasStorage` (Tier 1 lookup), with blanket implementation for `GraphRepository`.
- **Acceptance Criteria**:
  - Clean separation allows backends that lack native vector indexes or graph traversal to be composed modularly.
- **Tests**:
  - `test_composite_repository_composition`: Verify in-memory storage combines with mock vector index.

---

## Milestone 4: Two-Stage Hybrid Graph-RAG Query Engine
**Goal**: Implement hybrid dense/sparse search, Reciprocal Rank Fusion (RRF), topological distance decay, and grounded evidence packaging.
**Target Crates**: `embedag-rag`, `embedag`.

### Epic 4.1: Hybrid Candidate Retrieval & Fusion
*Traceability: [09_Hybrid_Graph_RAG_and_Query_Engine.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/09_Hybrid_Graph_RAG_and_Query_Engine.md)*

#### Ticket 4.1.1: Dense Vector & Sparse FTS Indexing
- **Description**: Implement candidate search using `pgvector` / SQLite BLOB dot product for dense search, and `tsvector` / SQLite `FTS5` for sparse keyword search.
- **Acceptance Criteria**:
  - Dense query returns top-K nearest semantic neighbors.
  - Sparse query finds misspelled proper nouns (e.g. `"Carols V"` matches `"Carlos V"`).
- **Tests**:
  - `test_sparse_fts_typo_resilience`: Verify misspelled query returns target entity in sparse results.

#### Ticket 4.1.2: Reciprocal Rank Fusion (RRF) Implementation
- **Description**: Implement RRF algorithm ($k = 60$) merging dense and sparse ranked lists.
- **Acceptance Criteria**:
  - Mathematically sound score calculation: $Score = \frac{1}{60 + R_{\text{dense}}} + \frac{1}{60 + R_{\text{sparse}}}$.
  - Candidates ranking high in either list surface at the top.
- **Tests**:
  - `test_rrf_scoring_order`: Assert fused order matches expected mathematical ranking.

---

### Epic 4.2: Topological Subgraph Expansion & Evidence Packaging
*Traceability: [09_Hybrid_Graph_RAG_and_Query_Engine.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/09_Hybrid_Graph_RAG_and_Query_Engine.md)*

#### Ticket 4.2.1: Bounded Subgraph Expansion & Distance Decay
- **Description**: Take top RRF candidate seeds, traverse k-hop ancestors/descendants, and apply topological distance decay: $Score(N) = Score(\text{Seed}) \times 0.85^{\text{hops}}$.
- **Acceptance Criteria**:
  - Returns complete connected subgraph within depth bounds.
  - Applies exponential decay factor to distant nodes.
- **Tests**:
  - `test_subgraph_expansion_with_decay`: Assert 2-hop descendants have lower relevance score than immediate 1-hop children.

#### Ticket 4.2.2: Grounded Prompt Evidence Packager with Citations
- **Description**: Format extracted subgraphs into compact Markdown evidence blocks with node UUID citations and explicit confidence tags for generative LLMs.
- **Acceptance Criteria**:
  - Outputs structured Markdown preserving hierarchy and citation anchors (`[ID: ...]`).
  - Labels algorithmic edges as `EXTRACTED (1.0)` and model edges as `INFERRED (< 1.0)`.
- **Tests**:
  - `test_evidence_packaging_format`: Assert formatted prompt matches expected schema with citations.

---

## Milestone 5: End-to-End Integration, Facade Crate & Verification
**Goal**: Unify all workspace crates under the `embedag` facade, run comprehensive benchmarks, and verify code quality.
**Target Crates**: `embedag`, all crates.

### Epic 5.1: Unified Facade & Developer Experience

#### Ticket 5.1.1: Root Facade Crate API Re-exports
- **Description**: Implement top-level `embedag` crate re-exporting core traits, derive macros, providers, and persistence adapters via Cargo feature flags.
- **Acceptance Criteria**:
  - End users can depend on `embedag = { version = "0.1", features = ["full"] }` or granular features.
  - Comprehensive documentation with runnable doc-tests.
- **Tests**:
  - `cargo test --doc` across all crates.

#### Ticket 5.1.2: End-to-End Pipeline Integration Test
- **Description**: Implement full workflow test: Input struct $\to$ `#[derive(AsGraphContext)]` $\to$ extraction $\to$ `MemoryDag` $\to$ cycle check $\to$ Diesel Postgres/SQLite persistence $\to$ hybrid Graph-RAG query.
- **Acceptance Criteria**:
  - Complete round-trip executes without errors.
  - Persisted graph matches expected topological relationships.
- **Tests**:
  - `tests/e2e_pipeline_test.rs`: Full automated test suite against local Postgres container and in-memory SQLite.

---

## Milestone 6: High-Value Relevant Features (Visual Exporters, Migrations, Communities, Bitemporal)
*Traceability: [11_Multi_Database_Extensibility_and_Future_Adapters.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/11_Multi_Database_Extensibility_and_Future_Adapters.md), [12_Dynamic_Graph_Updates_and_Mixed_Modality_Resolution.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/12_Dynamic_Graph_Updates_and_Mixed_Modality_Resolution.md)*
**Goal**: Implement schema evolution, visual exporters, topological community detection, and bitemporal history.
**Target Crates**: `embedag-core`, `embedag-rag`, `embedag-diesel`.

### Epic 6.1: Visual Graph Exporters
#### Ticket 6.1.1: Mermaid & Graphviz DOT Exporters
- **Description**: Implement `.to_mermaid()` and `.to_dot()` on `MemoryDag` in `embedag-core`.
- **Acceptance Criteria**:
  - Outputs valid Mermaid flowcharts and Graphviz DOT graph strings with node labels and confidence-weighted edge styles.
- **Tests**:
  - `test_mermaid_export_syntax`: Assert exported Mermaid string conforms to valid graph syntax.

#### Ticket 6.1.2: GraphML & JSON-LD Serialization
- **Description**: Implement serializers exporting DAGs into GraphML and W3C JSON-LD formats for interoperability with external graph visualizers (Gephi, Cytoscape).
- **Acceptance Criteria**:
  - Generates conformant XML GraphML and JSON-LD schemas.
- **Tests**:
  - `test_graphml_serialization`: Verify export generates valid XML tree structure.

### Epic 6.2: Schema Evolution & Graph Migrations
#### Ticket 6.2.1: Graph Migration Engine (`GraphMigration`)
- **Description**: Implement a migration runner in `embedag-core` that allows declarative reparenting, category splitting, and node merging across persisted DAGs.
- **Acceptance Criteria**:
  - Validates that graph transformations do not create cycles or leave orphaned children.
  - Generates transactional SQL deltas for Diesel.
- **Tests**:
  - `test_category_split_migration`: Split category `"Confectionery"` into `"Chocolate"` and `"Candy"`, verifying child edges are re-routed accurately.

### Epic 6.3: Topological Community Detection (Leiden & Louvain)
#### Ticket 6.3.1: In-Memory Community Detection in `embedag-rag`
- **Description**: Implement Leiden or Louvain graph clustering over `MemoryDag` to detect high-level topological communities.
- **Acceptance Criteria**:
  - Groups closely connected DAG subgraphs into discrete community IDs.
  - Query planner routes broad macro queries to community hubs before node expansion.
- **Tests**:
  - `test_community_clustering_accuracy`: Assert disconnected and modular subgraphs partition into separate communities.

### Epic 6.4: Bitemporal DAG Versioning
#### Ticket 6.4.1: Temporal Edge Versioning (`valid_from`, `valid_to`)
- **Description**: Extend `semantic_edges` relational schema to include `valid_from` and `valid_to` timestamps. Implement historical point-in-time CTE traversals (`AS OF TIMESTAMP`).
- **Acceptance Criteria**:
  - Mutations do not overwrite historic edges; obsolete edges receive `valid_to = NOW()`.
  - Point-in-time queries traverse only edges active at the specified historical timestamp.
- **Tests**:
  - `test_point_in_time_dag_traversal`: Verify querying graph at $T_0$ returns original topology, and querying at $T_1$ returns updated topology.

---

## Milestone 7: Multi-Model & Native Graph Adapters (Future Horizon)
*Traceability: [11_Multi_Database_Extensibility_and_Future_Adapters.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/11_Multi_Database_Extensibility_and_Future_Adapters.md)*
**Goal**: Implement native non-SQL backend adapters for SurrealDB and Neo4j.
**Target Crates**: `embedag-surrealdb`, `embedag-neo4j`.

### Epic 7.1: SurrealDB Embedded Adapter (`embedag-surrealdb`)
#### Ticket 7.1.1: SurrealDB Multi-Model Storage Implementation
- **Description**: Implement `GraphStorage` and `VectorIndex` for SurrealDB using native record links (`->belongs_to->`) and SurrealQL vector functions.
- **Acceptance Criteria**:
  - Runs embedded in-process (`kv-mem`) with zero C-FFI.
  - Replaces CTEs with native SurrealQL graph traversals.
- **Tests**:
  - `test_surrealdb_embedded_roundtrip`: In-memory integration test verifying node creation, edge wiring, and vector search.

### Epic 7.2: Neo4j OpenCypher Bolt Adapter (`embedag-neo4j`)
#### Ticket 7.2.1: OpenCypher Bolt Protocol Adapter
- **Description**: Implement `GraphStorage` for Neo4j and Memgraph via the pure-Rust `neo4rs` driver.
- **Acceptance Criteria**:
  - Translates `persist_graph_delta` into atomic OpenCypher `MERGE` queries.
  - Translates `fetch_subgraph` into variable-length Cypher pattern matching (`[:HAS_PARENT*1..N]`).
- **Tests**:
  - `test_neo4j_cypher_subgraph_traversal`: Test Cypher query formatting against mock Bolt server.

---

## 2. Quality Assurance & Verification Gates
Aligned with project agent governance:
- **Clippy**: `cargo clippy --all-targets --all-features -- -D warnings`
- **Formatting**: `cargo fmt --check`
- **Coverage**: Minimum 85% line and branch coverage measured via `cargo llvm-cov`
- **Complexity**: Cyclomatic Complexity $\le 15$, Cognitive Complexity $\le 20$
- **Security**: Zero advisories via `cargo audit`.
