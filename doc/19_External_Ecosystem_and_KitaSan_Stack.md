# External Ecosystem & The KitaSan Stack (Priority 1)

| Metadata | Details |
| :--- | :--- |
| **Document ID** | `19_External_Ecosystem_and_KitaSan_Stack` |
| **Domain** | Priority 1 Vendor Ecosystem & External Stack Audit |
| **Responsibility** | Technical Audit of External Open Source Repositories: `cool-japan/oxibonsai`, `cool-japan/oxirag`, `cool-japan/oxirs`, `cool-japan/oxillama`, `cool-japan/oximedia`, and `leynos/diesel-cte-ext` |
| **Target Crates** | `embedag-llm`, `embedag-diesel`, `embedag-core`, `embedag-rag`, `embedag` |
| **Status** | Definitive Architectural Specification |

---

## 1. Executive Summary & Strategic Open-Source Mandate

To satisfy the ironclad requirement that `embedag` must be **100% written in Rust** with zero runtime C/C++ dynamic libraries, zero CMake/libtorch bindings, and zero Python runtimes, the **KitaSan open-source ecosystem (`cool-japan` / COOLJAPAN OÜ)** is adopted as the **Priority 1 (Default) Machine Learning, Inference, Semantic Web, Multimedia, and GraphRAG Foundation**.

### 1.1 Strict Open-Source Sovereignty & License Boundary
- **Source of Truth**: [`cool-japan/cool-japan`](https://github.com/cool-japan/cool-japan).
- **Mandate**: **Strictly Open-Source (Apache-2.0 / MIT)**.
- **Explicit Exclusion of Commercial Verticals**: The COOLJAPAN ecosystem provides both open-source foundations and commercial licensed vertical workspaces (e.g., `autors`, `med`, `cad`, `aero`, `eda`, `3d`, `car`). `embedag` **strictly utilizes only the free open-source Apache-2.0 core projects**, never introducing proprietary licensed vertical dependencies or closed codebases.
- **Zero FFI / fail0 + Clippy0**: Every adopted crate in the KitaSan foundation enforces 100% pure Rust, zero C/C++/Fortran FFI, and zero Clippy warnings (`fail0 + Clippy0`).
- **Relational Persistence Partner**: In relational persistence and hierarchical graph queries, [`leynos/diesel-cte-ext`](https://github.com/leynos/diesel-cte-ext) provides first-class recursive CTE support directly atop Diesel and `diesel-async`.

### 1.2 Licence change (2026-11)
oxirag 0.2.0+ and oxillama 0.1.5+ are BUSL-1.1; the Apache-2.0 0.1.x releases are yanked on 2026-11-26; embedag removed them. The other KitaSan crates still used (oxirs, scirs2, oxitls, oxiarc, oximedia) were Apache-2.0 on 2026-11-22 and are guarded by `deny.toml`. Consequently, `oxirag`, `oxillama`, and `oxibonsai` are no longer dependencies of `embedag`.

---

## 2. In-Depth Technical Audit: The KitaSan Open-Source Stack

```mermaid
flowchart TD
    subgraph IngestionMultimodal ["embedag Multimodal Ingestion & In-Memory Core"]
        Core["embedag-core & embedag-derive"]
        OxiMedia["oximedia (Multimedia & Computer Vision)<br/>- Patent-Free Codecs (AV1, VP9, Opus, FLAC)<br/>- Image/Video Frame Extraction & Forensic Verification<br/>- Pure Rust FFmpeg + OpenCV Unified Engine"]
    end

    subgraph LLMExecution ["embedag-llm Inference & Structured Extraction"]
        CandleLLM["Hugging Face candle & HTTP Provider<br/>- GPU-first tensor inference<br/>- Ollama-compatible HTTP provider"]
        OxiBonsai["oxibonsai (Sub-2-Bit Engine)<br/>(Not implemented; removed BUSL-1.1)"]
        OxiLLaMA["oxillama (General GGUF Engine)<br/>(Not implemented; removed BUSL-1.1)"]
    end

    subgraph RelationalCTE ["embedag-diesel Relational Persistence"]
        DieselCTE["leynos/diesel-cte-ext<br/>- RecursiveCTEExt trait (with_recursive, with_recursive_not_all)<br/>- RecursiveParts (Seed, Step, Body fragments)<br/>- PostgreSQL & SQLite Async Support via diesel-async"]
    end

    subgraph GraphRAGSemantic ["embedag-rag Hybrid Retrieval & Semantic Web"]
        VerifiedRAG["VerifiedRagEngine<br/>- Pure-Rust SparseLexicalIndex & RrfRanker<br/>- Grounded citations [n]"]
        OxiRAG["oxirag (4-Layer RAG Engine)<br/>(Not implemented; removed BUSL-1.1)"]
        OxiRS["oxirs (Semantic Web & Graph Interop)<br/>- SPARQL 1.1/1.2 Query Engine (oxirs-arq)<br/>- RDF-Star, JSON-LD, GraphML Export<br/>- Durable TDB2 Persistent Storage Engine"]
    end

    Core --> OxiMedia
    Core --> OxiRS
    Core --> CandleLLM
    Core --> DieselCTE
    Core --> VerifiedRAG
```

---

### 2.1 `oxibonsai`: Zero-FFI Sub-2-Bit LLM Inference Engine (Not Implemented)
- **Status in embedag**: **Not implemented** (sub-2-bit PDA engine was removed due to upstream licence change to BUSL-1.1, 2026-11).
- **Repository**: [`cool-japan/oxibonsai`](https://github.com/cool-japan/oxibonsai)
- **Historical Context**: Formerly used for 1-bit and ternary quantized execution and PDA logit masking on CPU/edge.

### 2.2 `oxillama`: Pure-Rust GGUF LLM Inference Engine (Not Implemented)
- **Status in embedag**: **Not implemented** (sovereign GGUF runtime was removed due to upstream licence change to BUSL-1.1, 2026-11; oxillama 0.1.5+ is BUSL-1.1 and 0.1.x is yanked 2026-11-26).
- **Repository**: [`cool-japan/oxillama`](https://github.com/cool-japan/oxillama)
- **Historical Context**: Formerly evaluated for general GGUF model parsing and multi-quant execution.

### 2.3 `oxirag`: Four-Layer RAG Engine with SMT Logic Verification (Not Implemented)
- **Status in embedag**: **Not implemented** (4-layer RAG engine, SMT logic judge via OxiZ, and Speculator were removed due to upstream licence change to BUSL-1.1, 2026-11; oxirag 0.2.0+ is BUSL-1.1 and 0.1.x is yanked 2026-11-26; replaced by pure-Rust `VerifiedRagEngine`).
- **Repository**: [`cool-japan/oxirag`](https://github.com/cool-japan/oxirag)
- **Historical Context**: Formerly provided Echo vector store, Speculator draft verification, and OxiZ SMT logic checking.

### 2.4 `oxirs`: Modular Semantic Web, SPARQL 1.2 & RDF-Star Platform (Priority 1)
- **Repository**: [`cool-japan/oxirs`](https://github.com/cool-japan/oxirs)
- **License**: Apache-2.0 (Open Source)
- **Crates**: `oxirs-core`, `oxirs-arq`, `oxirs-tdb`, `oxirs-fuseki`
- **Architectural Strengths**:
  - **Complete SPARQL 1.1/1.2 Query Engine**: Powered by `oxirs-arq` with query optimization, CONSTRUCT/DESCRIBE template instantiation, and federated queries.
  - **Durable TDB2 Storage Engine**: Pure Rust on-disk storage backend (`oxirs-tdb`) with superblock, fsync-backed writes, free-page allocator, and GSPO/GPOS/GOSP indexes.
  - **RDF-Star & W3C Standards**: First-class support for RDF-Star statements, Turtle, TriG, N-Quads, JSON-LD, and GraphQL schema autogeneration (`graphql_autoschema`).
  - **Role in `embedag`**: Provides semantic graph export (JSON-LD, RDF-Star) in `embedag-core`, ontology mapping, and SPARQL 1.2 querying over the topological graph in `embedag-rag`.

### 2.5 `oximedia`: Patent-Free Multimedia & Computer Vision Framework (Priority 1 Multimodal)
- **Repository**: [`cool-japan/oximedia`](https://github.com/cool-japan/oximedia)
- **License**: Apache-2.0 (Open Source)
- **Crates**: `oximedia`, `oximedia-cli`, `oximedia-wasm`
- **Architectural Strengths**:
  - **Clean-Room Pure Rust FFmpeg + OpenCV Reconstruction**: Unifies multimedia processing and computer vision in a single memory-safe Rust framework with zero C/C++ dependencies.
  - **Patent-Free Codecs**: Bit-exact pure Rust implementations of AV1, VP9, VP8, Opus, Vorbis, FLAC.
  - **Computer Vision & Video Inspection**: Scene analysis, frame extraction, shot detection, spatial/temporal denoising, camera calibration, color management, and forensic image analysis (ELA, PRNU, copy-move detection).
  - **Async-First Architecture**: Built on Tokio for non-blocking stream processing and media decoding.
  - **Role in `embedag`**: Decodes and preprocesses multimodal inputs (invoices, scanned receipts, product images, video frames, audio telemetry) directly in pure Rust before passing visual/audio features to embedding encoders.

---

## 3. Analysis of `leynos/diesel-cte-ext` (Priority 1 Relational CTE Extension)

- **Repository**: [`leynos/diesel-cte-ext`](https://github.com/leynos/diesel-cte-ext)
- **License**: ISC (Open Source)
- **Architectural Strengths**:
  - **Type-Safe Recursive Query Builders**: Provides the `RecursiveCTEExt` trait directly extending Diesel connections:
    - `.with_recursive(...)` for standard recursive queries (`UNION ALL`).
    - `.with_recursive_not_all(...)` for deduplicated recursive queries (`UNION`).
  - **Composable Parts with `RecursiveParts`**: Encapsulates seed query, step query, and primary query fragments (`RecursiveParts::new(seed, step, body)`).
  - **Macro Helpers**: Offers `columns!`, `table_columns!`, `seed_query!`, `step_query!`, and `cte_query!` to bridge runtime column names to compile-time Diesel metadata.
  - **Async Support**: Ships with the `async` feature enabling `RecursiveCTEExt` on `diesel_async` connection pools (`AsyncPgConnection`).
  - **Cross-Backend Compatibility**: Handles both PostgreSQL and SQLite recursive CTE dialect differences seamlessly without raw SQL string concatenation.
  - **Role in `embedag`**: Powers arbitrary-depth multi-parent hierarchy traversals and SQL-level visited path cycle detection in `embedag-diesel`.

---

## 4. Technology Priority Ranking Matrix

| Technology | Category | Priority Tier | License | Primary Architectural Rationale |
| :--- | :--- | :--- | :--- | :--- |
| `huggingface/candle` | LLM Inference | **Priority 1 (Default)** | Apache-2.0 / MIT | GPU-first pure-Rust tensor inference (Metal / CUDA / CPU) in `CandleProvider`. |
| Ollama HTTP Provider | LLM Inference | **Priority 2 (External GPU)** | Open API | External GPU inference via Ollama-compatible HTTP endpoint. |
| `VerifiedRagEngine` | RAG & Verification | **Priority 1 (Default)** | In-house / Workspace | Pure-Rust verified retrieval with `SparseLexicalIndex`, `RrfRanker`, and cited chunks `[n]`. |
| `cool-japan/oxirs` | Semantic Web & Graph | **Priority 1 (Default)** | Apache-2.0 (Open Source) | RDF-Star, SPARQL 1.1/1.2, TDB2 persistent storage, JSON-LD / GraphML export. |
| `cool-japan/oximedia` | Multimodal Processing | **Priority 1 (Default)** | Apache-2.0 (Open Source) | Pure Rust FFmpeg + OpenCV reconstruction, patent-free codecs, image/video frame extraction. |
| `leynos/diesel-cte-ext` | SQL CTE Builder | **Priority 1 (Default)** | ISC (Open Source) | `RecursiveCTEExt` typed recursive queries (`UNION ALL` & `UNION`), async support for Diesel. |
| `cool-japan/trustformers`| Neural Embeddings | **Priority 1 (Default)** | Apache-2.0 (Open Source) | Pure Rust transformer layers, BGE-M3 & MiniLM tokenization and inference. |
| `fastembed-rs` | Neural Embeddings | **Priority 2 (Fallback)** | Apache-2.0 | Requires ONNX Runtime C library; secondary fallback for legacy models. |
| Cloud APIs (OpenAI / Anthropic)| LLM Inference | **Priority 3 (Optional)** | Proprietary / Cloud | Remote fallback; introduces network roundtrip latency and privacy concerns. |
| `cool-japan/oxibonsai` | LLM Inference | **Not Implemented / Removed** | BUSL-1.1 | Sub-2-bit PDA engine removed due to upstream licence change (2026-11). |
| `cool-japan/oxillama` | LLM Inference | **Not Implemented / Removed** | BUSL-1.1 | GGUF engine removed due to upstream licence change (2026-11; 0.1.x yanked 2026-11-26). |
| `cool-japan/oxirag` | RAG & Logic Verifier | **Not Implemented / Removed** | BUSL-1.1 | 4-layer RAG and SMT judge removed due to licence change (2026-11; 0.1.x yanked 2026-11-26). |
| COOLJAPAN Vertical Workspaces | Vertical Applications | **Excluded (Commercial)** | Commercial | `autors`, `med`, `cad`, `aero`, `eda`, `3d`, `car` are commercial licensed products; NOT used in `embedag`. |
