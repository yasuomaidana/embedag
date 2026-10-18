# Dendrite Integration & Repository Responsibilities

This document defines the architectural boundary, responsibilities, exposed interfaces, and extraction mappings of the `embedag` repository in relation to the Dendrite control-plane platform and its multi-directory sibling ecosystem.

---

## 1. Purpose

`embedag` is the foundational pure-Rust graph engine, in-memory directed acyclic graph (DAG) arena, and hybrid Graph-RAG library powering deterministic dependency resolution across the Dendrite developer platform. It provides language-agnostic DAG topological sorting, parallel execution level grouping, reverse dependency impact analysis, weighted critical-path calculations, BLAKE3 Merkle change trees, structural AST similarity matching, and relational/multi-model persistence. Within the broader Dendrite sub-project decomposition, `embedag` serves as the shared sovereign Rust library consumed by specialized Rust sidecars (`dendrite-core-rs` (`Dendrite:docs/architecture/2026-10-quality-audit-execution-plan.md`) and `dendrite-rag-rs` (`Dendrite:docs/architecture/2026-10-quality-audit-execution-plan.md`)), replacing ad-hoc Go heuristic solvers with mathematically verified, zero-cloud graph algorithms.

---

## 2. Owns vs. Does NOT Own

### Owns (embedag Capabilities)
- **In-Memory DAG Arena**: The [`MemoryDag`](../crates/embedag-core/src/dag.rs) arena (backed by `petgraph`) supporting acyclic cycle detection, typestate mutation plans ([`Unverified`](../crates/embedag-core/src/mutation.rs), [`CycleValidated`](../crates/embedag-core/src/mutation.rs), [`Committed`](../crates/embedag-core/src/mutation.rs)), streaming traversal iterators, degree pagination for heavy hubs, and declarative schema migrations.
- **Topological Resolution & Scheduling Algorithms**: Deterministic topological sorting ([`toposort`](../crates/embedag-core/src/resolve.rs#L21)), parallel execution levels ([`levels`](../crates/embedag-core/src/resolve.rs#L83)), reverse dependency impact ordering ([`reverse_deps`](../crates/embedag-core/src/resolve.rs#L156)), and weighted critical path calculation ([`critical_path`](../crates/embedag-core/src/resolve.rs#L187)) with deterministic tie-breaking.
- **Content-Addressed Merkle Delta Trees**: Hierarchical BLAKE3 Merkle hashing ([`MerkleNode`](../crates/embedag-core/src/merkle.rs#L14)), path-independent directory tree synthesis ([`MerkleNode::from_paths`](../crates/embedag-core/src/merkle.rs#L69)), and structural delta diffing ([`diff`](../crates/embedag-core/src/merkle.rs#L137)) that prunes unchanged subtrees during change detection and state journal verification.
- **AST Similarity Matching & Similarity Graphs**: Structural AST tree representation ([`Tree`](../crates/embedag-matcher/src/tree.rs)), subtree structural hashing ([`AstMatcher`](../crates/embedag-matcher/src/ast.rs)), and undirected similarity clustering ([`SimilarityGraph`](../crates/embedag-matcher/src/graph.rs)) in [`embedag-matcher`](../crates/embedag-matcher/src/lib.rs).
- **Procedural Derive Macros**: [`#[derive(AsGraphContext)]`](../crates/embedag-derive/src/lib.rs) and [`#[derive(GraphTarget)]`](../crates/embedag-derive/src/lib.rs) for automatic struct-to-graph context mapping.
- **Continuous & Mixed-Modality Encoders**: Scalar log scaling, cyclical temporal encoders, Uber H3 geospatial indexing via `h3o`, and composite hybrid distance functions.
- **Relational Persistence**: PostgreSQL with `pgvector`, SQLite (`bundled`), and Turso / libSQL (`diesel-libsql` and `libsql 0.9`) with asynchronous pooling (`diesel-async`) and recursive CTE traversal builders ([`diesel-cte-ext`](../crates/embedag-diesel/src/lib.rs)).
- **Native Multi-Model Storage Adapters**: Native OxiRS TDB2 persistent storage ([`embedag-oxirs`](../crates/embedag-oxirs/src/lib.rs)), SurrealDB multi-model engine ([`embedag-surrealdb`](../crates/embedag-surrealdb/src/lib.rs)), and Neo4j OpenCypher Bolt adapter ([`embedag-neo4j`](../crates/embedag-neo4j/src/lib.rs)).
- **Hybrid Graph-RAG Retrieval Engine**: Two-stage retrieval fusing dense vector search and sparse lexical FTS via Reciprocal Rank Fusion (RRF), bounded topological subgraph expansion with decay, grounded citation prompt packaging, and topological community detection (Leiden clustering planned in Milestone 9 for code knowledge graphs).
- **Facade Feature Customization**: Slim dependency configurations (`default-features = false, features = ["core", "matcher"]`) enabling consumers to avoid unwanted database drivers, LLM engines, or native C-bindings.

### Does NOT Own (Explicit Exclusions)
- **`dendrite-control` (Dendrite Go repository, module `dendrite.dev/control`)**:
  - Owns the primary control-plane daemon, Cobra CLI commands (`cmd/dendrite` (`Dendrite:cmd/dendrite/main.go`)), Web UI HTTP/WebSocket server (`pkg/server` (`Dendrite:pkg/server`), `pkg/ui` (`Dendrite:pkg/ui/penpot`)), personal context greeting state machines (`pkg/personalcontext` (`Dendrite:pkg/personalcontext/greeting`)), LiteLLM gateway rendering and token budget governance (`pkg/gateway` (`Dendrite:pkg/gateway`), `pkg/tokens` (`Dendrite:pkg/tokens`)), environment replication and Docker stack deployment (`pkg/env` (`Dendrite:pkg/env`)), and external tracker API synchronization (`pkg/tracker/plane` (`Dendrite:pkg/tracker/plane/client.go`), `pkg/tracker/jira` (`Dendrite:pkg/tracker/jira`)).
- **`dendrite-mcp-hub` (Go module `dendrite.dev/mcphub`)**:
  - Owns the standalone Model Context Protocol (MCP) server extracted from `pkg/mcpserver` (`Dendrite:pkg/mcpserver`), managing JetBrains IDE IPC bridges, Penpot design token sync, and upstream tool multiplexing over stdio and SSE.
- **`dendrite-core-rs` (Rust crate `dendrite-core`)**:
  - Owns the Tree-sitter polyglot parsing engine (signature folding, `last_complete_node` truncation anchor splicing, McCabe cyclomatic and cognitive complexity metrics), pre-flight token counting across engine tokenizers, and the stdio JSON-RPC 2.0 sidecar server protocol. Consumes `embedag` as a library dependency.
- **`dendrite-rag-rs` (Rust crate `dendrite-rag`)**:
  - Owns the standalone stdio JSON-RPC 2.0 daemon for local-first retrieval, CAS blob checkpointer management, and 3-mode distributed memory sync (`local-only`, `edge`, `push`). Consumes `embedag-rag` and `embedag-core` as underlying engines.
- **`dendrite-git-rs` (Rust crate `dendrite-git`)**:
  - Owns gitoxide (`gix`)-based git object operations, hunk extraction, hunk dependency graphs, semantic hunk clustering, and `--local-review` stacked branch orchestration.
- **Authoritative Issue Trackers & LLM Gateways**:
  - Plane v2 (`http://localhost:8090`) owns authoritative issue storage and Gantt views.
  - LiteLLM Gateway (`http://localhost:4000`) owns API keys, spend ledgers, and cloud failovers.

---

## 3. Interfaces

### Exposed Interface Contract
Unlike the sibling `dendrite-*-rs` crates, `embedag` does **not** ship a standalone JSON-RPC daemon binary. Instead, it exposes a pure Rust crate library API across its workspace members:

1. **Facade Crate [`embedag`](../crates/embedag/src/lib.rs)**:
   - Feature flags:
     - `core`: Exposes [`embedag::dag`](../crates/embedag/src/lib.rs#L44), re-exporting [`MemoryDag`](../crates/embedag-core/src/dag.rs), [`toposort`](../crates/embedag-core/src/resolve.rs#L21), [`levels`](../crates/embedag-core/src/resolve.rs#L83), [`reverse_deps`](../crates/embedag-core/src/resolve.rs#L156), [`critical_path`](../crates/embedag-core/src/resolve.rs#L187), and [`embedag::dag::merkle`](../crates/embedag/src/lib.rs#L49) ([`MerkleNode`](../crates/embedag-core/src/merkle.rs#L14), [`diff`](../crates/embedag-core/src/merkle.rs#L137)).
     - `matcher`: Exposes [`embedag::matcher`](../crates/embedag/src/lib.rs#L56), re-exporting [`Tree`](../crates/embedag-matcher/src/tree.rs), [`AstMatcher`](../crates/embedag-matcher/src/ast.rs), and [`SimilarityGraph`](../crates/embedag-matcher/src/graph.rs).
     - `rag`: Exposes [`embedag::rag`](../crates/embedag/src/lib.rs#L68) ([`OxiRagEngine`](../crates/embedag-rag/src/lib.rs), [`RrfRanker`](../crates/embedag-rag/src/lib.rs), [`SubgraphExpander`](../crates/embedag-rag/src/lib.rs), [`GroundedEvidencePackager`](../crates/embedag-rag/src/lib.rs)).
     - `diesel`: Exposes relational schema repositories, `pgvector`, SQLite, and Turso adapters.
2. **Sub-Crates**:
   - [`embedag-core`](../crates/embedag-core/src/lib.rs): Core domain models, DAG arena, topological algorithms, Merkle trees, encoders.
   - [`embedag-matcher`](../crates/embedag-matcher/src/lib.rs): AST similarity metrics and graph clustering.
   - [`embedag-rag`](../crates/embedag-rag/src/lib.rs): Graph-RAG query execution and topological evidence packaging.
   - [`embedag-diesel`](../crates/embedag-diesel/src/lib.rs): Relational database backend for PostgreSQL, SQLite, and Turso / libSQL.

### Consumers
- **`dendrite-core-rs`**: Consumes `embedag` with features `["core", "matcher"]` to implement the `dendrite-core` JSON-RPC sidecar protocol for AST operations and dependency graph solving.
- **`dendrite-rag-rs`**: Consumes `embedag` with features `["core", "rag"]` (and optionally `matcher`) to power the `dendrite-rag` sidecar for hybrid search, memory sync, and knowledge graphs.
- **`dendrite-control` (indirect)**: Interacts with `embedag` logic through the Go `sidecar.Client` calling the `dendrite-core` and `dendrite-rag` sidecar binaries over stdio JSON-RPC 2.0 with schema definitions in `proto/*.json`.

---

## 4. Code Being Extracted / Replaced

The following table lists the exact Dendrite Go packages and files (verified from inspecting `pkg/` and `cmd/dendrite/`) that are being extracted or replaced by `embedag` capabilities, along with the Go fallback behavior:

| Dendrite Go Package & Files | Existing Go Implementation | Replaced / Extracted By (embedag Target) | Go Fallback Behavior |
|---|---|---|---|
| `pkg/pool/dag/solver.go` (`Dendrite:pkg/pool/dag/solver.go`)<br>`pkg/pool/dag/manifest.go` (`Dendrite:pkg/pool/dag/manifest.go`)<br>`pkg/pool/dag/types.go` (`Dendrite:pkg/pool/dag/types.go`)<br>`cmd/dendrite/pool.go` (`Dendrite:cmd/dendrite/pool.go`) | `dag.DAGSolver`: Kahn's toposort (`computeDeterministicTopoOrder`), 3-color DFS cycle detector (`detectCycle`), manifest parser for `go.mod`, `Cargo.toml`, `package.json`, `pyproject.toml`. | [`embedag::dag::toposort`](../crates/embedag-core/src/resolve.rs#L21) and [`embedag::dag::levels`](../crates/embedag-core/src/resolve.rs#L83) wrapped in `dendrite-core-rs` sidecar (Ticket 9.2.1 / DEN-051). | In-process Go `dag.DAGSolver` in `pkg/pool/dag/solver.go` (`Dendrite:pkg/pool/dag/solver.go`) activates if the `dendrite-core` binary is missing or handshake fails. |
| `pkg/tracker/dag/dag.go` (`Dendrite:pkg/tracker/dag/dag.go`)<br>`pkg/tracker/dag/types.go` (`Dendrite:pkg/tracker/dag/types.go`)<br>*(referenced as `pkg/stories/dag`, `pkg/tracker` in DEN-074)* | `dag.Graph`: Ticket/milestone dependency ingestion, DFS cycle detection, Kahn toposort, downstream unblock weights (`CalculateUnblockWeights` with BFS distance decay), Gantt planning (`GenerateGanttPlan`), and `SmartSelect`. | [`embedag::dag::toposort`](../crates/embedag-core/src/resolve.rs#L21) and [`embedag::dag::critical_path`](../crates/embedag-core/src/resolve.rs#L187) wrapped in `dendrite-core-rs` (Ticket 9.2.2 / DEN-074). | In-process Go `dag.Graph` in `pkg/tracker/dag/dag.go` (`Dendrite:pkg/tracker/dag/dag.go`) remains active as the primary Go implementation. |
| `pkg/store/segmented/segmented_store.go` (`Dendrite:pkg/store/segmented/segmented_store.go`)<br>`pkg/store/store.go` (`Dendrite:pkg/store/store.go`)<br>`pkg/node/wal/` (`Dendrite:docs/roadmap/development-status-roadmap.md`) *(planned, To Do)* | `segmented.SegmentedStore`: Content-addressed chunk checkpointer with SHA256/BLAKE3 hashing, SQLite index (`meta.db`), and compressed blob files. Planned WAL state journal protocol in DEN-068. | [`embedag_core::merkle`](../crates/embedag-core/src/merkle.rs) ([`MerkleNode`](../crates/embedag-core/src/merkle.rs#L14), [`diff`](../crates/embedag-core/src/merkle.rs#L137)) for Merkle digest verification and crash recovery (Ticket 9.3.1 / DEN-068). | Standard SQLite ACID transaction commit in `meta.db` and single-hash verification in Go `SegmentedStore` (`Dendrite:pkg/store/segmented/segmented_store.go`). |
| `pkg/rag/indexer.go` (`Dendrite:pkg/rag/indexer.go`)<br>`pkg/rag/ask.go` (`Dendrite:pkg/rag/ask.go`)<br>`pkg/rag/types.go` (`Dendrite:pkg/rag/types.go`) | `rag.Indexer`: In-process Go AST chunking (`chunkGoAST` via `go/parser`), Markdown heading chunking, disk cache (`rag_cache.json`), and Okapi BM25 scoring. | [`embedag-rag`](../crates/embedag-rag/src/lib.rs): Dense vector + sparse FTS indexing, Reciprocal Rank Fusion (`RrfRanker`), bounded subgraph expansion, and Leiden community clustering (Ticket 9.4.1 / DEND-202) wrapped in `dendrite-rag-rs`. | In-process Go Okapi BM25 indexer in `pkg/rag/indexer.go` (`Dendrite:pkg/rag/indexer.go`) serves as the persistent fallback. |
| `pkg/ast/complexity.go` (`Dendrite:pkg/ast/complexity.go`) | `AnalyzeSourceComplexity`: Line-based heuristic scanning (`bufio.Scanner` + string prefix matching on `if `, `for `, `case `) for cyclomatic and cognitive complexity. | Language-agnostic structural AST representations ([`Tree`](../crates/embedag-matcher/src/tree.rs)) and subtree structural matching ([`AstMatcher`](../crates/embedag-matcher/src/ast.rs)) in `embedag-matcher`, coupled with Tree-sitter in `dendrite-core-rs`. | Go standard library `go/ast` parser in `pkg/ast/complexity.go` (`Dendrite:pkg/ast/complexity.go`) (Pillar 1). |

---

## 5. Dependencies on Sibling Repositories

### Downstream Consumption (How Siblings Consume embedag)
Sibling crates in the multi-directory workspace consume `embedag` via Cargo path dependencies or Cargo pool patch overrides generated by `dendrite pool resolve`:

```toml
# In dendrite-core-rs/Cargo.toml
[dependencies]
embedag = { path = "../../RustRoverProjects/embedag/crates/embedag", default-features = false, features = ["core", "matcher"] }

# In dendrite-rag-rs/Cargo.toml
[dependencies]
embedag = { path = "../../RustRoverProjects/embedag/crates/embedag", default-features = false, features = ["core", "rag"] }
```

When managed under `dendrite pool`, `pool resolve` injects the patch directive:
```toml
[patch."https://github.com/yasuomaidana/embedag"]
embedag = { path = "../../RustRoverProjects/embedag/crates/embedag" }
```

### Upstream Dependencies (embedag's External Dependencies)
`embedag` is fully self-contained and has **zero dependencies** on Dendrite Go packages or sibling Rust crates:
- **Graph & Math**: `petgraph` (0.8), `blake3` (1.0), `h3o` (0.11), `chrono` (0.4), `uuid` (1.0).
- **Persistence**: `diesel` (2.3), `diesel-async` (0.9), `diesel-cte-ext` (0.3), `libsql` (0.9), `diesel-libsql` (0.1.4), `tokio-postgres` (0.7).
- **Inference & Vector**: `oxirag` (0.1.1), `oxirs-core` (0.4.1), `candle-core` (0.11), `oxillama` (0.1.4), `schemars` (1.0), `gbnf` (0.2).
- **Multi-Model**: `surrealdb` (3.3), `neo4rs` (0.8.0).
- **Test Harness**: `testcontainers` (0.27), `testcontainers-modules` (0.15) for mandatory PostgreSQL, SurrealDB, and Neo4j verification.

---

## 6. Ticket Map

Tickets relocated from Dendrite (and tracked authoritatively in `embedag`'s [`DEV_STATUS.md`](../DEV_STATUS.md), Milestone 9):

| Ticket ID | Origin Umbrella Ticket | Target Crate(s) | Status | Capability Group & Scope |
|---|---|---|---|---|
| **Ticket 9.1.1** | Milestone 9 Foundation | [`embedag-core`](../crates/embedag-core/src/lib.rs)<br>[`embedag-matcher`](../crates/embedag-matcher/src/lib.rs)<br>[`crates/embedag`](../crates/embedag/src/lib.rs) | **In Review (`[~]`)**<br>*(implemented on branch `feat/dendrite-dag-api`, tests pass)* | **Dendrite-Facing Core API**: Slim facade features `core,matcher` exposing `embedag::dag` ([`toposort`](../crates/embedag-core/src/resolve.rs#L21), [`levels`](../crates/embedag-core/src/resolve.rs#L83), [`reverse_deps`](../crates/embedag-core/src/resolve.rs#L156), [`critical_path`](../crates/embedag-core/src/resolve.rs#L187)), `embedag::dag::merkle` ([`MerkleNode`](../crates/embedag-core/src/merkle.rs#L14), [`diff`](../crates/embedag-core/src/merkle.rs#L137)), and `embedag-matcher` ([`Tree`](../crates/embedag-matcher/src/tree.rs), [`AstMatcher`](../crates/embedag-matcher/src/ast.rs), [`SimilarityGraph`](../crates/embedag-matcher/src/graph.rs)). |
| **Ticket 9.2.1** | `DEN-051`<br>*(Go side `pkg/pool/dag`)* | [`embedag-core`](../crates/embedag-core/src/lib.rs) | **Planned / In Backlog (`[ ]`)**<br>*(Dendrite: `To Verify`)* | **Multi-Directory Dependency DAG Solver**: Ingestion of multi-directory manifest models, cyclic hazard detection, and deterministic topological build order for `dendrite pool`. |
| **Ticket 9.2.2** | `DEN-074`<br>*(Go side `pkg/stories/dag`, `pkg/tracker`)* | [`embedag-core`](../crates/embedag-core/src/lib.rs) | **Planned / In Backlog (`[ ]`)**<br>*(Dendrite: `To Verify`)* | **Ticket–Milestone Dependency DAG Solver & Prioritizer**: Relational ingestion of ticket/milestone DAGs, unblock weight calculation with distance decay, and critical path scheduling. |
| **Ticket 9.3.1** | `DEN-068`<br>*(Go side `pkg/node/wal`, `pkg/store`)* | [`embedag-core`](../crates/embedag-core/src/lib.rs) | **Planned / In Backlog (`[ ]`)**<br>*(Dendrite: `To Do`)* | **Merkle Digest Verification Protocol**: State journal verification, Merkle delta diffing, and crash consistency validation for node WALs. |
| **Ticket 9.4.1** | `DEND-202` / Plane #107<br>*(Go side `pkg/agentsearch`, `pkg/rag`)* | [`embedag-rag`](../crates/embedag-rag/src/lib.rs) | **Planned / In Backlog (`[ ]`)**<br>*(Dendrite: `To Do`)* | **Leiden Community Clustering & God-Node Index**: Graph community detection, god-node centrality scoring, and dependency subgraphs for repository intelligence. |

---

## 7. Imported Docs

The following documentation files from Dendrite substantially describe `embedag`'s responsibilities and have been recorded in [`doc/imports.json`](imports.json):

1. `docs/architecture/2026-10-quality-audit-execution-plan.md` (`Dendrite:docs/architecture/2026-10-quality-audit-execution-plan.md`): Defines the authoritative 5-sub-project decomposition (Pillar 9, §5), sidecar JSON-RPC interface contracts, extraction sequence, and dogfood E2E verification proof.
2. `docs/features/FEAT-029-autonomous-pool-dag-synchronization.md` (`Dendrite:docs/features/FEAT-029-autonomous-pool-dag-synchronization.md`): Details the multi-directory pool DAG solver, typestate dependency mode transitions, and the automated Stories/Tickets/Milestones DAG Connector (`FEAT-DAG-CONNECTOR`).
3. `docs/features/FEAT-004-repo-intel-ast.md` (`Dendrite:docs/features/FEAT-004-repo-intel-ast.md`): Specifies the AST repository intelligence architecture, bounded subgraphs, and Leiden community clustering.
4. `docs/features/FEAT-022-agent-search-and-capability-discovery.md` (`Dendrite:docs/features/FEAT-022-agent-search-and-capability-discovery.md`): Outlines hybrid search with Reciprocal Rank Fusion (RRF) and Graphify AST code-knowledge coupling for agent capability routing.
5. `docs/features/FEAT-026-distributed-codebase-ecosystem.md` (`Dendrite:docs/features/FEAT-026-distributed-codebase-ecosystem.md`): Defines the 3-tier capability resolution hierarchy ($P_1 > P_2 > P_3$), prioritizing user-owned repositories (`semdag`/`embedag`) and curated high-performance OSS (`turso`, `diesel`, `diesel-cte-ext`).
6. `docs/features/FEAT-040-turso-offline-cache-outbox-sync.md` (`Dendrite:docs/features/FEAT-040-turso-offline-cache-outbox-sync.md`): Specifies offline-first embedded Turso (libSQL) storage in WAL mode with transactional outbox queueing.
7. `docs/cli/pool.md` (`Dendrite:docs/cli/pool.md`): Documents the CLI command contract for `dendrite pool` (`add`, `rm`, `list`, `resolve`).

---

## 8. Open Questions & Boundary Conflicts

1. **Sidecar Wrapper Boundary vs. Direct Library Consumption**:
   - Dendrite's decomposition plan (`2026-10-quality-audit-execution-plan.md` §5) places `dendrite-core-rs` and `dendrite-rag-rs` as sibling directories under `~/GoLandProjects/` shipping stdio JSON-RPC sidecar binaries.
   - `embedag` resides in `~/RustRoverProjects/embedag` and acts as a shared Rust library.
   - *Resolution*: `embedag` maintains pure library crates; `dendrite-core-rs` and `dendrite-rag-rs` act as thin RPC adapter wrappers that expose `embedag`'s algorithms over JSON-RPC 2.0 to Go.
2. **Go Package Naming Drift (`pkg/tracker/dag` vs. `pkg/stories/dag`)**:
   - Dendrite roadmap tickets (DEN-074) reference `pkg/stories/dag`, whereas the working Go implementation resides in `pkg/tracker/dag/dag.go` (`Dendrite:pkg/tracker/dag/dag.go`) and `pkg/tracker/dag/types.go` (`Dendrite:pkg/tracker/dag/types.go`).
   - *Resolution*: `embedag::dag::resolve` replaces both the working `pkg/tracker/dag` and the planned `pkg/stories/dag`.
3. **Tracking Topology (Plane v2 vs. Local `DEV_STATUS.md`)**:
   - Dendrite tickets are authoritatively tracked in Plane v2 (`http://localhost:8090`). `embedag` has no standalone Plane project and tracks Milestone 9 locally in [`DEV_STATUS.md`](../DEV_STATUS.md) (with `TICKET_LOG.md` symlink).
   - *Resolution*: Umbrella tickets `DEN-051`, `DEN-074`, `DEN-068`, and `DEND-202` remain in Plane for the Go control plane side; child implementations in `embedag` must be synchronized manually or via `dendrite plane replicate` to prevent state drift.
4. **WAL Format Parity**:
   - Dendrite's planned WAL journal in `pkg/node/wal` (DEN-068) must align with `embedag_core::merkle`'s hash format (`blake3("l" || key || content)` and `blake3("b" || key || children)`).
   - *Resolution*: The JSON-RPC contract in `proto/` must define the canonical wire serialization format for Merkle trees so that Go state transitions and Rust Merkle proofs remain byte-identical.
5. **Turso / libSQL Persistence Boundary**:
   - Dendrite uses `go-libsql` for local CLI ticket caching (`.dendrite/cache/dendrite.db`, FEAT-040).
   - `embedag` uses `diesel-libsql` and `libsql 0.9` for relational knowledge graph persistence and recursive CTE queries (`embedag-diesel`).
   - *Boundary*: `embedag` does **not** manage the Dendrite CLI's internal ticket cache; its Turso support is dedicated to relational graph indexing and vector storage.
