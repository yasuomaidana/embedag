# External Repositories & The KitaSan Stack (Priority 1)

## 1. Executive Summary & Strategic Ranking
To satisfy the requirement that `embedag` must be **100% written in Rust** with zero runtime C/C++ or Python dependencies, the **KitaSan ecosystem (`cool-japan` / COOLJAPAN OÜ)** is adopted as the **Priority 1 (Default) Machine Learning, Inference, and GraphRAG Foundation**.

Other frameworks (such as Hugging Face Candle, FastEmbed ONNX Runtime, or remote Cloud APIs like OpenAI/Anthropic) are retained as secondary or optional fallback backends, but the **KitaSan stack represents the primary first-class architectural core**.

---

## 2. In-Depth Analysis of the KitaSan Ecosystem (`cool-japan`)

```
┌────────────────────────────────────────────────────────────────────────┐
│                        embedag Extraction & RAG                        │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
    ┌───────────────────────────────┼───────────────────────────────┐
    ▼                               ▼                               ▼
┌───────────────────────┐ ┌───────────────────┐ ┌───────────────────────┐
│     oxibonsai         │ │   trustformers    │ │    oxirs / oxirag     │
│  (Priority 1 LLM)     │ │ (Priority 1 Embed)│ │  (Priority 1 Graph)   │
│ - Pure Rust Inference │ │ - Pure Rust BPE   │ │ - Subgraph Extraction │
│ - Zero C/C++, Zero FFI│ │ - Pure Rust Trans-│ │ - Vector Indexing     │
│ - Sub-2-bit / Ternary │ │   former Layers   │ │ - Topological Context │
│ - Native SIMD / Metal │ │ - BGE / MiniLM    │ │ - Provenance Auditing │
└───────────────────────┘ └───────────────────┘ └───────────────────────┘
```

### 2.1 `oxibonsai`: Zero-FFI Pure Rust LLM Inference Engine (Priority 1)
- **Repository**: [`cool-japan/oxibonsai`](https://github.com/cool-japan/oxibonsai)
- **Crates.io**: `oxibonsai`, `oxibonsai-core`, `oxibonsai-runtime`, `oxibonsai-tokenizer`, `oxibonsai-kernels`
- **Why It Is Top 1**:
  - **Zero C/C++ / Zero-FFI**: Unlike `llama.cpp` (which requires CMake, C++ compilers, and platform-specific linkers) or PyTorch, `oxibonsai` compiles directly with standard `cargo build`.
  - **Sub-2-Bit & Ternary Quantization**: Natively runs 1-bit (`Q1_0_g128`) and ternary (`TQ2_0_g128`) models, providing ultra-low memory footprints and blazing-fast inference on edge devices, CPU SIMD (AVX-512, NEON), and Apple Metal.
  - **Standalone Pure Rust Tokenizer (`oxibonsai-tokenizer`)**: Directly ingests Hugging Face `tokenizer.json` files without external C dynamic libraries.
  - **In-Process Integration**: Embedded directly into the `embedag-llm` binary with zero network latency, running token-by-token constrained sampling in memory.

### 2.2 `trustformers`: Pure Rust Neural Embeddings & Transformers (Priority 1)
- **Repository**: [`cool-japan/trustformers`](https://github.com/cool-japan/trustformers)
- **Why It Is Top 1 for Embeddings**:
  - Eliminates the Microsoft ONNX Runtime C library dependency required by `fastembed`.
  - Provides native Rust tensor operations, embedding layers, and transformer attention for dense text embeddings (e.g. BGE, MiniLM architectures).
  - Guarantees true 100% Rust memory safety and seamless cross-compilation (x86_64, aarch64, and WebAssembly).

### 2.3 `oxirs` & `oxirag`: Pure Rust Semantic Web & GraphRAG (Priority 1)
- **Repositories**: [`cool-japan/oxirs`](https://github.com/cool-japan/oxirs), [`cool-japan/oxirag`](https://github.com/cool-japan/oxirag)
- **Key Concepts Ingested**:
  - **Bounded Graph Context Synthesis**: Pulling scoped topological subgraphs (k-hop neighborhoods) to feed into prompt generation.
  - **Hybrid Vector + Graph Indices (`oxirs-vec`)**: Unifying high-dimensional vector similarity with topological edge traversals.
  - **Provenance Tracking**: Maintaining strict audit trails distinguishing deterministic facts from model inferences.

---

## 3. Analysis of `leynos/diesel-cte-ext` (Priority 1 Relational CTE Extension)

### 3.1 Project Profile & Architectural Value
- **Repository**: [`leynos/diesel-cte-ext`](https://github.com/leynos/diesel-cte-ext)
- **Why It Is Adopted Directly**:
  - Solves the single greatest hurdle in Rust relational graph persistence: **Diesel's lack of native `WITH RECURSIVE` DSL support**.
  - Provides typed DSL builders (`RecursiveParts`, `with_recursive`, `seed_query!`, `step_query!`, `cte_query!`).
  - Supports both **PostgreSQL** and **SQLite**.
  - Compatible with `diesel-async`, enabling non-blocking execution in high-concurrency Tokio environments.

---

## 4. Priority Ranking Matrix for Machine Learning & Persistence

| Component Domain | Priority 1 (Default / Core) | Priority 2 (Alternative / Feature Gated) | Priority 3 (Cloud Fallback) |
|---|---|---|---|
| **Local LLM Inference** | **`oxibonsai` (KitaSan Pure Rust)** | `candle` (Hugging Face pure Rust) / `llama.cpp` | OpenAI / Anthropic REST APIs |
| **Local Text Embeddings** | **`trustformers` / `oxirag` (KitaSan)** | `fastembed` (ONNX C-FFI) / `candle` | OpenAI `text-embedding-3` |
| **GraphRAG Topology** | **`oxirs` & `petgraph`** | Custom Adjacency Lists | External Graph DBs (Neo4j) |
| **Relational Persistence** | **`diesel` + `diesel-async` + `diesel-cte-ext`** | Raw SQL strings / `sqlx` | NoSQL / Document DBs |

---

## 5. Architectural Harmony & Conflict Verification
- **No FFI Clashes**: By selecting `oxibonsai` as Priority 1, `embedag` avoids linker collisions between C++ standard libraries, OpenMP runtimes, or dynamic ONNX libraries.
- **Async Runtime Compatibility**: `oxibonsai` CPU kernels execute synchronously inside Tokio's `spawn_blocking` pool, while `diesel-async` handles PostgreSQL I/O without blocking the Tokio runtime.
- **Zero Schema Conflicts**: `oxibonsai`'s native token sampler directly integrates with `schemars`-derived grammar constraints.
