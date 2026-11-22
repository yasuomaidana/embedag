# Master Architectural Specification & System Catalog

| Metadata           | Details                                                                                                                  |
|:-------------------|:-------------------------------------------------------------------------------------------------------------------------|
| **Document ID**    | `00_MASTER_SPECIFICATIONS`                                                                                               |
| **Domain**         | Master System Architecture & Global Specifications Catalog                                                               |
| **Responsibility** | Global Architectural Blueprint, Core Invariants, KitaSan Open Source Stack Integration, and Modular Specifications Index |
| **Target Crates**  | Entire `embedag` Workspace (`embedag-core`, `embedag-derive`, `embedag-llm`, `embedag-diesel`, `embedag-rag`, `embedag`) |
| **Status**         | Definitive Master Specification                                                                                          |

---

## 1. Executive Summary & Architectural Vision

`embedag` is an enterprise-grade, **100% written in Rust** framework for **deterministic and probabilistic Directed
Acyclic Graph (DAG) construction, multimodal entity resolution, relational persistence, and hybrid GraphRAG (
Retrieval-Augmented Generation)**.

Unlike naive vector search or rigid flat taxonomies, `embedag` models real-world business domains as **multi-parent
directed acyclic graphs**, resolving fuzzy, multimodal input records (such as unstructured invoices, product catalogs,
sensor telemetry, video frames, and conversational text) into mathematically verified, cycle-free hierarchical
topologies.

### 1.1 Core Architectural Tenets

1. **100% Pure Rust & Zero FFI**: Zero runtime dependencies on C/C++ runtimes (e.g. `llama.cpp`, CMake, FFmpeg C
   libraries) or Python. The entire machine learning inference, tokenization, computer vision, vector search, and graph
   traversal stack compiles natively with standard `cargo build`.
2. **Priority 1 Foundation (The KitaSan Open Source Stack)**: Primary machine learning inference, computer vision,
   semantic web, and GraphRAG capabilities are powered by the **KitaSan open-source ecosystem (`cool-japan` /
   COOLJAPAN OÜ)** (`oxirs`, `oximedia`), Hugging Face `candle`, and pure-Rust `VerifiedRagEngine`:
    - `cool-japan/oxibonsai`: (Not implemented; removed due to upstream licence change to BUSL-1.1, 2026-11). Sub-2-bit (1-bit and ternary) LLM execution engine with pushdown automaton logit masking.
    - `cool-japan/oxillama`: (Not implemented; removed due to upstream licence change to BUSL-1.1, 2026-11). Sovereign pure Rust alternative to `llama.cpp` for GGUF model loading, multi-quantized inference, and 25 model architectures.
    - `cool-japan/oxirag`: (Not implemented; removed due to upstream licence change to BUSL-1.1, 2026-11). Four-layer pure-Rust RAG engine (Echo vector store, Speculator, Judge SMT-based logic verification via `OxiZ` solver, and Layer 4 GraphRAG). Replaced by pure-Rust `VerifiedRagEngine`.
    - `cool-japan/oxirs`: Modular Semantic Web platform providing SPARQL 1.1/1.2 query execution (`oxirs-arq`), durable
      on-disk TDB2 storage (`oxirs-tdb`), RDF-Star, and GraphML/JSON-LD serialization.
    - `cool-japan/oximedia`: Patent-free multimedia and computer vision framework (pure Rust clean-room reconstruction
      of FFmpeg + OpenCV) providing royalty-free codec decoding (AV1, VP9, VP8, Opus, FLAC), video frame extraction, and
      forensic image analysis.
    - *Strict Open-Source Boundary*: Only free Apache-2.0 open-source repositories from `cool-japan/cool-japan` are
      utilized; commercial licensed vertical workspaces (`autors`, `med`, `cad`, etc.) and BUSL-1.1 relicensed repositories (`oxirag`, `oxillama`, `oxibonsai`) are strictly excluded and guarded by `deny.toml`. Local inference uses `candle` (GPU first) and an HTTP provider; verified retrieval uses `VerifiedRagEngine`.
3. **Multi-Parent DAG Topologies**: Entities can belong to multiple conceptual parents simultaneously (e.g., a product
   node belonging to both *Confectionery -> Chocolate* and *Dietary -> Gluten-Free*), strictly guarded against circular
   loops.
4. **Separation of Determinism and Probability**: Numerical, temporal, and spatial attributes are transformed using
   deterministic mathematical encoders (MinMax, Z-score, cyclical trigonometry, Uber H3 grid indexing), preventing
   probabilistic LLM hallucinations and floating-point arithmetic errors.
5. **Cycle Guards & Typestate Safety**: Cycles are prevented across two independent defense barriers:
    - Compile-time typestate contracts (`MutationPlan<T, CycleValidated>`) and in-memory Tarjan Strongly Connected
      Components (SCC) validation.
    - Database-level cycle prevention in recursive SQL CTEs powered by [
      `leynos/diesel-cte-ext`](https://github.com/leynos/diesel-cte-ext) (`RecursiveCTEExt`, `with_recursive`,
      `with_recursive_not_all`) with array path tracking.
6. **Hexagonal Architecture (Ports & Adapters)**: Domain logic in `embedag-core` is strictly decoupled from storage
   implementations (`GraphRepository` trait) and inference runtimes (`LlmProvider` trait).
7. **Mandatory Containerized Persistence Verification**: Relational persistence adapters and future database backends
   are mandatorily tested against real disposable Docker containers managed via
   **[Testcontainers for Rust](https://rust.testcontainers.org)** (`testcontainers`). Testing against mocks or relying
   solely on SQLite is strictly prohibited for verifying PostgreSQL and vector search functionality.

---

## 2. End-to-End System Architecture

```mermaid
flowchart TD
    subgraph Ingestion ["1. MULTIMODAL INGESTION & CONTEXT"]
        Raw["Unstructured Text / Invoices / Video Frames / Audio Telemetry"] --> Media["oximedia Processing<br/>- Pure Rust Codec Decoding (AV1, VP9, Opus)<br/>- Computer Vision Frame & Feature Extraction"]
        Media --> Derive["#[derive(AsGraphContext)]"]
    end

    subgraph Encoders ["2. PARALLEL FEATURE EXTRACTION"]
        Derive --> Det["Deterministic Feature Encoding<br/>- Continuous Scalars (MinMax / Z-Score)<br/>- Cyclical Temporal Trigonometry [sin, cos]<br/>- Uber H3 Spatial Hexagonal Cells"]
        Derive --> Neural["Neural & Multimodal Embeddings<br/>- trustformers Embeddings<br/>- Pure-Rust BPE Tokenizer<br/>- Multimodal Asset Vectors via oximedia"]
    end

    subgraph Resolution ["3. MULTI-TIERED WATERFALL RESOLUTION"]
        Det & Neural --> Tiers["Waterfall Resolution Pipeline<br/>- Tier 1: Exact Key & Alias Cache (O(1))<br/>- Tier 2: Composite Hybrid Distance (Gower + Angular + Geo)<br/>- Tier 3: Pure-Rust LLM Inference (Candle / HTTP)<br/>- Tier 4: Dead-Letter Queue & Human Review"]
    end

    subgraph Mutation ["4. MUTATION LIFECYCLE & CYCLE GUARDS"]
        Tiers --> Mut["Graph Mutation Engine<br/>- Directives: InsertNode, UpdateAttributes, RetargetEdge<br/>- Shadow DAG Simulation (Tarjan SCC Acyclic Guard)<br/>- Typestate: MutationPlan&lt;T, CycleValidated&gt;"]
    end

    subgraph Persistence ["5. RELATIONAL PERSISTENCE & RECURSIVE CTES"]
        Mut --> DB["Relational Persistence Engine<br/>- Atomic Multi-Row Upserts (diesel-async PostgreSQL / SQLite)<br/>- Recursive CTE Traversals via leynos/diesel-cte-ext<br/>- SQL Visited Path Cycle Prevention (with_recursive_not_all)<br/>- Mandatory Testcontainers Harness (rust.testcontainers.org)"]
    end

    subgraph GraphRAG ["6. HYBRID TWO-STAGE GRAPHRAG & VERIFICATION"]
        DB --> RAG["Hybrid GraphRAG (VerifiedRagEngine & oxirs)<br/>- Stage 1: Dual Vector + Sparse (BM25) with RRF<br/>- Stage 2: Topological Subgraph Expansion & Grounded Citations<br/>- Verification: Pure-Rust VerifiedRagEngine Chunk Citations<br/>- Semantic Interop: oxirs SPARQL 1.2 & RDF-Star Serialization"]
    end
```

---

## 3. Modular Specification Catalog (Single Responsibility & Domain)

To guarantee high cohesion, maintainability, and clean domain boundaries, the documentation suite is partitioned into 22
dedicated specifications:

| ID   | Specification Document                                                                                                                                                    | Domain                         | Primary Architectural Responsibility                                                                               |
|:-----|:--------------------------------------------------------------------------------------------------------------------------------------------------------------------------|:-------------------------------|:-------------------------------------------------------------------------------------------------------------------|
| `00` | [00_MASTER_SPECIFICATIONS.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/00_MASTER_SPECIFICATIONS.md)                                                       | Master Architecture            | Master blueprint, system tenets, KitaSan open-source stack & diesel-cte-ext integration catalog.                   |
| `01` | [01_Development_Plan.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/01_Development_Plan.md)                                                                 | Project Governance             | Scrum milestones, epics, user stories, acceptance criteria, test specifications.                                   |
| `02` | [02_Workspace_Crate_Topology.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/02_Workspace_Crate_Topology.md)                                                 | Workspace Architecture         | Crate boundaries, dependency graphs, test dependencies, and isolation layers.                                      |
| `03` | [03_Architectural_Design_Patterns.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/03_Architectural_Design_Patterns.md)                                       | Design Patterns                | Idiomatic Rust patterns, hexagonal ports, typestates, CTE builder pattern, Testcontainers pattern.                 |
| `04` | [04_Core_Domain_and_Entity_Contracts.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/04_Core_Domain_and_Entity_Contracts.md)                                 | Domain Contracts               | Core traits: `AsGraphContext`, `NodePayload`, `GraphTarget`, `GraphClassifier`.                                    |
| `05` | [05_In_Memory_DAG_and_Arena_Topology.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/05_In_Memory_DAG_and_Arena_Topology.md)                                 | Graph Topology                 | `MemoryDag<T>`, generational indices, `petgraph`, arena memory layout.                                             |
| `06` | [06_Procedural_Derive_Macros.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/06_Procedural_Derive_Macros.md)                                                 | Metaprogramming                | `embedag-derive`, `#[derive(AsGraphContext)]`, syn/quote compile-time AST generation.                              |
| `07` | [07_Deterministic_Feature_Encoding.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/07_Deterministic_Feature_Encoding.md)                                     | Feature Engineering            | Mathematical feature encoders: scalars, cyclical time, and Uber H3 spatial cells.                                  |
| `08` | [08_Neural_and_Multimodal_Embeddings.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/08_Neural_and_Multimodal_Embeddings.md)                                 | Neural & Multimodal            | Local embeddings via `trustformers`, `oximedia` multimedia/CV pipelines (oxirag Echo store not implemented).        |
| `09` | [09_Mixed_Modality_Representation_and_Similarity.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/09_Mixed_Modality_Representation_and_Similarity.md)         | Similarity Engine              | `UnifiedNode<T>`, composite hybrid distance (Gower + Angular + H3), LLM delta cards.                               |
| `10` | [10_Structured_Extraction_and_Schema_Derivation.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/10_Structured_Extraction_and_Schema_Derivation.md)           | Extraction Schemas             | Universal schema synthesis (`schemars`), `ExtractionDirective`, Draft-Then-Decode.                                 |
| `11` | [11_Pure_Rust_LLM_Inference_and_Constrained_Decoding.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/11_Pure_Rust_LLM_Inference_and_Constrained_Decoding.md) | LLM Inference                  | `CandleProvider` GPU-first inference, HTTP provider, PDA logit masking (oxibonsai and oxillama not implemented).  |
| `12` | [12_Multi_Tiered_Waterfall_Resolution.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/12_Multi_Tiered_Waterfall_Resolution.md)                               | Entity Resolution              | 4-tier waterfall: exact match, hybrid distance, LLM arbitration, human review.                                     |
| `13` | [13_Topological_Cycle_Detection_and_Guards.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/13_Topological_Cycle_Detection_and_Guards.md)                     | Topological Invariants         | Cycle prevention algorithms: Tarjan SCC, `would_create_cycle`, typestate validation.                               |
| `14` | [14_Dynamic_Graph_Mutations_and_Diffing.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/14_Dynamic_Graph_Mutations_and_Diffing.md)                           | Change Management              | `GraphDiff`, Mutation Directives execution lifecycle, shadow DAG simulation, upserts.                              |
| `15` | [15_Relational_Persistence_and_Schema_Engine.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/15_Relational_Persistence_and_Schema_Engine.md)                 | Persistence                    | `GraphRepository` port, Diesel schema, PostgreSQL `pgvector`, SQLite, batch transactions.                          |
| `16` | [16_Hierarchical_Recursive_CTE_Traversals.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/16_Hierarchical_Recursive_CTE_Traversals.md)                       | SQL Graph Queries              | Recursive CTE queries via `leynos/diesel-cte-ext`, path tracking, depth limits, indices.                           |
| `17` | [17_Hybrid_Vector_and_Lexical_Retrieval.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/17_Hybrid_Vector_and_Lexical_Retrieval.md)                           | Retrieval Fusion               | Stage 1 search: Dense vector store, BM25 sparse lexical tokens, RRF fusion (oxirag not implemented).              |
| `18` | [18_Topological_Subgraph_Expansion_and_GraphRAG.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/18_Topological_Subgraph_Expansion_and_GraphRAG.md)           | GraphRAG Synthesis             | Stage 2 subgraph expansion, topological decay, grounded citations via `VerifiedRagEngine` (oxirag not implemented).|
| `19` | [19_External_Ecosystem_and_KitaSan_Stack.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/19_External_Ecosystem_and_KitaSan_Stack.md)                         | Vendor Evaluation              | Technical audit of KitaSan stack (`oxirs`, `oximedia`), diesel-cte-ext, and licence change notes.                 |
| `20` | [20_Multi_Database_Extensibility_and_Future_Adapters.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/20_Multi_Database_Extensibility_and_Future_Adapters.md) | Future Adapters                | Native adapters: `oxirs` (TDB2 / SPARQL 1.2), SurrealDB, Neo4j, LanceDB.                                           |
| `21` | [21_Heavy_Graph_Handling_and_Topological_Pagination.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/21_Heavy_Graph_Handling_and_Topological_Pagination.md)   | Graph Scalability & Pagination | Streaming traversal iterators, degree pagination (hub slicing), differential cycle check, windowed recursive CTEs. |
| `22` | [22_Dependency_Resolution_and_Merkle_Delta_API.md](22_Dependency_Resolution_and_Merkle_Delta_API.md) | Dependency Resolution & Deltas | Slim `core,matcher` API: deterministic toposort / levels / reverse_deps / critical_path, BLAKE3 Merkle diff, AST similarity graphs. |
| `24` | [24_Longest_Path_and_Graph_Surgery_Primitives.md](24_Longest_Path_and_Graph_Surgery_Primitives.md) | Longest Path & Graph Surgery | Requirement draft: forward and backward longest-path pass with slack, two weights per node, subgraph replacement, incremental range fingerprints, bounded tokens on class pairs |

---

## 4. Key Performance Indicators & Non-Functional Requirements

1. **Compilation & Binary Invariant**:
    - Compiles cleanly on `x86_64` and `aarch64` (Apple Silicon / Linux) with zero C dynamic libraries.
    - `cargo build --workspace` succeeds with zero warnings under `#![deny(missing_docs)]` and strict clippy settings
      (`fail0 + Clippy0`).
2. **Deterministic Encoding Latency**:
    - Scalar normalization, cyclical trigonometry, and H3 cell derivation run in $< 5\,\mu\text{s}$ per record.
3. **In-Memory Graph Mutation Throughput**:
    - Inserting nodes and validating DAG invariants on an active shadow graph runs in $< 50\,\mu\text{s}$ for subgraphs
      up to $10,000$ nodes.
4. **End-to-End Query Latency**:
    - Two-stage Hybrid GraphRAG queries (vector search + CTE expansion + prompt assembly) complete in $< 15\,\text{ms}$
      on PostgreSQL.
5. **Persistent Integration Test Fidelity**:
    - 100% of relational persistence features (`embedag-diesel`) must pass on live containerized PostgreSQL instances
      with `pgvector` enabled via [Testcontainers for Rust](https://rust.testcontainers.org) in CI.
