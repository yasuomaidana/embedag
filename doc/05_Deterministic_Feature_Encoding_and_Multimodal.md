# Deterministic Feature Encoding & Multimodal Fusion

## 1. Architectural Motivation: Separation of Determinism vs. Probability
A primary design principle of `embedag` is that **Large Language Models must never be entrusted with raw arithmetic, geospatial proximity, or continuous temporal reasoning**.

When numeric values (e.g., coordinates `[-99.1332, 19.4326]`, timestamps `"2026-10-11T20:30:00Z"`, or package weights `"50.0g"`) are injected into text prompts, LLMs suffer from severe failure modes:
1. **Tokenization Fragmentation**: Floating-point numbers are split into arbitrary BPE sub-tokens (e.g., `50`, `.`, `004`), destroying numerical ordering and magnitude in vector representations.
2. **Numeric Drift & Hallucination**: Generative models cannot execute non-linear trigonometric formulas or spatial containment checks reliably.
3. **Inference Latency & Token Bloat**: Serializing raw numeric matrices into text inflates token usage and increases request latency by orders of magnitude.

Therefore, `embedag` enforces **deterministic-first routing**: continuous, temporal, and spatial features bypass the LLM and are processed through native Rust encoders before fusion.

---

## 2. Deterministic Feature Encoders

```
┌────────────────────────────────────────────────────────┐
│                   Raw Input Struct                     │
│    [Text Prompt, Scalars/Weights, Timestamps, Lat/Lon] │
└───────────────────────────┬────────────────────────────┘
                            │
            ┌───────────────┴───────────────┐
            ▼                               ▼
┌───────────────────────────────┐ ┌───────────────────────────────┐
│     Probabilistic Path        │ │      Deterministic Path       │
│  - Text Extraction (oxibonsai)│ │  - Scalars: Range/Log Scale   │
│  - Dense Embeddings           │ │  - Time: Cyclical Sin/Cos     │
│    (Priority 1: trustformers/ │ │  - Geo: Uber H3 Cells (h3o)   │
│     oxirag; P2: fastembed)    │ │                               │
└───────────────┬───────────────┘ └───────────────┬───────────────┘
                │                                 │
                └───────────────┬─────────────────┘
                                ▼
┌────────────────────────────────────────────────────────┐
│             Composite Hybrid Distance                  │
│       D_total = w_text*D_ang + w_num*D_gower + w_geo*D │
└───────────────────────────────┬────────────────────────┘
                                │
                                ▼
                       In-Memory petgraph DAG
```

### 2.1 Continuous Scalar Normalization
To prevent large-scale metrics (e.g., item price $10,000) from dominating small-scale metrics (e.g., item weight 0.05kg), `embedag` applies bounded normalization:

1. **Min-Max Scaling**:
   $$x_{\text{norm}} = \frac{x - x_{\min}}{x_{\max} - x_{\min}}$$
2. **Relative Scale-Invariant Difference (for pairwise comparison)**:
   $$S_{\text{num}}(x, y) = 1.0 - \min\left(1.0, \frac{|x - y|}{\max(|x|, |y|, \epsilon)}\right)$$
3. **Logarithmic Scaling (for heavy-tailed distributions like prices or views)**:
   $$x_{\text{log}} = \frac{\ln(1 + x)}{\ln(1 + x_{\max})}$$

### 2.2 Cyclical Temporal Encoding
Standard linear representations of timestamps fail at boundaries: $23:59$ and $00:01$ are 2 minutes apart in reality, but numerically 23.98 units apart on a linear scale.

`embedag` maps timestamps onto continuous unit circles:
$$x_{\text{hour\_sin}} = \sin\left(\frac{2\pi \cdot \text{hour}}{24}\right), \quad x_{\text{hour\_cos}} = \cos\left(\frac{2\pi \cdot \text{hour}}{24}\right)$$
$$x_{\text{day\_sin}} = \sin\left(\frac{2\pi \cdot \text{day\_of\_week}}{7}\right), \quad x_{\text{day\_cos}} = \cos\left(\frac{2\pi \cdot \text{day\_of\_week}}{7}\right)$$

In addition to cyclical vectors, the elapsed delta time ($\Delta t = t_1 - t_0$) is retained as a continuous monotonic feature for temporal DAG edge construction (`EventA --followed_by--> EventB`).

### 2.3 Geospatial Hierarchical Hexagons via `h3o`
Rather than calculating expensive trigonometric Haversine distance over floating-point coordinates `(lat, lon)`, `embedag` utilizes Uber's **H3 Hierarchical Hexagonal Spatial Index** implemented in 100% pure Rust via the `h3o` crate:
- **64-bit Integer Indexing**: Every coordinate resolves to a compact `u64` index at resolutions 0 to 15.
- **Topological Grid Rings**: Distance between two locations is computed via integer cell traversals (`grid_distance`), eliminating floating-point math during candidate search.
- **Hierarchical Containment**: Parent cell lookups allow instant spatial clustering using bitwise shifts.

---

## 3. Local Text Embeddings: Priority 1 vs Fallback Backends

### 3.1 Priority 1: `trustformers` & `oxirag` (The KitaSan Pure Rust Stack)
- **Engine**: Pure Rust Transformer layers and tokenizers developed under `cool-japan/trustformers`.
- **Zero FFI**: Zero C/C++ compilation, zero dynamic link libraries, and zero runtime dependencies.
- **Models**: Native BGE-small, MiniLM, and multilingual embedding checkpoints.
- **Thread Safety**: Runs deterministically within Rust rayon thread pools or Tokio blocking workers.

### 3.2 Priority 2: `fastembed-rs` (ONNX Runtime Fallback)
- Available via the optional `fastembed` feature flag. Uses Microsoft ONNX Runtime binaries via C-FFI for high-throughput batching when ONNX hardware acceleration is specifically preferred.

---

## 4. Multimodal Asset Pipeline
For records containing images, packaging photos, or binary attachments:
1. Fields marked with `#[graph(image, mime = "image/jpeg")]` are captured as `MultimodalBlob`.
2. Payloads are checked for size limits and MIME compliance.
3. If dispatched to vision models (such as `oxibonsai-image` in pure Rust, or cloud VLMs like GPT-4o), blobs are converted to zero-copy base64 buffers or raw byte slices.

---

## 5. Composite Hybrid Distance Metric (Gower + Angular + Geo)
When comparing two candidate nodes $A$ and $B$ containing mixed data types:
$$D_{\text{total}}(A, B) = w_{\text{vec}} \cdot D_{\text{angular}}(A, B) + w_{\text{num}} \cdot D_{\text{scalar}}(A, B) + w_{\text{geo}} \cdot D_{\text{h3}}(A, B)$$

Where:
- $D_{\text{angular}} = \frac{1 - \cos(\mathbf{v}_A, \mathbf{v}_B)}{2} \in [0, 1]$
- $D_{\text{scalar}} = \frac{1}{M} \sum_{k=1}^M \frac{|x_{A,k} - x_{B,k}|}{\text{range}(x_k)} \in [0, 1]$ (Gower continuous metric)
- $D_{\text{h3}} = \min\left(1.0, \frac{\text{grid\_distance}(\text{cell}_A, \text{cell}_B)}{\text{max\_radius}}\right) \in [0, 1]$

If $D_{\text{total}} \le \tau_{\text{threshold}}$, an edge is formed deterministically without invoking the LLM.
