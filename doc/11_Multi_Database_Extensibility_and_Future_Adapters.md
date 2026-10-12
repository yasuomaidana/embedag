# Multi-Database Extensibility, Engine Evaluation & Future Adapters

## 1. Executive Summary
While the initial release of `embedag` concentrates on relational persistence with **PostgreSQL (via `diesel-async`)** and **SQLite (via `diesel` & `diesel-cte-ext`)**, the enterprise vision requires clean extensibility to **native graph databases, multi-model engines, and embedded vector stores**.

This document evaluates candidate database engines for future versions, audits how `embedag`'s Hexagonal Architecture (`GraphRepository` port) decouples domain logic from SQL, identifies which database engines will benefit most from `embedag`, and outlines high-priority auxiliary features for the crate roadmap.

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

---

### 2.1 SurrealDB (`embedag-surrealdb`) — The #1 Multi-Model Candidate

#### Profile & Architecture
- **Language**: 100% pure Rust (`surrealdb`).
- **Deployment**: Runs embedded in-process (`kv-rocksdb`, `kv-mem`) or as a distributed server.
- **Data Model**: Multi-model (Document JSON + Native Graph Pointers + Vector HNSW indexing).

#### Why SurrealDB Fits `embedag` Exceptionally Well
1. **Unifies the Three Storage Silos**: In PostgreSQL, `embedag` must orchestrate three distinct extensions/patterns: relational tables for metadata, `pgvector` for similarity, and recursive CTEs for graph traversals. In SurrealDB, all three are native within a single query:
   ```sql
   -- SurrealQL native graph traversal with vector similarity and JSON metadata
   SELECT 
       id, name, attributes,
       ->belongs_to->category.* AS categories,
       vector::similarity::cosine(embedding, $query_vector) AS score
   FROM product
   WHERE vector::similarity::cosine(embedding, $query_vector) > 0.8;
   ```
2. **True In-Process Testing & Edge Deployments**: Because SurrealDB runs embedded in pure Rust, integration test suites can run without launching Docker containers or linking C-FFI SQLite libraries.
3. **Mutual Benefit**: SurrealDB currently lacks an ergonomic ingestion pipeline to map arbitrary user structs into validated DAGs with LLM extraction and cycle guards. `embedag` acts as the ideal structured ingestion and reasoning engine for SurrealDB applications.

---

### 2.2 Neo4j / Memgraph (`embedag-neo4j`) — The #1 Native Property Graph Candidate

#### Profile & Architecture
- **Protocol**: Bolt binary network protocol via the pure-Rust `neo4rs` driver.
- **Query Language**: OpenCypher (`MATCH`, `MERGE`, `WHERE`).

#### Why Neo4j Fits `embedag`
1. **Arbitrary Variable-Length Path Traversal**: In relational SQL, arbitrary-depth queries require complex `WITH RECURSIVE` queries. In Cypher, multi-hop ancestor and descendant queries are natively expressive:
   ```cypher
   MATCH (seed:Node {id: $root_id})-[:HAS_VARIANT*0..5]->(descendant:Node)
   RETURN seed, descendant;
   ```
2. **Industrial Heterogeneous GNN Pipelines**: Enterprise data science teams frequently maintain PyTorch Geometric or DGL pipelines connected to Neo4j. `embedag` can stream validated, cycle-checked DAGs directly into Neo4j via atomic `MERGE` transactions.

---

### 2.3 LanceDB (`embedag-lancedb`) — Embedded Columnar Vector Store

#### Profile & Architecture
- **Language**: 100% Rust (`lancedb`), built on Apache Arrow and Lance columnar format.
- **Deployment**: Embedded in-process, zero-copy reads, serverless.
- **Fit**: Ideal for local/mobile apps with massive feature vectors and structured scalar filters, but lacks recursive multi-hop traversals (subgraphs must be managed in-memory via `petgraph`).

---

### 2.4 Oxigraph / `oxirs` (`embedag-oxigraph`) — W3C RDF & Semantic Web

#### Profile & Architecture
- **Language**: 100% Rust (`oxigraph`), SPARQL 1.1/1.2 query engine.
- **Fit**: For organizations requiring compliance with enterprise RDF ontologies, FIBO, or Dublin Core. `embedag` can serialize DAG edges into RDF Triples (`Subject-Predicate-Object`).

---

## 3. Extensibility Audit of the Current Architecture

Does the current trait design allow seamless extension to non-SQL databases? **Yes, with high fidelity.**

### 3.1 Decoupling Verification: `GraphRepository` Port
The `GraphRepository` trait in `embedag-core` does **not** leak any Diesel or SQL types:
```rust
#[async_trait::async_trait]
pub trait GraphRepository: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    async fn get_node(&self, id: &uuid::Uuid) -> Result<Option<PersistedNode>, Self::Error>;
    
    async fn fetch_subgraph(
        &self,
        root_id: &uuid::Uuid,
        direction: TraversalDirection,
        max_depth: usize,
    ) -> Result<petgraph::graph::DiGraph<PersistedNode, PersistedEdge>, Self::Error>;

    async fn persist_graph_delta(
        &self,
        nodes: &[PersistedNode],
        edges: &[PersistedEdge],
    ) -> Result<(), Self::Error>;

    async fn find_similar_nodes(
        &self,
        embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<(PersistedNode, f32)>, Self::Error>;

    async fn lookup_alias(&self, alias: &str) -> Result<Option<uuid::Uuid>, Self::Error>;
    async fn register_alias(&self, alias: &str, node_id: uuid::Uuid) -> Result<(), Self::Error>;
}
```

### 3.2 Granular Capability Traits (Interface Segregation Principle)
To make implementation trivial for databases that do not natively support all features (e.g., LanceDB has no native recursive graph queries; standard SQLite has no native vector index), `embedag` decomposes `GraphRepository` into composed capability sub-traits:

```rust
/// Core topological storage: node CRUD and graph traversal
#[async_trait::async_trait]
pub trait GraphStorage: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    async fn get_node(&self, id: &uuid::Uuid) -> Result<Option<PersistedNode>, Self::Error>;
    async fn fetch_subgraph(&self, root_id: &uuid::Uuid, direction: TraversalDirection, max_depth: usize) -> Result<petgraph::graph::DiGraph<PersistedNode, PersistedEdge>, Self::Error>;
    async fn persist_graph_delta(&self, nodes: &[PersistedNode], edges: &[PersistedEdge]) -> Result<(), Self::Error>;
}

/// Vector candidate similarity index
#[async_trait::async_trait]
pub trait VectorIndex: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    async fn find_similar_nodes(&self, embedding: &[f32], limit: usize) -> Result<Vec<(uuid::Uuid, f32)>, Self::Error>;
}

/// Tier 1 deterministic alias and normalization store
#[async_trait::async_trait]
pub trait AliasStorage: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    async fn lookup_alias(&self, alias: &str) -> Result<Option<uuid::Uuid>, Self::Error>;
    async fn register_alias(&self, alias: &str, node_id: uuid::Uuid) -> Result<(), Self::Error>;
}

/// Universal umbrella repository blanket-implemented when all capabilities are present
pub trait GraphRepository: GraphStorage + VectorIndex + AliasStorage {}
impl<T: GraphStorage + VectorIndex + AliasStorage> GraphRepository for T {}
```
This enables specialized adapters:
- A pure vector store (e.g. Qdrant) can implement `VectorIndex`.
- An in-memory cache can implement `AliasStorage`.
- A pure graph database (e.g. Neo4j) can implement `GraphStorage`.
- The user can compose them into a unified `CompositeRepository<GraphStorage, VectorIndex, AliasStorage>`.

---

## 4. High-Value Auxiliary Features for `embedag`

Based on the multi-database review, the following complementary capabilities represent the most valuable additions to the crate roadmap:

### 4.1 Schema Evolution & Graph Migrations
When taxonomies change (e.g., splitting a category `"Beverages"` into `"Cold Drinks"` and `"Hot Beverages"`), `embedag` can provide a `GraphMigration` engine that generates transactional graph deltas across both memory and storage.

### 4.2 Graph Export & Visualization Facades
Add built-in exporters in `embedag-core`:
- **Mermaid Markdown**: `MemoryDag::to_mermaid()` for instant documentation rendering.
- **Graphviz DOT**: `MemoryDag::to_dot()`.
- **JSON-LD / W3C RDF**: For interoperability with semantic web ecosystems (`oxirs`).

### 4.3 Community Detection & Topological Clustering
Port the **Leiden and Louvain community detection algorithms** into `embedag-rag`. By partitioning massive DAGs into coherent topological clusters, the RAG query engine can route natural language questions to entire subgraphs rather than flat candidate nodes.

### 4.4 Temporal Versioning (Bitemporal DAGs)
Tracking `valid_from` and `valid_to` timestamps on edges, enabling historical queries (e.g., *"What was the taxonomy of Carlos V on January 1st, 2025?"*).
