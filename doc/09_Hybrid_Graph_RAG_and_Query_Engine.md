# Hybrid Graph-RAG & Query Engine Architecture (Second-Pass Specification)

## 1. Overview & Problem Definition
Conventional Retrieval-Augmented Generation (RAG) pipelines rely exclusively on flat vector similarity. However, flat vector search suffers from two critical flaws in structured domains:
1. **Structural Blindness**: Vector similarity cannot understand hierarchical relationships (e.g., retrieving all variants under a parent brand, or finding upstream supplier dependencies).
2. **Typo & Out-of-Vocabulary Vulnerability**: Dense embedding models break when proper nouns are misspelled (e.g., `"Carols V"` instead of `"Carlos V"`), because subword tokenizers segment typos into unrelated tokens.

To resolve these limitations, `embedag` provides a **Two-Stage Hybrid Graph-RAG Query Engine** combining:
- **Topological Graph Traversal** (via Recursive CTEs / `petgraph` & `oxirs` paradigms)
- **Dense Vector Search** (Priority 1: `trustformers` / `oxirag` / `pgvector`)
- **Sparse Keyword Search** (via PostgreSQL `tsvector` / SQLite `FTS5` with BM25)
- **Reciprocal Rank Fusion (RRF)**
- **Topological Distance Decay Scoring**
- **In-Memory Composite Re-ranking** (Gower + Angular + Spatial)

---

## 2. End-to-End Query Lifecycle

```
                     Incoming User Query
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 1. Query Analysis & Intent Decomposition                    │
│    - Extract Entity Keywords ("Carlos V")                   │
│    - Extract Scalar Filters (price < $5, weight < 100g)     │
│    - Extract Spatial Constraints (within 10km of Warehouse) │
│    - Generate Dense Embedding via trustformers              │
└─────────────────────────────┬───────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 2. Stage 1: Candidate Generation & Reciprocal Rank Fusion   │
│                                                             │
│   ┌──────────────────┐               ┌──────────────────┐   │
│   │   Dense Search   │               │   Sparse Search  │   │
│   │(trustformers/vec)│               │ (tsvector/FTS5)  │   │
│   └────────┬─────────┘               └────────┬─────────┘   │
│            │                                  │             │
│            └────────────────┬─────────────────┘             │
│                             ▼                               │
│                   Reciprocal Rank Fusion                    │
│             Score = 1/(k + R_dense) + 1/(k + R_sparse)      │
└─────────────────────────────┬───────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 3. Stage 2: Topological Subgraph Expansion & Decay          │
│                                                             │
│  - Seed Anchors Identified (Top-K Candidates)               │
│  - Traverse Ancestors & Descendants via Recursive CTE       │
│  - Compute Topological Distance Decay:                      │
│      Score(node) = RRF(seed) * γ^hops (γ = 0.85)            │
│  - Exact Composite Re-ranking (Gower + Angular + Geo)       │
└─────────────────────────────┬───────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 4. Grounded Context Packaging & Synthesis                   │
│   - Token-budgeted Markdown Context                         │
│   - Explicit Provenance Tags [EXTRACTED 1.0 / INFERRED 0.75]│
│   - Passed to oxibonsai LLM with Citation Anchors           │
└─────────────────────────────────────────────────────────────┘
```

---

## 3. Typo-Resilience via Reciprocal Rank Fusion (RRF)
To ensure robust retrieval even when users misspell proper nouns or technical terms:
1. **Sparse Lexical Query**: Searches full-text indices (`tsvector` on Postgres, `FTS5` on SQLite) for substring, prefix, and trigram matches via BM25.
2. **Dense Semantic Query**: Searches embedding vectors for conceptual proximity.
3. **RRF Aggregation**:
   $$Score_{\text{RRF}}(d) = \frac{1}{60 + \text{Rank}_{\text{dense}}(d)} + \frac{1}{60 + \text{Rank}_{\text{sparse}}(d)}$$

Where $k = 60$ is the smoothing hyperparameter preventing individual top ranks from skewing fused scores. An item appearing near the top of either list or moderately high on both is guaranteed to surface in the top candidates.

---

## 4. Stage 2: Topological Subgraph Expansion & Distance Decay
Once seed nodes are retrieved via RRF, flat chunks are expanded into **grounded subgraphs**:
1. **Directional Traversal**:
   - **Upstream (Ancestors)**: Traverses inbound edges to identify parent categories, brands, and master assemblies.
   - **Downstream (Descendants)**: Traverses outbound edges to identify child variants, packaging, and components.
2. **Topological Distance Decay Scoring**:
   Evidence originating further from the primary seed node has its relevance score decayed exponentially:
   $$Score_{\text{hop}}(N) = Score_{\text{RRF}}(\text{Seed}) \times \gamma^{h}$$
   Where $\gamma = 0.85$ is the decay discount factor and $h$ is the shortest path edge distance between the seed and node $N$. This guarantees structural relevance while penalizing distant, noisy branches.

---

## 5. Structured Query Planner
When a user submits a multi-faceted natural language query (e.g. *"Show all variants of Carlos V with weight under 100g sold in downtown"*), the `embedag-rag` query planner converts the request into structured execution primitives:
1. **Spatial Filter**: Bounding box / H3 grid ring around downtown coordinates (`h3o`).
2. **Graph Traversal**: Subtree query anchored on `"Carlos V"` (`max_depth <= 3`).
3. **Scalar Predicate**: Hard SQL/arena filter: `weight_grams < 100.0`.
4. **Vector Match**: Semantic prompt match across descriptive notes.

This transforms probabilistic requests into deterministic database filters, preventing hallucinations.

---

## 6. Grounded Evidence Packaging & Citation Generation
Before prompt delivery, retrieved subgraphs are formatted into a token-budgeted Markdown package with explicit confidence annotations:

```markdown
### Grounded Evidence Context (Target: Carlos V)
- [Node: Carlos V (Brand) | ID: e12a4b] (Confidence: 1.0, Provenance: EXTRACTED)
  ├── Edge: [has_variant] (Confidence: 1.0)
  │   └── [Node: Carlos V 50gr (Variant) | ID: f78c9d] (Weight: 50g, Price: $1.20)
  └── Edge: [has_variant] (Confidence: 0.95, Provenance: INFERRED)
      └── [Node: Carlos V 12-Pack (Package) | ID: a43b1c] (Count: 12 pcs, Total: 600g)
```

The generative model (`oxibonsai`) is instructed to append citation tags (e.g., `[ID: f78c9d]`), allowing downstream user interfaces to render direct links to verified graph entities.
