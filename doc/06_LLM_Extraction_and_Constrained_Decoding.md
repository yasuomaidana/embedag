# LLM Extraction Layer & Constrained Decoding (Second-Pass Specification)

## 1. Overview & Architectural Objectives
The extraction layer of `embedag` bridges unstructured natural language with strictly typed Rust DAG data structures. 

Instead of relying on fragile prompt engineering, `embedag` enforces **mathematical schema compliance** through compile-time schema derivation and constrained decoding. The LLM functions purely as a linguistic classifier and entity extractor, never as an unconstrained free-form generator.

The **KitaSan stack (`oxibonsai`)** is the **Priority 1 (Default) native engine**, providing zero-FFI, pure-Rust execution.

---

## 2. Universal Schema Derivation via `schemars`
To maintain a single source of truth, all extraction targets derive `schemars::JsonSchema` alongside `serde::Serialize` and `serde::Deserialize`.

```rust
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ExtractionDirective {
    AssignExisting {
        #[schemars(description = "Business key or slug of the existing parent node in the graph")]
        parent_key: String,
        relationship: String,
        confidence: f32,
    },
    InsertSubtree {
        #[schemars(description = "Anchor parent path or key where new nodes should attach")]
        anchor_parent_key: String,
        new_nodes: Vec<NewNodeProposal>,
        reasoning: String,
    },
    Unknown {
        unresolved_text: String,
        suggested_category: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NewNodeProposal {
    pub key: String,
    pub name: String,
    pub entity_level: String,
    pub attributes: serde_json::Value,
}
```

At compile time or startup, `embedag-llm` generates the canonical JSON Schema representation from `ExtractionDirective`.

---

## 3. Deep Dive: `oxibonsai` Execution Engine (Priority 1)

### 3.1 Zero-FFI Pushdown Automaton Logit Masking
In `oxibonsai`, token generation is constrained at the token-sampling level using a Pushdown Automaton (PDA) compiled from the `schemars` JSON Schema or GBNF grammar:

```
Token Generation Step (oxibonsai-runtime)
  1. Transformer Forward Pass -> Unconstrained Logits L ∈ R^|V| (|V| ~ 128,000)
  2. Pushdown Automaton (PDA) queries current grammar state:
     - State S_t permits valid next Unicode characters: C_valid = {'"', 'a'..'z', ':', ...}
  3. Vocabulary Filter:
     - For each token ID in vocabulary:
         If decoded token string contains invalid transitions:
             Logits[token_id] = -∞
  4. Softmax Activation -> Bounded Probability Distribution:
     - P(token_id) = 0.0 for all schema-violating tokens
  5. Sample next token -> Update PDA State S_{t+1}
```

Because invalid tokens have $-\infty$ logits, the model is **mathematically incapable of generating malformed JSON, unclosed brackets, or undeclared fields**.

### 3.2 Threading & Async Runtime Isolation
`oxibonsai`'s CPU SIMD kernels (AVX2, AVX-512, NEON) are CPU-bound. To prevent blocking Tokio event loops:
- `OxibonsaiProvider` wraps model execution inside `tokio::task::spawn_blocking`.
- Inference runs on dedicated OS worker threads while `diesel-async` and network I/O continue smoothly on Tokio worker threads.

### 3.3 KV Cache Management & Speculative Decoding
- **KV Cache Reuse**: Reusable prompt templates (system instructions and static taxonomy definitions) are cached in the `oxibonsai` KV cache across queries, reducing time-to-first-token (TTFT) by up to 70%.
- **Speculative Decoding**: Employs small draft models (`oxibonsai` sub-1B ternary models) to speculate structural syntax tokens (e.g. `{"action": "assign_existing"}`), which are verified in single forward passes by the target model.

---

## 4. Mitigating the Alignment Tax: Draft-Then-Decode
While constrained decoding eliminates syntax errors, immediate rigid schema enforcement imposes an **alignment tax**:
- Forcing a model to produce structured enum tags immediately robs it of intermediate working tokens (Scratchpad / Chain-of-Thought).
- This constraint causes severe accuracy drops on ambiguous, complex classifications.

### 4.1 The Draft-Then-Decode Solution
`embedag` introduces a preliminary reasoning buffer at the root of the schema:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExtractionEnvelope<T> {
    #[schemars(description = "Step-by-step reasoning explaining linguistic intent, ambiguity, and taxonomy placement")]
    pub chain_of_thought: String,

    #[schemars(description = "Structured deterministic payload")]
    pub payload: T,
}
```
Because JSON keys are emitted in schema order, `oxibonsai` first generates tokens into `chain_of_thought`. Once internal attention states have established context, the model populates the strict fields of `payload` with significantly higher classification accuracy.

### 4.2 Telemetry & Provenance Isolation
When deserializing `ExtractionEnvelope<T>`:
- `payload` is forwarded to the in-memory `MemoryDag` changeset diffing engine.
- `chain_of_thought` is attached to the provenance audit record (`EdgePayload::provenance`) and logged for observability, ensuring clean domain payloads without polluting graph node attributes.

---

## 5. Robust Error Handling & Retry Feedback Loop
If an edge-case model response violates constraints or triggers a timeout:
1. **Exponential Backoff**: Up to 3 retries with jitter.
2. **Dynamic Error Feedback (Reprompting)**:
   ```rust
   let feedback_prompt = format!(
       "{user_prompt}\n\n[System Note: Previous output failed validation with error: {err}. Please rectify and follow schema.]"
   );
   ```
3. **Graceful Fallback**: If all retries fail, `ExtractionDirective::Unknown` is emitted, routing the item safely to the Tier 4 Human Review Queue rather than panicking or halting ingestion.

---

## 6. Model-Agnostic Trait: `LlmProvider`

```rust
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    async fn extract_structured<T: schemars::JsonSchema + for<'de> serde::Deserialize<'de> + Send>(
        &self,
        prompt: &str,
        system_context: Option<&str>,
    ) -> Result<ExtractionEnvelope<T>, Self::Error>;
}
```
Through Cargo feature flags:
- `default = ["kitasan"]`: Binds `oxibonsai` as the default engine.
- Optional flags `candle`, `openai`, `anthropic` allow swapping providers without modifying application domain logic.
