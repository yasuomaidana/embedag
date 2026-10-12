# Graph Resolution, In-Memory Diffing & Cycle Guards

## 1. Multi-Tiered Waterfall Resolution Pipeline
Invoking an LLM on every incoming record wastes tokens, introduces non-deterministic latency, and ignores historical human corrections. 

To ensure optimal speed, cost efficiency, and determinism, `embedag` processes records through a **4-tier waterfall resolution pipeline**:

```
                  Incoming Unstructured Record
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│ Tier 1: Deterministic Exact / Normalized Alias Store        │
│   - Lowercase stem, punctuation strip, SKU exact match      │
│   - Instant lookup: O(1) in memory / Postgres table         │
└──────────────────────────────┬──────────────────────────────┘
                               │
              ┌────────────────┴────────────────┐
          Match Found?                      No Match
              │                                 │
              ▼                                 ▼
       Return Target Node   ┌────────────────────────────────────────┐
                            │ Tier 2: Hybrid Similarity Search       │
                            │   - trustformers / oxirag (Dense)      │
                            │   - Gower distance on typed scalars    │
                            └───────────────────┬────────────────────┘
                                                │
                               ┌────────────────┴────────────────┐
                       Confidence >= 0.85?               Ambiguous / Low
                               │                                 │
                               ▼                                 ▼
                        Attach / Update Node   ┌──────────────────────────────┐
                                               │ Tier 3: oxibonsai Local LLM  │
                                               │   - Top candidate anchors    │
                                               │   - GBNF / Structured JSON   │
                                               └───────────────┬──────────────┘
                                                               │
                                              ┌────────────────┴────────────────┐
                                         High Conf?                     Uncertain
                                              │                                 │
                                              ▼                                 ▼
                                      Apply Mutation             ┌─────────────────────────────┐
                                                                 │ Tier 4: Review Queue & Loop │
                                                                 │   - Route to 'Unknown'      │
                                                                 │   - Human correction caches │
                                                                 │     into Tier 1 Alias Store │
                                                                 └─────────────────────────────┘
```

### 1.1 Tier 1: Deterministic Exact & Alias Cache
- **Mechanism**: Maintains a normalized token inverted index and alias mapping table (`raw_alias -> canonical_node_id`).
- **Behavior**: If a user previously corrected `"Carlos V"` to `["Food", "Confectionery", "Chocolate"]`, this alias is stored. Any future variant matching normalized `"carlos v"` resolves deterministically in $< 1\text{ms}$ with zero LLM API calls.

### 1.2 Tier 2: Hybrid Distance (Dense + Sparse + Numerical)
- **Mechanism**: Calculates composite similarity $S_{\text{total}} = w_{\text{vec}} S_{\text{vec}} + w_{\text{num}} S_{\text{num}}$. Vectors are generated via `trustformers` / `oxirag`.
- **Behavior**: If similarity exceeds the high-confidence threshold ($\tau \ge 0.85$), the system attaches the entity directly.

### 1.3 Tier 3: Constrained LLM Inference with Candidate Subgraph
- **Mechanism**: If similarity is ambiguous (e.g., $0.60 \le S < 0.85$), the query engine pulls the top 3 candidate anchor subtrees from the graph and feeds them as prompt context into `oxibonsai`.
- **Behavior**: The model outputs an `ExtractionDirective::AssignExisting` or `ExtractionDirective::InsertSubtree`.

### 1.4 Tier 4: Human Feedback & Override Loop
- **Mechanism**: If the LLM returns `ExtractionDirective::Unknown`, the record is queued for human audit. When an operator assigns the category, the decision is written back to Tier 1, permanently closing the gap.

---

## 2. In-Memory Graph Diffing Engine
Before issuing database write queries, proposed changes are compared against the in-memory `petgraph` representation:
1. **Node Equality Check**: Compares `NodePayload::node_key()`.
2. **Changeset Calculation**:
   - **Unchanged Nodes**: Skipped.
   - **Updated Attributes**: Placed in `Vec<NodeUpdate>`.
   - **Novel Nodes**: Placed in `Vec<NodeInsert>`.
   - **New Directed Edges**: Placed in `Vec<EdgeInsert>`.

This ensures database operations are consolidated into minimal atomic batch statements.

---

## 3. Strict Cycle Detection & Acyclic Guardrails

### 3.1 The Risk of Graph Cycles
In a Directed Acyclic Graph, introducing a circular relationship ($A \to B \to C \to A$) causes catastrophic failures:
- Recursive SQL CTEs (`WITH RECURSIVE`) will loop infinitely until query timeout or server memory exhaustion.
- Downstream topological sorting algorithms panic or deadlock.

### 3.2 Detection Algorithm
Before applying any mutation plan to the active graph:
1. A shadow graph with candidate edges is evaluated in memory.
2. `embedag` executes cycle detection via `petgraph::algo::is_cyclic_directed` or Tarjan’s Strongly Connected Components (SCC):
   ```rust
   pub fn verify_dag_invariance<N, E>(graph: &petgraph::graph::DiGraph<N, E>) -> Result<(), GraphError> {
       if petgraph::algo::is_cyclic_directed(graph) {
           return Err(GraphError::CycleDetected("Proposed mutation violates acyclic DAG constraint".into()));
       }
       Ok(())
   }
   ```
3. If a cycle is detected, the transaction is rejected, and the offending edge is flagged.

---

## 4. Confidence & Provenance Tagging (Graphify Paradigm)
Every edge in `embedag` carries a confidence score and provenance origin:

| Provenance Category | Confidence Score | Source Description |
|---|---|---|
| `AlgorithmicExtracted` | `1.0` | Exact alias match, AST parser, deterministic regex, H3 containment. |
| `UserOverride` | `1.0` | Explicit human curation / verified business taxonomy mapping. |
| `HybridSimilarity` | `0.85 .. 0.99` | High-confidence hybrid mathematical distance match. |
| `LlmInferred` | `0.40 .. 0.84` | Probabilistic relationship inferred via LLM (`oxibonsai`). |

Downstream RAG retrieval pipelines can filter or weight evidence based on minimum confidence thresholds (e.g. `WHERE confidence >= 0.8`), ensuring safety-critical pipelines do not ingest unverified inferences.
