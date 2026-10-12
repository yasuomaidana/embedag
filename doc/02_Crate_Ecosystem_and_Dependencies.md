# Workspace Crate Architecture & External Dependency Ecosystem

## 1. Internal Workspace Crate Topology

To maintain strict modularity, high build concurrency, and clean dependency inversion, `embedag` is organized as a Cargo workspace partitioned into six specialized crates:

```
embedag/ (Workspace Root)
├── Cargo.toml
├── crates/
│   ├── embedag-core/       # Domain traits, petgraph DAG arena, cycle detection, feature metrics
│   ├── embedag-derive/     # Procedural macros (#[derive(AsGraphContext)], #[derive(GraphTarget)])
│   ├── embedag-llm/        # oxibonsai (Priority 1 pure Rust), schemars, GBNF grammar, providers
│   ├── embedag-diesel/     # PostgreSQL & SQLite persistence, diesel-async, diesel-cte-ext
│   ├── embedag-rag/        # Hybrid query engine, two-stage RRF retrieval, oxirag/oxirs synergy
│   └── embedag/            # Top-level unified facade crate
```

---

## 2. Internal Crate Scopes & Responsibilities

### 2.1 `embedag-core`
- **Scope**: Pure domain logic and graph abstraction with zero database drivers or network clients.
- **Responsibilities**:
  - Trait definitions: `AsGraphContext`, `GraphTarget`, `NodePayload`, `GraphRepository`, `GraphClassifier`.
  - In-memory Directed Acyclic Graph arena backed by `petgraph::graph::DiGraph`.
  - Cycle detection and topological validation (`petgraph::algo::is_cyclic_directed`).
  - Feature representation: `FeatureMap`, `MultimodalBlob`, continuous normalization, and cyclical time math.
  - Hybrid mathematical distance metrics (composite Gower + Angular + Spatial distance).
  - Domain error types (`DagError`, `RepositoryError`).

### 2.2 `embedag-derive`
- **Scope**: Compile-time procedural macros.
- **Responsibilities**:
  - `#[derive(AsGraphContext)]`: Parses `#[graph(text)]`, `#[graph(scalar)]`, `#[graph(time)]`, `#[graph(image)]`. Emits implementations for text concatenation, cyclical time conversion, and scalar feature mapping.
  - `#[derive(GraphTarget)]`: Parses `#[node(key)]`, `#[node(label)]`. Implements `NodePayload`.

### 2.3 `embedag-llm`
- **Scope**: Model-agnostic extraction and constrained decoding (with **KitaSan Stack as Priority 1**).
- **Responsibilities**:
  - **Priority 1 Engine (`oxibonsai`)**: Zero-FFI, zero-C/C++ local inference engine running sub-2-bit and ternary quantized models with native SIMD and Apple Metal.
  - **Priority 1 Tokenizer (`oxibonsai-tokenizer`)**: Standalone pure-Rust BPE tokenizer.
  - `schemars` JSON Schema generation from Rust structs.
  - Context-free GBNF grammar compilation for local logit masking pushdown automata.
  - Draft-then-decode envelope generation (`chain_of_thought` + structured payload).
  - Optional provider implementations (`CandleProvider`, `OpenAIProvider`, `AnthropicProvider`).
  - Implements `GraphClassifier` trait for `embedag-core`.

### 2.4 `embedag-diesel`
- **Scope**: Relational persistence and recursive CTE traversal.
- **Responsibilities**:
  - PostgreSQL adapter: `diesel-async`, `AsyncPgConnection`, `bb8` connection pool, `pgvector`.
  - SQLite adapter: `diesel` (synchronous/spawn_blocking), `zerocopy` BLOB vectors, FTS5.
  - Integration of `diesel-cte-ext` for typed recursive CTE traversals (`WITH RECURSIVE`).
  - Transaction management with savepoints and batch upserts (`ON CONFLICT`).
  - Implements `GraphRepository` trait for `embedag-core`.

### 2.5 `embedag-rag`
- **Scope**: Hybrid search, query planning, and context synthesis.
- **Responsibilities**:
  - Multi-modal candidate retrieval (Dense KNN + Sparse FTS).
  - **Priority 1 Pure Rust Embeddings (`trustformers` / `oxirag`)**.
  - Reciprocal Rank Fusion (RRF) algorithm ($k=60$).
  - Subgraph expansion (retrieving ancestors, descendants, and sibling variants via repository).
  - Grounded prompt context formatting for generative models.

### 2.6 `embedag` (Facade Crate)
- **Scope**: Top-level crate providing ergonomic developer experience.
- **Responsibilities**:
  - Re-exports core traits, macros, and adapters through clean Cargo feature flags:
    - `derive`: Enables `embedag-derive`.
    - `kitasan` (default): Enables `oxibonsai`, `oxibonsai-tokenizer`, and `trustformers`.
    - `postgres`: Enables PostgreSQL Diesel async persistence.
    - `sqlite`: Enables SQLite Diesel persistence.
    - `rag`: Enables `embedag-rag`.
    - `candle` / `fastembed` / `cloud`: Enables alternative inference and embedding backends.
    - `full`: Enables all features.

---

## 3. External Dependencies Evaluation Matrix

| Crate Name | Version | Used By | Stack Tier | Purpose & Rationale | Feature Flags |
|---|---|---|---|---|---|
| **`oxibonsai`** | `0.2` | `embedag-llm` | **Priority 1 (KitaSan)** | Pure Rust, zero-FFI, zero-C/C++ inference engine for quantized/ternary LLMs. Primary local inference engine. | `simd-avx2`, `simd-neon` |
| **`oxibonsai-tokenizer`** | `0.2` | `embedag-llm` | **Priority 1 (KitaSan)** | Standalone pure-Rust BPE tokenizer loading `tokenizer.json` directly without C-FFI. | `default` |
| **`trustformers`** | `0.1` | `embedag-core`, `embedag-rag` | **Priority 1 (KitaSan)** | Pure Rust transformers and token embeddings. Eliminates ONNX Runtime C library FFI dependencies. | `default` |
| **`diesel-cte-ext`** | `0.3` | `embedag-diesel` | **Priority 1 (Direct Dep)** | Typed `WITH RECURSIVE` Common Table Expressions extension for Diesel DSL. Eliminates raw SQL strings. | `async` |
| **`petgraph`** | `0.6` | `embedag-core` | **Priority 1** | High-performance index-based DAG arena (`DiGraph`), cycle detection (`is_cyclic_directed`), and topological sorting. | `default` |
| **`schemars`** | `0.8` | `embedag-llm`, `embedag-core` | **Priority 1** | Generates standard JSON Schema from Rust structs at compile time; acts as universal contract for LLM outputs. | `derive`, `chrono`, `uuid` |
| **`serde`** / **`serde_json`** | `1.0` | All crates | **Priority 1** | Universal serialization and deserialization across memory, LLM payloads, and database JSONB. | `derive` |
| **`h3o`** | `0.6` | `embedag-core` | **Priority 1** | 100% pure Rust implementation of Uber H3 hierarchical hexagonal spatial index. Eliminates floating-point Haversine. | `default` |
| **`diesel`** | `2.2` | `embedag-diesel` | **Priority 1** | Type-safe relational database ORM and query builder for PostgreSQL and SQLite. | `postgres`, `sqlite`, `chrono`, `uuid`, `serde_json` |
| **`diesel-async`** | `0.5` | `embedag-diesel` | **Priority 1** | Non-blocking async driver for Diesel over Tokio runtime, eliminating thread starvation on Postgres. | `postgres`, `bb8` |
| **`pgvector`** | `0.4` | `embedag-diesel` | **Priority 1 (PG)** | Native Diesel and PostgreSQL support for high-dimensional vector similarity operations (`<->`, `<=>`). | `postgres`, `diesel` |
| **`zerocopy`** | `0.8` | `embedag-diesel` | **Priority 1 (SQLite)** | Zero-copy byte serialization of `Vec<f32>` embedding vectors into raw SQLite BLOBs. | `derive` |
| **`uuid`** | `1.10` | All crates | **Priority 1** | Globally unique identifiers for graph nodes, edges, and transactional idempotency. | `v4`, `v7`, `serde` |
| **`chrono`** | `0.4` | `embedag-core`, `embedag-diesel` | **Priority 1** | High-precision UTC timestamp management, duration parsing, and cyclical temporal conversions. | `serde` |
| **`tokio`** | `1.40` | `embedag-llm`, `embedag-diesel`, `embedag-rag` | **Priority 1** | Standard asynchronous runtime for async I/O and blocking task isolation. | `rt-multi-thread`, `macros` |
| **`async-trait`** | `0.1` | `embedag-core` | **Priority 1** | Enables object-safe asynchronous methods on public domain traits (`GraphRepository`, `GraphClassifier`). | `default` |
| **`thiserror`** | `1.0` / `2.0` | All crates | **Priority 1** | Idiomatic, strongly-typed domain error declarations with zero boilerplate. | `default` |
| **`syn`** / **`quote`** / **`proc-macro2`** | `2.0` | `embedag-derive` | **Priority 1** | Standard AST manipulation and code generation toolchain for Rust derive macros. | `full` |
| *`fastembed`* | `4.0` | `embedag-core`, `embedag-rag` | *Priority 2 (Optional)* | ONNX Runtime C-FFI local text embeddings. Feature-gated behind `fastembed`. | `default` |
| *`candle-core`* | `0.8` | `embedag-llm` | *Priority 2 (Optional)* | Alternative pure-Rust ML framework from Hugging Face. Feature-gated behind `candle`. | `default` |
| *`reqwest`* | `0.12` | `embedag-llm` | *Priority 3 (Cloud)* | Asynchronous HTTP client for cloud LLM APIs (OpenAI, Anthropic). Feature-gated behind `cloud`. | `json`, `rustls-tls` |

---

## 4. Architectural Harmony & Mutual Compatibility
1. **Zero FFI Collisions**: By standardizing on `oxibonsai` and `trustformers` as Priority 1, the default build has zero C/C++ dependencies and builds cleanly with `cargo build`.
2. **Deterministic Locking**: Check in `Cargo.lock` to guarantee reproducible builds.
3. **Feature Isolation**: Adapters are modularly feature-flagged, ensuring edge/offline builds do not pull in PostgreSQL dependencies, and pure-Rust builds do not pull in C-FFI runtimes.
