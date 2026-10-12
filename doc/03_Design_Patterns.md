# Architectural & Rust Design Patterns

## 1. Executive Summary
This document defines the core software architecture and idiomatic Rust design patterns governing `embedag`. The selection of these patterns directly resolves the complex trade-offs identified in our research:
- Preventing borrow-checker deadlocks during graph mutations.
- Eliminating LLM hallucinations and runtime syntax failures.
- Preserving high cache locality and multi-threaded throughput.
- Enforcing strict separation between deterministic business rules and probabilistic model inferences.
- Standardizing on **zero-FFI pure Rust execution** via the **KitaSan stack (`oxibonsai`, `trustformers`)** as Priority 1.

---

## 2. Core Architectural & System Patterns

```mermaid
flowchart TD
    subgraph "Core Domain (embedag-core)"
        PortRepo[Port: GraphRepository]
        PortClass[Port: GraphClassifier]
        Arena[In-Memory DAG Arena (petgraph)]
        Typestate[Typestate: Unverified -> Validated -> Committed]
    end

    subgraph "Adapters (Infrastructure)"
        DieselPg[Adapter: Diesel Async Postgres]
        DieselSqlite[Adapter: Diesel SQLite]
        OxiEngine["Adapter: oxibonsai (Priority 1 Pure Rust)"]
        CloudEngine[Adapter: OpenAI / Anthropic]
    end

    PortRepo <|.. DieselPg
    PortRepo <|.. DieselSqlite
    PortClass <|.. OxiEngine
    PortClass <|.. CloudEngine
```

### 2.1 Hexagonal Architecture (Ports & Adapters)
- **Problem**: Monolithic Rust crates couple prompt serialization with database drivers, creating brittle builds where updating a database crate breaks inference logic.
- **Pattern**: Core domain logic lives in `embedag-core`, defining explicit asynchronous **Ports**:
  - `GraphRepository`: Persistence port.
  - `GraphClassifier`: Inference transformation port.
  - `AsGraphContext`: Ingestion port.
- **Adapters**: `embedag-diesel` and `embedag-llm` act as infrastructure plugins implementing these ports. The core crate depends on zero network drivers or SQL engines.

---

### 2.2 Index-Based Generational Arena Pattern (`petgraph`)
- **Problem**: Traditional object-oriented graphs rely on `Rc<RefCell<Node>>` or `Arc<RwLock<Node>>`. In Rust, this causes severe heap fragmentation, runtime borrow check panics, pointer chasing cache misses, and thread contention.
- **Pattern**: Nodes and directed edges are stored in flat, contiguous memory vectors indexed by lightweight 32-bit integers (`NodeIndex`, `EdgeIndex`).
- **Benefits**:
  - Bypasses borrow-checker aliasing conflicts.
  - Contiguous memory buffers trigger hardware CPU prefetching during graph traversals.
  - Arena implements `Send` and `Sync`, unlocking parallel processing via `rayon`.

---

### 2.3 Typestate Pattern for Graph Mutations
- **Problem**: Committing an unverified graph mutation plan to the database risks injecting circular references ($A \to B \to C \to A$), crashing recursive CTE queries.
- **Pattern**: Leverage zero-sized types (ZST) to encode validation states at compile time:

```rust
pub struct Unverified;
pub struct CycleValidated;
pub struct Committed;

pub struct MutationPlan<T, State = Unverified> {
    directives: Vec<MutationDirective<T>>,
    _state: std::marker::PhantomData<State>,
}

impl<T: NodePayload> MutationPlan<T, Unverified> {
    pub fn new(directives: Vec<MutationDirective<T>>) -> Self {
        Self { directives, _state: std::marker::PhantomData }
    }

    /// Validates DAG acyclic invariant against the active arena.
    pub fn validate_cycles(self, arena: &MemoryDag<T>) -> Result<MutationPlan<T, CycleValidated>, DagError> {
        arena.simulate_and_check(&self.directives)?;
        Ok(MutationPlan { directives: self.directives, _state: std::marker::PhantomData })
    }
}

impl<T: NodePayload> MutationPlan<T, CycleValidated> {
    /// Only a validated plan can be committed to the repository!
    pub async fn commit<R: GraphRepository>(self, repo: &R) -> Result<MutationPlan<T, Committed>, RepositoryError> {
        repo.apply_plan(&self.directives).await?;
        Ok(MutationPlan { directives: self.directives, _state: std::marker::PhantomData })
    }
}
```
Attempting to call `.commit()` on an unverified plan fails at compile time with a compiler error, completely eliminating runtime graph corruption bugs.

---

### 2.4 Multi-Tiered Waterfall Resolution Pattern
- **Problem**: Calling LLMs for every incoming record is slow, expensive, and fails to reuse prior human curation.
- **Pattern**: A 4-stage waterfall:
  1. **Tier 1 (Deterministic Cache / Alias Store)**: Exact normalized token lookup ($< 1\text{ms}$, 0 tokens).
  2. **Tier 2 (Hybrid Distance Matching)**: Composite text + scalar + spatial similarity ($< 5\text{ms}$, 0 LLM tokens).
  3. **Tier 3 (Constrained Local LLM Inference)**: Structured decoding via `oxibonsai` with top-3 candidate anchor subgraphs.
  4. **Tier 4 (Human-in-the-Loop Feedback Loop)**: Routing unresolvable records to an `Unknown` queue, writing human fixes back into Tier 1.

---

### 2.5 Draft-Then-Decode Pattern (Cognitive Scratchpad)
- **Problem**: Strict token-level logit masking (GBNF/JSON Schema) introduces an "alignment tax"—forcing the model to output strict enum tags before it has generated reasoning context degrades classification accuracy.
- **Pattern**: The extraction schema places a free-form `chain_of_thought` string as the first field. The model streams intermediate thoughts into this scratchpad before populating the strict structural payload fields.

---

### 2.6 Dual-Pass Extraction & Confidence Tagging (Graphify Paradigm)
- **Problem**: Blending algorithmic facts with LLM guesses without distinction makes downstream RAG queries brittle.
- **Pattern**: Every node and edge carries an explicit provenance tag:
  - `EXTRACTED` (Confidence = 1.0): Verified algorithmic facts (regex, AST, SI units, H3 grid).
  - `INFERRED` (Confidence = 0.40 .. 0.95): Probabilistic LLM classifications.
  - `UserOverride` (Confidence = 1.0): Explicit human curation.

---

### 2.7 Composite Hybrid Distance Metric (Gower + Angular + Spatial)
- **Problem**: Dense embeddings degrade on pure numbers (e.g. $50g$ vs $500g$).
- **Pattern**: Combine text vectors, range-normalized continuous metrics, and spatial grid distances into a unified, bounded metric:
  $$D_{\text{total}}(A, B) = w_{\text{vec}} \cdot D_{\text{angular}}(A, B) + w_{\text{num}} \cdot D_{\text{scalar}}(A, B) + w_{\text{geo}} \cdot D_{\text{h3}}(A, B)$$

---

### 2.8 Two-Stage Retrieval with Reciprocal Rank Fusion (RRF)
- **Problem**: Dense embedding models fail on misspelled proper nouns (e.g. `"Carols V"` vs `"Carlos V"`).
- **Pattern**:
  - Run dense vector search (`pgvector`/`sqlite-vec`) in parallel with sparse full-text search (`tsvector`/`FTS5`).
  - Merge rankings via Reciprocal Rank Fusion:
    $$Score_{\text{RRF}}(d) = \frac{1}{60 + \text{Rank}_{\text{dense}}(d)} + \frac{1}{60 + \text{Rank}_{\text{sparse}}(d)}$$
  - Expand top seeds into k-hop subgraphs via recursive CTEs.

---

### 2.9 Cycle-Guarded Recursive CTE Visitor Pattern
- **Problem**: Recursive SQL CTEs (`WITH RECURSIVE`) loop infinitely if cycles exist.
- **Pattern**: Implement typed recursive query builders using `diesel-cte-ext` with explicit path array tracking (`WHERE NOT (child_id = ANY(visited_path))`) in PostgreSQL and path string matching in SQLite.

---

## 3. Code Quality & Complexity Guardrails (per `pattern-complexity-auditor`)
1. **Error Handling**: 100% typed errors via `thiserror`. Zero `.unwrap()` or `.expect()` in non-test crate code.
2. **Cognitive & Cyclomatic Limits**: Functions strictly bound to Cyclomatic Complexity $\le 15$ and Cognitive Complexity $\le 20$.
3. **Zero-Copy & Smart Pointers**: Avoid `.clone()` spam; pass borrowed slices `&[T]` and `&str`. Reference graph nodes via 32-bit `NodeIndex` values that implement `Copy`.
