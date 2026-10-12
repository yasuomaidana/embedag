# Multi-Database Extensibility, Engine Evaluation & Future Adapters

| Metadata | Details |
| :--- | :--- |
| **Document ID** | `20_Multi_Database_Extensibility_and_Future_Adapters` |
| **Domain** | Database Extensibility & Future Adapter Ecosystem |
| **Responsibility** | Evaluation of Future Graph & Multi-Model Backends (SurrealDB, Neo4j, LanceDB, Oxigraph) and Adapter Roadmap |
| **Target Crates** | `embedag-core` (Ports), Future Crates (`embedag-surrealdb`, `embedag-neo4j`, etc.) |
| **Status** | Definitive Architectural Specification |

---

## 1. Executive Summary & Extensibility Vision

While the initial release of `embedag` concentrates on relational persistence with **PostgreSQL (via `diesel-async`)** and **SQLite (via `diesel` & `diesel-cte-ext`)**, the enterprise vision requires clean extensibility to **native graph databases, multi-model engines, and embedded vector stores**.

This document evaluates candidate database engines for future versions, audits how `embedag`'s Hexagonal Architecture (`GraphRepository` port) decouples domain logic from SQL, and outlines the adapter roadmap.

---

## 2. Evaluation of Future Database Backends

```mermaid
flowchart TD
    subgraph Core ["embedag-core (Domain & Ports)"]
        Repo["GraphRepository (Trait Port)"]
    end

    subgraph Relational ["Relational Adapters (v0.1 Foundation)"]
        Pg["embedag-diesel (PostgreSQL + diesel-async + pgvector)"]
        Sqlite["embedag-diesel (SQLite + diesel-cte-ext)"]
    end

    subgraph NativeGraph ["Native Graph Adapters (v0.2 Enterprise)"]
        Surreal["embedag-surrealdb (SurrealDB Embedded / Server)"]
        Neo["embedag-neo4j (Neo4j / Memgraph via Bolt Cypher)"]
    end

    subgraph VectorSemantic ["Vector & Semantic Adapters (v0.3 Specialty)"]
        Lance["embedag-lancedb (Embedded Columnar Vector)"]
        Oxi["embedag-oxigraph (W3C RDF / SPARQL Triples)"]
    end

    Repo <|.. Pg
    Repo <|.. Sqlite
    Repo <|.. Surreal
    Repo <|.. Neo
    Repo <|.. Lance
    Repo <|.. Oxi
```

### 2.1 SurrealDB (`embedag-surrealdb`) — The #1 Multi-Model Candidate
- **Profile**: Native Rust multi-model database supporting tabular, document, graph edges (`RELATE`), and vector search.
- **Why It Fits**: Embedded in-process mode (`surrealdb::engine::local::Mem`), native graph edges with bidirectional traversal, and built-in vector indexing.

### 2.2 Neo4j / Memgraph (`embedag-neo4j`) — The #1 Native Property Graph Candidate
- **Profile**: Industry standard labeled property graph engines.
- **Why It Fits**: Direct Cypher query mapping (`MATCH (p)-[:CONTAINS]->(c)`), enterprise acceptance, and visualization tools.

### 2.3 LanceDB (`embedag-lancedb`) — Embedded Columnar Vector Store
- **Profile**: Serverless vector database built on Apache Arrow and Lance columnar format.
- **Why It Fits**: Ultra-fast disk-based vector search with zero daemon processes.

### 2.4 Oxigraph (`embedag-oxigraph`) — W3C RDF & Semantic Web
- **Profile**: Pure Rust triplestore implementing W3C RDF and SPARQL 1.1.
- **Why It Fits**: Pure Rust, zero FFI, standard ontology compliance.

---

## 3. Decoupling Verification of `GraphRepository`

The `GraphRepository` trait port defined in `embedag-core` requires zero SQL constructs:
- Return types use domain structs (`T: NodePayload`).
- Query inputs use business keys (`T::Key`) or normalized slices (`&[f32]`).
- All mutations pass through typed `MutationDirective<T>` vectors.

This guarantees that any backend implementing `GraphRepository` will integrate seamlessly without breaking existing domain code.

---

## 4. Auxiliary Enterprise Features Roadmap

1. **Interactive Graph Visualizer**: Export DAGs directly to Graphviz DOT, Mermaid diagrams, and Cytoscape.js JSON.
2. **Topological Community Detection**: Louvain and Leiden community detection algorithms for clustering related subgraphs.
3. **Bitemporal Auditing**: Track `system_time` vs `valid_time` on all node and edge mutations.
4. **Declarative Schema Migrations**: Automated migration utilities for upgrading entity levels and property schemas.
