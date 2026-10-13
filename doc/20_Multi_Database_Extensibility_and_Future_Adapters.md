# Multi-Database Extensibility, Engine Evaluation & Future Adapters

| Metadata | Details |
| :--- | :--- |
| **Document ID** | `20_Multi_Database_Extensibility_and_Future_Adapters` |
| **Domain** | Database Extensibility & Future Adapter Ecosystem |
| **Responsibility** | Evaluation of Future Graph & Multi-Model Backends (OxiRS, SurrealDB, Neo4j, LanceDB), Containerized Testing Parity, and Adapter Roadmap |
| **Target Crates** | `embedag-core` (Ports), Future Crates (`embedag-oxirs`, `embedag-surrealdb`, `embedag-neo4j`, etc.) |
| **Status** | Definitive Architectural Specification |

---

## 1. Executive Summary & Extensibility Vision

While the initial release of `embedag` concentrates on relational persistence with **PostgreSQL (via `diesel-async`)** and **SQLite (via `diesel` & `leynos/diesel-cte-ext`)**, the enterprise vision requires clean extensibility to **native graph databases, multi-model engines, and sovereign semantic web stores**.

This document evaluates candidate database engines for future versions, audits how `embedag`'s Hexagonal Architecture (`GraphRepository` port) decouples domain logic from SQL, and outlines the adapter roadmap with emphasis on the KitaSan open-source stack.

---

## 2. Evaluation of Future Database Backends

```mermaid
flowchart TD
    subgraph Core ["embedag-core (Domain & Ports)"]
        Repo["GraphRepository (Trait Port)"]
    end

    subgraph Relational ["Relational Adapters (v0.1 Foundation)"]
        Pg["embedag-diesel (PostgreSQL + diesel-async + pgvector)"]
        Sqlite["embedag-diesel (SQLite + leynos/diesel-cte-ext)"]
    end

    subgraph SovereignSemantic ["Sovereign Semantic Web (KitaSan Priority 1)"]
        OxiRS["embedag-oxirs (cool-japan/oxirs)<br/>- SPARQL 1.1/1.2 Query Engine (oxirs-arq)<br/>- Durable TDB2 On-Disk Engine (oxirs-tdb)<br/>- RDF-Star & GraphQL Autoschema"]
    end

    subgraph NativeGraph ["Native Graph Adapters (Enterprise)"]
        Surreal["embedag-surrealdb (SurrealDB Embedded / Server)"]
        Neo["embedag-neo4j (Neo4j / Memgraph via Bolt Cypher)"]
    end

    subgraph VectorSpecialty ["Embedded Vector Stores"]
        Lance["embedag-lancedb (Embedded Columnar Vector)"]
    end

    subgraph TestHarness ["Mandatory Test Harness"]
        TC["Testcontainers (rust.testcontainers.org)"]
    end

    Repo <|.. Pg
    Repo <|.. Sqlite
    Repo <|.. OxiRS
    Repo <|.. Surreal
    Repo <|.. Neo
    Repo <|.. Lance

    TC -.->|PostgreSQL + pgvector| Pg
    TC -.->|SurrealDB Container| Surreal
    TC -.->|Neo4j Container| Neo
```

### 2.1 `oxirs` (`embedag-oxirs`) — The Priority 1 Sovereign Semantic Web & RDF-Star Engine
- **Profile**: Native pure-Rust Semantic Web, SPARQL 1.2, GraphQL, and RDF-Star platform from [`cool-japan/oxirs`](https://github.com/cool-japan/oxirs).
- **Why It Fits**:
  - **Zero C/C++ FFI**: 100% pure Rust compiled with `cargo build`.
  - **Durable TDB2 Storage Engine**: Provides fsync-backed, crash-resilient disk storage with GSPO/GPOS/GOSP quad indexes (`oxirs-tdb`).
  - **SPARQL 1.2 & Federated Queries**: Complete query engine (`oxirs-arq`) with query optimizer and federated `SERVICE` querying.
  - **Interoperability**: Connects `embedag` taxonomies to enterprise knowledge graphs, ontologies, and W3C open data standards.

### 2.2 SurrealDB (`embedag-surrealdb`) — The #1 Multi-Model Candidate
- **Profile**: Native Rust multi-model database supporting tabular, document, graph edges (`RELATE`), and vector search.
- **Why It Fits**: Embedded in-process mode (`surrealdb::engine::local::Mem`), native graph edges with bidirectional traversal, and built-in vector indexing.

### 2.3 Neo4j / Memgraph (`embedag-neo4j`) — The #1 Native Property Graph Candidate
- **Profile**: Industry standard labeled property graph engines.
- **Why It Fits**: Direct Cypher query mapping (`MATCH (p)-[:CONTAINS]->(c)`), enterprise acceptance, and visualization tools.

### 2.4 LanceDB (`embedag-lancedb`) — Embedded Columnar Vector Store
- **Profile**: Serverless vector database built on Apache Arrow and Lance columnar format.
- **Why It Fits**: Ultra-fast disk-based vector search with zero daemon processes.

---

## 3. Decoupling Verification of `GraphRepository`

The `GraphRepository` trait port defined in `embedag-core` requires zero SQL constructs:
- Return types use domain structs (`T: NodePayload`).
- Query inputs use business keys (`T::Key`) or normalized slices (`&[f32]`).
- All mutations pass through typed `MutationDirective<T>` vectors.

This guarantees that any backend implementing `GraphRepository` will integrate seamlessly without breaking existing domain code.

### 3.1 Consistent Containerized Test Mandate for Future Adapters
Just as `embedag-diesel` enforces mandatory integration testing with **[Testcontainers for Rust](https://rust.testcontainers.org)** for PostgreSQL, every future persistence adapter must provide a dedicated integration test suite backed by real throwaway containers:
- `embedag-surrealdb`: Verified using `testcontainers` SurrealDB image in distributed mode, alongside embedded `kv-mem`.
- `embedag-neo4j`: Verified against real Neo4j container instances using the `testcontainers` Neo4j module.
- `embedag-oxirs`: Verified against real durable TDB2 datasets and embedded SPARQL test suites.

This guarantees zero architectural drift and unified quality assurance across all supported database backends.

---

## 4. Auxiliary Enterprise Features Roadmap

1. **Interactive Graph Visualizer**: Export DAGs directly to Graphviz DOT, Mermaid diagrams, and Cytoscape.js JSON.
2. **Topological Community Detection**: Louvain and Leiden community detection algorithms for clustering related subgraphs.
3. **Bitemporal Auditing**: Track `system_time` vs `valid_time` on all node and edge mutations.
4. **Declarative Schema Migrations**: Automated migration utilities for upgrading entity levels and property schemas.

---

## 5. Delivered Implementation: Native OxiRS TDB2 Persistent Storage Engine (`embedag-oxirs`) (Ticket 7.3.1)

### 5.1 Architectural Assessment & Technical Rationale
The implementation of `embedag-oxirs` delivers native, pure-Rust Semantic Web and RDF-Star persistence directly inside the `embedag` workspace. It fulfills Ticket 7.3.1 and provides immediate advantages over relational-only storage:

1. **Zero External Daemons / Zero Docker Dependency:**
   - Persistence occurs directly on local filesystem paths via line-atomic N-Quads append logging (`data.nq`) with explicit fsync durability policies (`SyncPolicy::EveryN` vs `SyncPolicy::OnFlush`).
2. **Direct Graph Knowledge Model:**
   - Completely eliminates SQL join tables and impedance mismatches. Entities, hierarchical edges, confidence scores, and metadata map natively to W3C RDF subjects, predicates, objects, and quads.
3. **Sub-Second Cold Starts via Memory-Mapped Snapshots:**
   - High-performance frozen snapshots (`snapshot.oxsnap`) allow knowledge graphs to be loaded via `mmap` in sub-milliseconds without parsing N-Quads line-by-line or rebuilding BTree indexes on every startup.
4. **SPARQL 1.1/1.2 Query & Graph-RAG Grounding:**
   - Persisted graphs can be traversed via SPARQL queries (`query_sparql`) or pattern queries (`find_quads`), directly empowering `oxirag` Layer 4 retrieval in both `embedag` and `docula`.

```mermaid
flowchart TD
    subgraph Core ["embedag-core"]
        MemDag["MemoryDag<T>"]
        RepoPort["GraphRepository<T> (Port Trait)"]
    end

    subgraph OxiRSCrate ["embedag-oxirs (Native Storage Engine)"]
        Config["OxiRsTdbConfig<br/>(base_iri, schema_ns, SyncPolicy)"]
        Engine["OxiRsTdbEngine<br/>(Arc<RdfStore>)"]
        Vocab["SchemaVocab & Mapping<br/>(node_to_quads, edge_to_quads)"]
        RepoImpl["GraphRepository Impl<br/>(ReadOnly, Transactional, VectorSearch)"]
    end

    subgraph DurableStorage ["Persistent On-Disk Assets"]
        AppendLog["data.nq<br/>(Crash-Safe Line-Atomic Append Log)"]
        Snapshot["snapshot.oxsnap<br/>(Frozen Mmap Instant Cold Start)"]
        SparqlExec["SPARQL 1.1/1.2 Query Executor"]
    end

    subgraph CrossWorkspaceConsumer ["Cross-Workspace Consumer: docula"]
        DoculaRAG["docula-rag (EmbedagGraphRetriever)"]
        DocPipeline["DoculaPipeline (detectable RAG context)"]
    end

    MemDag -->|export_dag| Engine
    Engine -->|load_dag| MemDag
    Engine --> Config
    Engine --> Vocab
    Engine --> RepoImpl
    RepoPort <|.. RepoImpl

    Engine --> AppendLog
    Engine --> Snapshot
    Engine --> SparqlExec

    DoculaRAG --> Engine
    DoculaRAG --> DocPipeline
```

### 5.2 Implemented Public APIs & Capabilities
- **Engine Core**: [`OxiRsTdbEngine`](file:///Users/yasuomaidana/RustRoverProjects/embedag/crates/embedag-oxirs/src/engine.rs) wrapping `Arc<RdfStore>`, supporting `open`, `open_persistent`, and `in_memory`.
- **Repository Compliance**: Full implementation of [`ReadOnlyGraphRepository<T>`](file:///Users/yasuomaidana/RustRoverProjects/embedag/crates/embedag-core/src/traits.rs#L121), [`TransactionalGraphRepository<T>`](file:///Users/yasuomaidana/RustRoverProjects/embedag/crates/embedag-core/src/traits.rs#L145), and [`VectorSearchRepository<T>`](file:///Users/yasuomaidana/RustRoverProjects/embedag/crates/embedag-core/src/traits.rs#L134).
- **Snapshot Accelerator**: `save_snapshot` and `build_snapshot` for baking frozen `snapshot.oxsnap` files.
- **Reference Example**: [`examples/oxirs_persistent_dag.rs`](file:///Users/yasuomaidana/RustRoverProjects/embedag/crates/embedag-oxirs/examples/oxirs_persistent_dag.rs) demonstrating healthcare ontology persistence and SPARQL extraction.

