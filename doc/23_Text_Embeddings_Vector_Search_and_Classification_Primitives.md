# Text Embeddings, Vector Search & Classification Primitives

## 1. Purpose

This document specifies the capabilities that move embedag's waterfall from caller-supplied scores to working classification:
- A real text `Embedder`.
- Relational vector search.
- Fuzzy descriptor normalization.
- kNN categorization that learns from corrections.
- Periodic-series detection.
- Hierarchies that share one table, separated by a discriminator column.
- Incremental Merkle trees for replica anti-entropy.

## 2. Assessment of the Current Implementation

1. **Text embeddings exist only in the spec.** `08_Neural_and_Multimodal_Embeddings.md` specifies pure-Rust `trustformers` / `oxirag` embeddings, but no `Embedder` exists. "Embeddings" today means the hand-built feature encoders, so callers have to supply their own vectors.
2. **Inference is simulated.** The LLM providers (`OxillamaProvider`, `CandleProvider`, `OxibonsaiProvider`) choose directives with heuristics (`embedag-llm/src/provider.rs:337`), and `CloudProvider` has no transport.
3. **`VectorSearchRepository` has no Diesel implementation.** `embedag-diesel` only runs `CREATE EXTENSION vector`, and there is no SQLite vector path.
4. **`AliasStore::normalize_token` only matches exactly.** It has no trigram or Jaro-Winkler scoring and no cleaning of bank- or processor-style descriptors.
5. **The `HierarchicalModel` derive is table-per-hierarchy and hard-codes `ON DELETE CASCADE`** in the generated DDL.
6. **`merkle.rs` rebuilds the whole tree on every change.** It has no incremental leaf updates and no prefix-subtree digests.
7. **Defaults are heavy.** The facade defaults to `full` (SurrealDB, Neo4j, OxiRS, libsql, pgvector), and `embedag-diesel` defaults to `test-harness`.

## 3. Requirements (Milestone 10: Text Embeddings, Vector Search & Classification Primitives)

| Ticket | Priority | Points | Crate | Requirement |
|---|---|---|---|---|
| `10.1.1` | high | 5 | `embedag-core` | Embedder trait + real text embeddings per spec 08 (trustformers/oxirag pure-Rust; fastembed/ONNX optional) |
| `10.1.2` | high | 5 | `embedag-diesel` | Diesel VectorSearchRepository for pgvector and SQLite (BLOB brute-force / sqlite-vec) |
| `10.2.1` | high | 5 | `embedag-llm` | Fuzzy merchant-descriptor normalization (bank descriptor cleaning + trigram/Jaro-Winkler) |
| `10.3.1` | high | 3 | `embedag-llm` | Real HTTP LlmProvider (Anthropic / OpenAI / Ollama-compatible) for Tier-3 |
| `10.4.1` | high | 3 | `embedag-derive` | HierarchicalModel: discriminator-scoped hierarchies + configurable ON DELETE |
| `10.2.2` | medium | 5 | `embedag-llm` | kNN categorizer over labelled history producing CandidateMatch |
| `10.2.3` | medium | 5 | `embedag-core` | Recurring/subscription series detection + amount-drift alerts |
| `10.3.2` | medium | 1 | `embedag` | Lean default features (test-harness and full off by default) |
| `10.4.2` | medium | 5 | `embedag-example-receipts` | Brand hierarchy + trait facet with inheritance in the finance categorization engine |
| `10.5.1` | medium | 3 | `embedag-core` | Incremental Merkle trees with prefix-subtree digests for replica anti-entropy (builds on 9.3.1 wire format) |

## 4. Specifications

### 10.1.1: Embedder trait + real text embeddings per spec 08 (trustformers/oxirag pure-Rust; fastembed/ONNX optional)

`doc/08_Neural_and_Multimodal_Embeddings.md` specifies pure-Rust transformer embeddings (`trustformers` / `oxirag`), with ONNX via `fastembed-rs` only as an optional secondary backend. The code has no text-embedding model, so callers must supply their own vectors.

**Request:**
- A trait `Embedder { fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>; fn dim(); fn model_id(); }`.
- A pure-Rust multilingual small model behind `embed-local` (the data is Spanish and English merchant and item names).
- Optional `embed-onnx` and `embed-http` implementations.

**Acceptance criteria:** deterministic output, with a recorded `model_id` and `dim` so they can be stored.

### 10.1.2: Diesel VectorSearchRepository for pgvector and SQLite (BLOB brute-force / sqlite-vec)

Only SurrealDB, Neo4j and OxiRS implement `VectorSearchRepository`. Diesel only runs `CREATE EXTENSION vector`.

**Request:** implement `find_nearest_neighbors` for:
- Postgres pgvector (HNSW index).
- SQLite: f32 BLOB with brute-force cosine, plus an optional `sqlite-vec` feature.

Both must be scoped by a tenant column such as `workspace_id`.

**Acceptance criteria:** the same results on both engines for a 10k-vector fixture.

### 10.2.1: Fuzzy merchant-descriptor normalization (bank descriptor cleaning + trigram/Jaro-Winkler)

`AliasStore::normalize_token` only matches exactly.

**Request:**
- A descriptor-cleaning pipeline for Mexican bank strings that strips branch numbers, city codes, card suffixes and processor prefixes (`OXXO SUC 1234 MTY` → `oxxo`; `PAYPAL *SPOTIFY` → `spotify`).
- Trigram and Jaro-Winkler candidate scoring that produces `CandidateMatch` for the waterfall.

**Acceptance criteria:** at least 95% top-1 accuracy on a labelled descriptor fixture.

### 10.3.1: Real HTTP LlmProvider (Anthropic / OpenAI / Ollama-compatible) for Tier-3

`CloudProvider` exists only as a struct, and the local providers return heuristics (`provider.rs:337`).

**Request:** a real HTTP provider behind a feature flag with JSON-schema-constrained output, timeouts, and usage reporting. It must not require `tokio/full`.

**Acceptance criteria:** the waterfall Tier-3 resolves on a recorded fixture.

### 10.4.1: HierarchicalModel: discriminator-scoped hierarchies + configurable ON DELETE

Consumers often keep several hierarchies in a single node table, separated by a discriminator column (for example `facet IN ('natural','trait','purchase')`), with a composite parent FK `(parent_id, facet)` so a node's parent always has the same facet. Separately, the generated DDL hard-codes `ON DELETE CASCADE`.

**Request:**
- Add `#[hierarchical(discriminator = "facet", value = "natural")]` so the derived CRUD, CTE traversal and candidate loading filter by the discriminator and stamp it on insert.
- Add `on_delete = "restrict"|"set_null"|"cascade"` for the parent FK, defaulting to `restrict`.

**Acceptance criteria:**
- Two `HierarchicalModel` structs sharing one table don't see each other's nodes.
- The generated DDL honours `on_delete`.

### 10.2.2: kNN categorizer over labelled history producing CandidateMatch

The waterfall expects callers to compute `CandidateMatch` scores.

**Request:** a `KnnCategorizer` that:
- Combines the composite distance (amount `encode_log`, `CyclicalTimeEncoder`, h3 location, text embedding) over labelled history.
- Returns the top-k labels with calibrated confidence.
- Learns online from corrections.
- Supports **scoped models**: a shared model per tenant (for example the natural category of an item) plus per-actor overlays (labels that are personal to one user), so one actor's corrections never leak into another's predictions.

**Acceptance criteria:**
- It beats a rules-only baseline on a fixture ledger.
- Corrections from actor A don't change predictions for actor B, except through the shared scope.

### 10.2.3: Recurring/subscription series detection + amount-drift alerts

**Request:** given time-ordered (merchant, amount, date) events, detect periodic series: weekly, biweekly, monthly, annual, with tolerance windows. Return each series with its next expected date and flag amount drift (price increases) and missed occurrences.

**Acceptance criteria:** detects Netflix, Spotify and rent patterns in a fixture ledger with fewer than 5% false positives.

### 10.3.2: Lean default features (test-harness and full off by default)

The facade crate defaults to `full`, which pulls in SurrealDB, Neo4j, OxiRS, libsql and pgvector. `embedag-diesel` defaults to `test-harness` (Testcontainers).

**Request:** an empty default, or `core` only.

**Acceptance criteria:** `embedag = { default-features = false, features = ["core","derive","diesel"] }` builds for iOS and Android.

### 10.4.2: Brand hierarchy + trait facet with inheritance in the finance categorization engine

`FinanceCategorizationEngine` models item, purchase, invoice and merchant categories, but has no brands. Luxury is inferred only from the store or the total.

**Request:**
- A brand tree (for example Nestlé → Carlos V).
- A `trait` hierarchy (Luxury, Imported, Organic…) that can attach to brands, merchants and items, with inheritance down the brand tree.
- `CategorizedInvoice` should expose per-item `natural` (exactly one), `traits` (with origin item/brand/merchant), `purchase` and `invoice` assignments, each carrying a source and a confidence.
- Item resolution from OCR text should also return the brand ("CARLOS V 20G" → item Carlos V, brand Carlos V ⊂ Nestlé).

**Acceptance criteria:** a chocolate bar bought as a birthday gift yields natural Food > Dessert > Chocolate, the brand's traits, and purchase Gift > Birthday Gift.

### 10.5.1: Incremental Merkle trees with prefix-subtree digests for replica anti-entropy (builds on 9.3.1 wire format)

`merkle.rs` (`MerkleNode::from_paths`, `diff`) rebuilds the whole tree on every change. Replica anti-entropy (a client replica and a server comparing keyed row sets) needs more than that.

**Request:**
- (a) `upsert_leaf` / `remove_leaf` that update the tree incrementally.
- (b) Digests per key prefix (`table/`, `table/row_prefix`) so that peers can descend level by level.
- (c) A deterministic fan-out, so independently built trees are identical.
- (d) Reuse the canonical wire format from Ticket 9.3.1 for digest frames.

**Acceptance criteria:** with 100k leaves and 10 differing rows, peers identify the 10 rows by exchanging fewer than 2% of the leaf digests.

## 5. Cross-Cutting Constraints

- **Toolchain:** pure Rust and zero C FFI on the default path. Native runtimes go behind opt-in features only.
- **Async:** no `tokio/full` on library crates. Use a minimal runtime, and keep HTTP clients on rustls.
- **Builds:** default features stay lean enough to build for `aarch64-apple-ios`, `aarch64-linux-android` and `wasm32-unknown-unknown` (where applicable).
- **Determinism:** every model-backed output records the engine, model id and version, and a confidence score, so consumers can store provenance.
