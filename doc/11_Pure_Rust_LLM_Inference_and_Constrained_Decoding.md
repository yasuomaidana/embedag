# Pure-Rust LLM Inference & Constrained Decoding

| Metadata | Details |
| :--- | :--- |
| **Document ID** | `11_Pure_Rust_LLM_Inference_and_Constrained_Decoding` |
| **Domain** | Zero-FFI LLM Execution & Grammar-Guided Sampling |
| **Responsibility** | `oxibonsai` (Sub-2-Bit) & `oxillama` (GGUF Multi-Quant) Execution Engines, Pushdown Automaton Logit Masking, Async Thread Isolation, and the `LlmProvider` Port |
| **Target Crates** | `embedag-llm` |
| **Status** | Definitive Architectural Specification |

---

## 1. Executive Summary & Architectural Role

To fulfill `embedag`'s core promise of **Zero C/C++ FFI and 100% Pure Rust**, LLM execution is powered strictly by the **KitaSan open-source LLM engines (`cool-japan` / COOLJAPAN OÜ)**:
1. **`cool-japan/oxibonsai` (Sub-2-Bit Engine, memory-constrained CPU deployments)**: Specialized for ultra-low memory footprints (1-bit `Q1_0_g128` and ternary `TQ2_0_g128` quantization) and Pushdown Automaton (PDA) constrained logit masking.
2. **`cool-japan/oxillama` (General GGUF Engine, CPU-only deployments)**: Pure-Rust sovereign replacement for `llama.cpp`. Supports full GGUF model parsing, 25 transformer architectures (LLaMA, Qwen3, Mistral, Gemma, Phi, DeepSeek, etc.), and mainstream quantization formats (`Q4_0`..`Q8_0`, K-quants, I-quants) built atop `SciRS2`, `OxiBLAS`, and `OxiFFT`.

Unlike `llama.cpp` or PyTorch, both engines compile directly with `cargo build` and execute in-process with zero foreign system dependencies.

---

## 2. In-Depth Architecture: Dual Pure-Rust Inference Engines

```mermaid
flowchart TD
    Schema["JSON Schema<br/>(schemars)"] --> Automaton["Pushdown Automaton (PDA)<br/>Grammar Compiler"]
    Automaton --> Masker["Logit Masker<br/>(Allowed Token Bitset)"]
    Prompt["Grounded Subgraph Prompt"] --> EngineSelector{"Engine Router"}

    EngineSelector -->|Sub-2-bit / Edge| OxiBonsai["oxibonsai Runtime<br/>- 1-Bit (Q1_0_g128)<br/>- Ternary (TQ2_0_g128)<br/>- Zero-FFI SIMD / Metal"]
    EngineSelector -->|General GGUF Weights| OxiLLaMa["oxillama Runtime<br/>- GGUF Parser<br/>- Q4_0..Q8_0, K-quants, I-quants<br/>- 25 Architectures via SciRS2/OxiBLAS"]

    OxiBonsai --> RawLogits["Raw Output Logits: [f32; vocab_size]"]
    OxiLLaMa --> RawLogits

    RawLogits & Masker --> MaskedLogits["Masked Logits: -inf on Disallowed Tokens"]
    MaskedLogits --> Sampler["Temperature / Top-P Sampler"]
    Sampler --> NextToken["Next Guaranteed Valid Token"]
    NextToken --> Automaton
```

### 2.1 Zero-FFI Pushdown Automaton (PDA) Logit Masking
At each generation step $t$:
1. The Pushdown Automaton inspects its current state and grammar stack derived from `schemars` JSON schemas.
2. It generates a bitmask of valid next tokens from the vocabulary ($\approx 32,000$ to $128,000$ tokens).
3. Tokens violating the schema syntax receive $-\infty$ logit bias before Softmax:
   $$\text{Logit}'(w) = \begin{cases} \text{Logit}(w) & \text{if } w \in \text{ValidTokens} \\ -\infty & \text{otherwise} \end{cases}$$
4. **Result**: Zero JSON syntax errors, zero missing required fields, and zero trailing commas.

### 2.2 GGUF Multi-Architecture Support with `oxillama`
When general pretrained open weights are provided (e.g. `Meta-Llama-3.1-8B-Instruct.Q4_K_M.gguf`, `Mistral-7B-Instruct-v0.3.Q5_0.gguf`), `embedag-llm` delegates to `oxillama`:
- Complete pure Rust GGUF parser (`oxillama-gguf`) reads metadata and tensor offsets directly via `mmap`.
- SIMD and matrix multiplication kernels run via `OxiBLAS` and `SciRS2`.
- RoPE rotary position embeddings execute via `OxiFFT`.
- Zero dynamic linking to CUDA C runtimes; optional pure Rust GPU kernels available via `oxillama-gpu`, limited to Q4_0 single-token decode.

### 2.3 Provider priority: GPU first

oxillama-runtime 0.1.4 offloads only Q4_0 single-token decode to the GPU (`oxillama-gpu`, wgpu); multi-token prefill, and therefore prompt processing and embeddings, runs on the CPU (measured: over 3 s per 500-token embedding on Apple Silicon, against about 0.1 s for a GPU runtime). Local providers are therefore ranked:

1. `candle` (`CandleProvider`) on Metal or CUDA: whole forward passes, prefill included, run on the GPU.
2. An Ollama-compatible HTTP provider (ticket 10.3.1): GPU inference outside the process.
3. `candle` on the CPU, `oxillama` (GGUF) and `oxibonsai` (sub-2-bit): CPU-only or memory-constrained deployments, and short structured outputs where decode dominates.

---

## 3. Async Isolation & Threadpool Management

Transformer matrix multiplications are CPU/GPU-bound and must never block Tokio worker threads handling network I/O or database transactions.

`embedag-llm` isolates both `oxibonsai` and `oxillama` on dedicated Rayon/blocking worker pools:

```rust
use tokio::task;

pub async fn run_constrained_inference(
    engine: std::sync::Arc<dyn LlmProvider>,
    prompt: String,
    schema: serde_json::Value,
) -> Result<serde_json::Value, LlmError> {
    task::spawn_blocking(move || {
        engine.generate_structured_blocking(&prompt, &schema)
    })
    .await
    .map_err(|e| LlmError::ExecutionPanicked(e.to_string()))?
}
```

---

## 4. The Model-Agnostic Inference Port: `LlmProvider`

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ModelMetadata {
    pub provider_name: String,
    pub model_id: String,
    pub quantization: String,
    pub pure_rust: bool,
}

#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Generates structured output conforming strictly to the provided JSON Schema.
    async fn generate_structured(
        &self,
        prompt: &str,
        schema: &serde_json::Value,
    ) -> Result<serde_json::Value, Self::Error>;

    /// Blocking variant for execution on spawn_blocking / Rayon threadpools.
    fn generate_structured_blocking(
        &self,
        prompt: &str,
        schema: &serde_json::Value,
    ) -> Result<serde_json::Value, Self::Error>;

    /// Model identifier and quantization metadata.
    fn model_info(&self) -> ModelMetadata;
}
```

---

## 5. Invariants & Reliability

1. **Zero FFI Linkage**: Both `oxibonsai` and `oxillama` compile cleanly with standard Cargo and zero system C libraries (`llama.cpp`, CMake, LibTorch).
2. **Deterministic Fallback**: If an edge-device memory budget is exceeded, the runtime gracefully returns `LlmError::OutOfMemory` to trigger Tier 4 human-in-the-loop fallback without crashing the process.
3. **Grammar Pushdown Guarantee**: All generated payloads parse cleanly as valid JSON conforming strictly to `ExtractionEnvelope<ExtractionDirective>`.
