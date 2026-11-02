# embedag-rag

`embedag-rag` is the hybrid Graph-RAG retrieval, topological subgraph expansion, and community summarization crate for the `embedag` workspace. It solves the problem of combining dense vector similarity search, sparse lexical keyword matching, and `MemoryDag` graph structure into ranked, provenance-annotated context windows without relying on external vector databases or LLM calls during retrieval.

## When to use it

Use `embedag-rag` when you have a `MemoryDag<T>` and want to:

- Fuse ranked candidate lists from dense vector search and sparse lexical/BM25 search using parameter-free Reciprocal Rank Fusion (`ReciprocalRankFusion`).
- Index nodes in memory for exact cosine similarity and term-overlap lexical search (`InMemoryHybridIndex`) or execute a complete two-stage hybrid retrieval + graph expansion query (`HybridRagEngine`).
- Expand top-K seed nodes along directed graph edges (ancestors and descendants) with exponential hop-distance decay and edge-confidence weighting (`SubgraphExpander`).
- Partition a `MemoryDag<T>` into modular communities via Leiden modularity optimization and generate deterministic community summaries (`LeidenClusterer`).
- Format retrieved nodes and their ancestor lineages into token-budgeted markdown context strings (`EvidenceSynthesizer`) or run the unified 4-layer `oxirag` verification pipeline (`OxiRagUnifiedEngine`).

Do not use `embedag-rag` as a durable storage engine; persist graphs using [`embedag-diesel`](../embedag-diesel/README.md), [`embedag-oxirs`](../embedag-oxirs/README.md), [`embedag-surrealdb`](../embedag-surrealdb/README.md), or [`embedag-neo4j`](../embedag-neo4j/README.md) and load or project the relevant subgraph into `MemoryDag<T>` for retrieval.

## Main types and functions

- **Reciprocal Rank Fusion (`rrf`)**:
  - `RrfRanker` and `RrfCandidate`: Computes `score(d) = sum_{r in R} 1 / (k + rank_r(d))` with default smoothing constant `smoothing_k = 60.0` (`RrfRanker::new(k)`, `fuse(&self, dense_results: &[Uuid], sparse_results: &[Uuid]) -> Vec<RrfCandidate>`, `fuse_multi`). Ties are broken deterministically by UUID order.
- **In-memory dense and sparse lexical indexes (`index`)**:
  - `DenseVectorIndex`: Stores dense `Vec<f32>` embeddings per `Uuid` and queries top-K nearest neighbors by cosine similarity (`with_dimensions`, `insert`, `search`).
  - `SparseLexicalIndex` and `LexicalDocument`: Indexes `business_key` and `raw_text` using lowercase token overlap and character trigrams for typo-resilient search (`new`, `insert`, `search`).
- **Topological subgraph expansion (`expansion`)**:
  - `SubgraphExpander`: Traverses up to `max_hops` ancestors, descendants, and optional 1-hop siblings from seed nodes with exponential distance decay (`decay_gamma.powi(hop_distance)`), returning `ExpandedSubgraph<T::Key>` (`seeds`, `nodes: HashMap<K, ExpandedNode<K>>`, `edges: Vec<ExpandedEdge<K>>`).
- **Community detection (`community`)**:
  - `CommunityDetector` and `CommunityPartition<K>`: Partitions a `MemoryDag<T>` into topologically connected communities (`detect`, `community_count`, `community_of`, `members`).
- **Evidence packaging and OxiRAG verification (`evidence`, `oxirag_engine`)**:
  - `GroundedEvidencePackager`: Formats an `ExpandedSubgraph` into a markdown context block with provenance tags (`EXTRACTED (1.0)` vs `INFERRED (< 1.0)`) and extracts `[Node: KEY]` citations (`package`, `extract_citations`).
  - `OxiRagEngine`: Runs the 4-layer `oxirag` Echo/Speculator/Judge/Graph verification pipeline (`query_verified`, `query_graph_verified`).

### Example

```rust
use embedag_core::dag::MemoryDag;
use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_rag::evidence::GroundedEvidencePackager;
use embedag_rag::expansion::SubgraphExpander;
use embedag_rag::index::{DenseVectorIndex, SparseLexicalIndex};
use embedag_rag::rrf::RrfRanker;
use uuid::Uuid;

#[derive(Debug, Clone)]
struct DocNode { id: String, title: String }
embedag_core::impl_node_payload!(DocNode, key = |n| &n.id, name = |n| n.title.as_str(), level = |_| "doc");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut dag = MemoryDag::new();
    dag.upsert_node(DocNode { id: "arch".into(), title: "System Architecture".into() });
    dag.upsert_node(DocNode { id: "rag".into(), title: "Hybrid GraphRAG Retrieval".into() });
    dag.add_edge_by_keys(&"arch".into(), &"rag".into(), EdgePayload::new("CONTAINS", 1.0, ProvenanceKind::DeterministicRule)?)?;

    let (arch_id, rag_id) = (Uuid::new_v4(), Uuid::new_v4());
    let mut dense = DenseVectorIndex::with_dimensions(2);
    dense.insert(arch_id, vec![0.8, 0.2])?;
    dense.insert(rag_id, vec![0.1, 0.9])?;

    let mut sparse = SparseLexicalIndex::new();
    sparse.insert(arch_id, "arch", "system architecture overview");
    sparse.insert(rag_id, "rag", "hybrid graphrag retrieval rrf");

    let dense_ids: Vec<_> = dense.search(&[0.1, 0.9], 2)?.into_iter().map(|(id, _)| id).collect();
    let sparse_ids: Vec<_> = sparse.search("graphrag rrf", 2).into_iter().map(|(id, _)| id).collect();
    let fused = RrfRanker::default().fuse(&dense_ids, &sparse_ids);
    assert_eq!(fused[0].id, rag_id);

    let subgraph = SubgraphExpander::default().expand(&dag, &[("rag".to_string(), fused[0].score)]);
    let context = GroundedEvidencePackager::new().package(&dag, &subgraph);
    assert!(context.contains("[Node: rag]") && context.contains("[Node: arch]"));
    Ok(())
}
```

## Feature flags

`embedag-rag` defines no Cargo feature flags.

## Workspace and Dendrite relationship

Within the workspace:

- `embedag-rag` depends on [`embedag-core`](../embedag-core/README.md) (`MemoryDag`, `NodePayload`, `EdgePayload`, `rank_by_cosine`) and `oxirag`.
- [`embedag`](../embedag/README.md) re-exports `embedag-rag` under `embedag::rag` and `embedag::prelude`.

Across the Dendrite repositories:

- `dendrite-rag-rs` directly consumes `embedag-rag` (`ReciprocalRankFusion`, `SubgraphExpander`, `LeidenClusterer`, `EvidenceSynthesizer`, `HybridRagEngine`) for hybrid vector + lexical code/document retrieval and graph-expanded context assembly.
- The Dendrite control plane uses `embedag-rag` to assemble provenance-backed context windows over ticket and architecture graphs.

## Build and test

```bash
cargo build -p embedag-rag
cargo test -p embedag-rag
```

## Specifications

- [`doc/17_Hybrid_Vector_and_Lexical_Retrieval.md`](../../doc/17_Hybrid_Vector_and_Lexical_Retrieval.md) — `ReciprocalRankFusion` and `InMemoryHybridIndex`.
- [`doc/18_Topological_Subgraph_Expansion_and_GraphRAG.md`](../../doc/18_Topological_Subgraph_Expansion_and_GraphRAG.md) — `SubgraphExpander`, `LeidenClusterer`, `EvidenceSynthesizer`, and `OxiRagUnifiedEngine`.
- [`doc/19_External_Ecosystem_and_KitaSan_Stack.md`](../../doc/19_External_Ecosystem_and_KitaSan_Stack.md) — Integration with `oxirag`.
