# embedag

`embedag` is the top-level facade crate for the `embedag` workspace. It re-exports the core in-memory DAG arena, procedural derive macros, structural AST matcher, constrained LLM extraction pipeline, hybrid Graph-RAG retrieval engine, and storage adapters (Diesel relational persistence, OxiRS RDF/SPARQL, SurrealDB, and Neo4j) behind a single dependency entry and a unified `embedag::prelude` module. It is intended for application developers who want to assemble multi-modal knowledge graphs, hierarchical taxonomies, and dependency graphs without managing version alignment across multiple sub-crates.

## When to use it

Use `embedag` when your application combines multiple capabilities from the workspace—such as deriving `AsGraphContext` on domain structs, validating acyclic graph mutations in `MemoryDag`, persisting hierarchies via `embedag-diesel`, and running hybrid retrieval with `embedag-rag`.

Do not use `embedag` with its default `full` feature set when you only need a single layer of the stack or want to minimize compile times and transitive dependencies (such as `surrealdb`, `neo4rs`, `oxirs-core`, or `candle-core`). Library crates and lightweight services should depend directly on [`embedag-core`](../embedag-core/README.md), [`embedag-matcher`](../embedag-matcher/README.md), or the specific adapter crate they require, or disable default features on `embedag` (`default-features = false`) and opt into individual feature flags.

## Main types and functions

When features are enabled, `embedag` re-exports each sub-crate as a module (`embedag::core`, `embedag::dag`, `embedag::derive`, `embedag::matcher`, `embedag::llm`, `embedag::persistence`, `embedag::rag`, `embedag::oxirs`, `embedag::surrealdb_backend`, `embedag::neo4j_backend`) and provides `embedag::prelude::*` with the primary types from each enabled module:

- From `core`: `MemoryDag`, `DagError`, `EdgePayload`, `ProvenanceKind`, `GraphTarget`, `MultimodalBlob`, `NodePayload`, `AsGraphContext`, `MutationPlan`, `MutationDirective`, `Unverified`, `CycleValidated`, `Committed`, `Condensation`.
- From `derive`: `AsGraphContext`, `GraphTarget`, `HierarchicalModel` (via `embedag::derive`).
- From `matcher`: `AstMatcher`, `Matcher`, `SimilarityGraph`, `Tree` (via `embedag::matcher`).
- From `llm`: `LlmProvider`, `OxillamaProvider`, `OxibonsaiProvider`, `ExtractionDirective`, `ExtractionEnvelope`, `EntityExtractor`, `EntityExtractorConfig`, `EntityProperties`, `LlmError`, `ModelMetadata`.
- From `persistence` (`embedag-diesel`): `HierarchicalTableConfig`, `HierarchyItem`, `CteHierarchyBuilder`, `UpsertDescriptor`, `ConflictStrategy`, `run_sqlite_migration`, `run_postgres_migration`.
- From `rag`: `GroundedEvidencePackager`, `ExpandedNode`, `ExpandedSubgraph`, `SeedRelationship`, `SubgraphExpander`, `DenseVectorIndex`, `SparseLexicalIndex`, `OxiRagEngine`, `RagError`, `RrfCandidate`, `RrfRanker`.
- From `oxirs`, `surrealdb_backend`, and `neo4j_backend`: `OxiRsTdbEngine`, `OxiRsTdbConfig`, `SurrealDbEngine`, `SurrealDbConfig`, `Neo4jEngine`, `Neo4jConfig`.

### Example

```rust
use embedag::prelude::*;

#[derive(Debug, Clone)]
struct TaskNode {
    id: String,
    title: String,
}

embedag::core::impl_node_payload!(
    TaskNode,
    key = |n| &n.id,
    name = |n| n.title.as_str(),
    level = |_| "task"
);

fn main() -> Result<(), DagError> {
    let mut dag = MemoryDag::new();
    let plan = MutationPlan::new()
        .upsert_node(TaskNode {
            id: "core".into(),
            title: "Build core library".into(),
        })
        .upsert_node(TaskNode {
            id: "cli".into(),
            title: "Build CLI binary".into(),
        })
        .add_edge(
            "core".into(),
            "cli".into(),
            EdgePayload::new("DEPENDS_ON", 1.0, ProvenanceKind::DeterministicRule)?,
        );

    let validated = plan.validate_cycles_differential(&dag)?;
    let _committed = validated.apply(&mut dag)?;

    let order = embedag::core::resolve::toposort(&dag)?;
    assert_eq!(order, vec!["core".to_string(), "cli".to_string()]);
    Ok(())
}
```

## Feature flags

| Feature | Default | Enables |
|---|---|---|
| `full` | Yes | `core`, `derive`, `llm`, `diesel`, `rag`, `oxirs-tdb`, `surrealdb`, `neo4j`, `turso`, `turso-async`, `matcher` |
| `core` | Via `full` | `dep:embedag-core` (`embedag::core`) |
| `derive` | Via `full` | `dep:embedag-derive` (`embedag::derive`) |
| `matcher` | Via `full` | `dep:embedag-matcher` and `core` (`embedag::matcher`) |
| `llm` | Via `full` | `dep:embedag-llm` (`embedag::llm`) |
| `diesel` | Via `full` | `dep:embedag-diesel` (`embedag::diesel`) |
| `turso` | Via `full` | `diesel` and `embedag-diesel/turso` |
| `turso-async` | Via `full` | `diesel` and `embedag-diesel/turso-async` |
| `rag` | Via `full` | `dep:embedag-rag` (`embedag::rag`) |
| `oxirs-tdb` | Via `full` | `dep:embedag-oxirs` (`embedag::oxirs`) |
| `surrealdb` | Via `full` | `dep:embedag-surrealdb` (`embedag::surrealdb`) |
| `neo4j` | Via `full` | `dep:embedag-neo4j` (`embedag::neo4j`) |

## Workspace and Dendrite relationship

Within the `embedag` workspace, this crate sits at the top of the dependency DAG and contains no logic of its own beyond re-exports and end-to-end integration tests (`crates/embedag/tests/e2e_pipeline.rs`).

Across the Dendrite repositories (`dendrite-core-rs`, `dendrite-git-rs`, `dendrite-rag-rs`, `dendrite-plane-rs`, and the Dendrite control plane), downstream crates typically depend on targeted sub-crates (`embedag-core`, `embedag-matcher`, `embedag-rag`, `embedag-diesel`) rather than the `full` facade to keep dependency graphs minimal:

- `dendrite-core-rs` and `dendrite-plane-rs` use `embedag-core` for dependency resolution (`resolve::toposort`, `levels`, `reverse_deps`, `critical_path`) and BLAKE3 Merkle diffing (`merkle::MerkleNode`, `merkle::diff`).
- `dendrite-git-rs` uses `embedag-matcher` for language-agnostic AST similarity and `embedag-core::merkle` for tree-level structural diffs.
- `dendrite-rag-rs` uses `embedag-rag` and `embedag-core` for reciprocal rank fusion, topological decay expansion, and Leiden clustering.
- The Dendrite control plane uses `embedag-core`, `embedag-diesel`, `embedag-llm`, and `embedag-rag` for ticket/repository DAG validation, relational persistence, and Graph-RAG context assembly.

## Build and test

```bash
cargo build -p embedag
cargo test -p embedag
```

## Specifications

- [`doc/00_MASTER_SPECIFICATIONS.md`](../../doc/00_MASTER_SPECIFICATIONS.md) — Architectural overview and specification index.
- [`doc/02_Workspace_Crate_Topology.md`](../../doc/02_Workspace_Crate_Topology.md) — Workspace crate hierarchy, dependency rules, and feature flags.
- [`doc/03_Architectural_Design_Patterns.md`](../../doc/03_Architectural_Design_Patterns.md) — Typestate, arena, capability trait, and waterfall patterns re-exported by the prelude.
