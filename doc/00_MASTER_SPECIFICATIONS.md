# Master Architectural Specification & System Catalog

| Metadata | Details |
| :--- | :--- |
| **Document ID** | `00_MASTER_SPECIFICATIONS` |
| **Domain** | Master System Architecture & Global Specifications Catalog |
| **Responsibility** | Global Architectural Blueprint, Core Invariants, and Modular Specifications Index |
| **Target Crates** | Entire `embedag` Workspace (`embedag-core`, `embedag-derive`, `embedag-llm`, `embedag-diesel`, `embedag-rag`, `embedag`) |
| **Status** | Definitive Master Specification |

---

## 1. Executive Summary & Architectural Vision

`embedag` is an enterprise-grade, **100% written in Rust** framework for **deterministic and probabilistic Directed Acyclic Graph (DAG) construction, entity resolution, relational persistence, and hybrid GraphRAG (Retrieval-Augmented Generation)**.

Unlike naive vector search or rigid flat taxonomies, `embedag` models real-world business domains as **multi-parent directed acyclic graphs**, resolving fuzzy, multimodal input records (such as unstructured invoices, product catalogs, sensor telemetry, and conversational text) into mathematically verified, cycle-free hierarchical topologies.

### 1.1 Core Architectural Tenets
1. **100% Pure Rust & Zero FFI**: Zero runtime dependencies on C/C++ runtimes (e.g. `llama.cpp`, CMake) or Python. The entire machine learning inference, tokenization, vector search, and graph traversal stack compiles natively with standard `cargo build`.
2. **Priority 1 Foundation (The KitaSan Stack)**: Primary machine learning inference and embeddings are powered by the **KitaSan ecosystem (`cool-japan` / COOLJAPAN OÜ)**:
   - `oxibonsai`: Zero-FFI pure Rust LLM execution engine with pushdown automaton logit masking.
   - `trustformers`: Pure Rust transformer layers and BPE tokenization.
   - `oxirs` / `oxirag`: Subgraph extraction and pure Rust graph utilities.
3. **Multi-Parent DAG Topologies**: Entities can belong to multiple conceptual parents simultaneously (e.g., a product node belonging to both *Confectionery -> Chocolate* and *Dietary -> Gluten-Free*), strictly guarded against circular loops.
4. **Separation of Determinism and Probability**: Numerical, temporal, and spatial attributes are transformed using deterministic mathematical encoders (MinMax, Z-score, cyclical trigonometry, Uber H3 grid indexing), preventing probabilistic LLM hallucinations and floating-point arithmetic errors.
5. **Cycle Guards & Typestate Safety**: Cycles are prevented across two independent defense barriers:
   - Compile-time typestate contracts (`MutationPlan<T, CycleValidated>`) and in-memory Tarjan Strongly Connected Components (SCC) validation.
   - Database-level cycle prevention in recursive SQL CTEs (`diesel-cte-ext`) with array path tracking.
6. **Hexagonal Architecture (Ports & Adapters)**: Domain logic in `embedag-core` is strictly decoupled from storage implementations (`GraphRepository` trait) and inference runtimes (`LlmProvider` trait).

---

## 2. End-to-End System Architecture

```mermaid
flowchart TD
    subgraph Ingestion ["1. INGESTION & CONTEXT"]
        Raw["Unstructured Text / Invoices / Catalogs / Telemetry"] --> Derive["#[derive(AsGraphContext)]"]
    end

    subgraph Encoders ["2. PARALLEL FEATURE EXTRACTION"]
        Derive --> Det["Deterministic Feature Encoding<br/>- Continuous Scalars (MinMax / Z-Score)<br/>- Cyclical Temporal Trigonometry [sin, cos]<br/>- Uber H3 Spatial Hexagonal Cells"]
        Derive --> Neural["Neural & Multimodal Embeddings<br/>- trustformers: BGE-M3 / MiniLM Dense Vectors<br/>- Pure-Rust BPE Tokenizer<br/>- Multimodal Asset Vectors"]
    end

    subgraph Resolution ["3. MULTI-TIERED WATERFALL RESOLUTION"]
        Det & Neural --> Tiers["Waterfall Resolution Pipeline<br/>- Tier 1: Exact Key & Alias Cache (O(1))<br/>- Tier 2: Composite Hybrid Distance (Gower + Angular + Geo)<br/>- Tier 3: Pure-Rust oxibonsai Constrained LLM Inference<br/>- Tier 4: Dead-Letter Queue & Human Review"]
    end

    subgraph Mutation ["4. MUTATION LIFECYCLE & CYCLE GUARDS"]
        Tiers --> Mut["Graph Mutation Engine<br/>- Directives: InsertNode, UpdateAttributes, RetargetEdge<br/>- Shadow DAG Simulation (Tarjan SCC Acyclic Guard)<br/>- Typestate: MutationPlan&lt;T, CycleValidated&gt;"]
    end

    subgraph Persistence ["5. RELATIONAL PERSISTENCE & RECURSIVE CTES"]
        Mut --> DB["Relational Persistence Engine<br/>- Atomic Multi-Row Upserts (diesel-async PostgreSQL / SQLite)<br/>- Recursive CTE Traversals (diesel-cte-ext)<br/>- SQL Visited Path Cycle Prevention"]
    end

    subgraph GraphRAG ["6. HYBRID TWO-STAGE GRAPHRAG ENGINE"]
        DB --> RAG["Hybrid GraphRAG<br/>- Stage 1: Dual Vector (HNSW) + Sparse (BM25) with RRF<br/>- Stage 2: Topological Subgraph Expansion & Grounded Citations"]
    end
```

---

## 3. Modular Specification Catalog (Single Responsibility & Domain)

To guarantee high cohesion, maintainability, and clean domain boundaries, the documentation suite is partitioned into 21 dedicated specifications:

| ID | Specification Document | Domain | Primary Architectural Responsibility |
| :--- | :--- | :--- | :--- |
| `00` | [00_MASTER_SPECIFICATIONS.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/00_MASTER_SPECIFICATIONS.md) | Master Architecture | Master blueprint, system tenets, and specification index. |
| `01` | [01_Development_Plan.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/01_Development_Plan.md) | Project Governance | Scrum milestones, epics, user stories, acceptance criteria, test specifications. |
| `02` | [02_Workspace_Crate_Topology.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/02_Workspace_Crate_Topology.md) | Workspace Architecture | Crate boundaries, dependency graphs, and isolation layers. |
| `03` | [03_Architectural_Design_Patterns.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/03_Architectural_Design_Patterns.md) | Design Patterns | Idiomatic Rust patterns, hexagonal ports, typestates, complexity limits. |
| `04` | [04_Core_Domain_and_Entity_Contracts.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/04_Core_Domain_and_Entity_Contracts.md) | Domain Contracts | Core traits: `AsGraphContext`, `NodePayload`, `GraphTarget`, `GraphClassifier`. |
| `05` | [05_In_Memory_DAG_and_Arena_Topology.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/05_In_Memory_DAG_and_Arena_Topology.md) | Graph Topology | `MemoryDag<T>`, generational indices, `petgraph`, arena memory layout. |
| `06` | [06_Procedural_Derive_Macros.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/06_Procedural_Derive_Macros.md) | Metaprogramming | `embedag-derive`, `#[derive(AsGraphContext)]`, syn/quote compile-time AST generation. |
| `07` | [07_Deterministic_Feature_Encoding.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/07_Deterministic_Feature_Encoding.md) | Feature Engineering | Mathematical feature encoders: scalars, cyclical time, and Uber H3 spatial cells. |
| `08` | [08_Neural_and_Multimodal_Embeddings.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/08_Neural_and_Multimodal_Embeddings.md) | Neural Pipelines | Local embeddings via `trustformers` (BGE-M3/MiniLM), tokenizers, and multimodal vectors. |
| `09` | [09_Mixed_Modality_Representation_and_Similarity.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/09_Mixed_Modality_Representation_and_Similarity.md) | Similarity Engine | `UnifiedNode<T>`, composite hybrid distance (Gower + Angular + H3), LLM delta cards. |
| `10` | [10_Structured_Extraction_and_Schema_Derivation.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/10_Structured_Extraction_and_Schema_Derivation.md) | Extraction Schemas | Universal schema synthesis (`schemars`), `ExtractionDirective`, Draft-Then-Decode. |
| `11` | [11_Pure_Rust_LLM_Inference_and_Constrained_Decoding.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/11_Pure_Rust_LLM_Inference_and_Constrained_Decoding.md) | LLM Inference | `oxibonsai` zero-FFI runtime, pushdown automaton logit masking, async isolation. |
| `12` | [12_Multi_Tiered_Waterfall_Resolution.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/12_Multi_Tiered_Waterfall_Resolution.md) | Entity Resolution | 4-tier waterfall: exact match, hybrid distance, LLM arbitration, human review. |
| `13` | [13_Topological_Cycle_Detection_and_Guards.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/13_Topological_Cycle_Detection_and_Guards.md) | Topological Invariants | Cycle prevention algorithms: Tarjan SCC, `would_create_cycle`, typestate validation. |
| `14` | [14_Dynamic_Graph_Mutations_and_Diffing.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/14_Dynamic_Graph_Mutations_and_Diffing.md) | Change Management | `GraphDiff`, Mutation Directives execution lifecycle, shadow DAG simulation, upserts. |
| `15` | [15_Relational_Persistence_and_Schema_Engine.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/15_Relational_Persistence_and_Schema_Engine.md) | Persistence | `GraphRepository` port, Diesel schema, PostgreSQL `pgvector`, SQLite, batch transactions. |
| `16` | [16_Hierarchical_Recursive_CTE_Traversals.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/16_Hierarchical_Recursive_CTE_Traversals.md) | SQL Graph Queries | Recursive CTE queries via `diesel-cte-ext`, path tracking, depth limits, indices. |
| `17` | [17_Hybrid_Vector_and_Lexical_Retrieval.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/17_Hybrid_Vector_and_Lexical_Retrieval.md) | Retrieval Fusion | Stage 1 search: Dense vector + BM25 sparse lexical tokens, Reciprocal Rank Fusion (RRF). |
| `18` | [18_Topological_Subgraph_Expansion_and_GraphRAG.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/18_Topological_Subgraph_Expansion_and_GraphRAG.md) | GraphRAG Synthesis | Stage 2 subgraph expansion, distance decay, query planner DAG, grounded citations. |
| `19` | [19_External_Ecosystem_and_KitaSan_Stack.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/19_External_Ecosystem_and_KitaSan_Stack.md) | Vendor Evaluation | Deep technical audit of `cool-japan/oxibonsai`, `trustformers`, `oxirs`, `diesel-cte-ext`. |
| `20` | [20_Multi_Database_Extensibility_and_Future_Adapters.md](file:///Users/yasuomaidana/RustRoverProjects/embedag/doc/20_Multi_Database_Extensibility_and_Future_Adapters.md) | Future Adapters | Future engines: SurrealDB, Neo4j, LanceDB, Oxigraph, and roadmap enterprise modules. |

---

## 4. Key Performance Indicators & Non-Functional Requirements

1. **Compilation & Binary Invariant**:
   - Compiles cleanly on `x86_64` and `aarch64` (Apple Silicon / Linux) with zero C dynamic libraries.
   - `cargo build --workspace` succeeds with zero warnings under `#![deny(missing_docs)]` and strict clippy settings.
2. **Deterministic Encoding Latency**:
   - Scalar normalization, cyclical trigonometry, and H3 cell derivation run in $< 5\,\mu	ext{s}$ per record.
3. **In-Memory Graph Mutation Throughput**:
   - Inserting nodes and validating DAG invariants on an active shadow graph runs in $< 50\,\mu	ext{s}$ for subgraphs up to $10,000$ nodes.
4. **End-to-End Query Latency**:
   - Two-stage Hybrid GraphRAG queries (vector search + CTE expansion + prompt assembly) complete in $< 15\,	ext{ms}$ on PostgreSQL.
