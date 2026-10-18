<!-- Imported from Dendrite docs/features/FEAT-004-repo-intel-ast.md @ 69ea048 on 2026-10-18. Source of truth for this repo's scope; keep in sync with the Dendrite copy until the extraction umbrella lands. -->

---
id: FEAT-004
title: Repository Intelligence & AST Knowledge Graph Architecture
status: approved
plane_epic: PROJ-E-04
dependencies: [FEAT-001]
repository: dendrite
tags: [knowledge-graph, ast, tree-sitter, graphify, mcp]
---

# FEAT-004: Repository Intelligence & AST Knowledge Graph Architecture

## 1. User Story & Business Objective
As a software engineer, I need autonomous agents to query precise call-graphs, symbol definitions, and blast-radii rather than performing brute-force recursive file greps, reducing context prompt token consumption by 50% to 80% and preventing hallucinated cross-file refactoring errors.

---

## 2. Technical Boundaries & Specifications

1. **Graphify Engine**:
   - Installed via `uv tool install graphifyy[mcp]` on execution nodes.
   - Extracts AST using Tree-sitter grammars (Go, Rust, TypeScript, Python).
   - Generates Leiden community clusters and dependency subgraphs.
   - Persists state in local SQLite (`graphify.db`).
2. **Repository Dissection Tooling**:
   - `ast-grep`: Structural pattern searching and code rewriting.
   - `ripgrep`: Ultra-fast literal token scans.
   - `git-sizer` & `scc`/`tokei`: Structural complexity, LOC, and repository bloat metrics.

---

## 3. Acceptance Criteria & Verification Matrix

- [x] **AC-1**: Graphify MCP queries (`query_blast_radius`, `get_caller_callee`) return bounded subgraphs in $< 200\text{ms}$.
- [x] **AC-2**: AST indexing executes entirely offline on the local node with zero telemetry or cloud transmission.
- [x] **AC-3**: Analysis jobs calculate cyclomatic and cognitive complexity metrics for every function in the target codebase.

---

## 4. Definition of Done (DoD) & Persistence Verification

- **Automated Tests**: Unit tests assert graph traversal correctness across circular dependency graphs.
- **Persistent Layer Verification**: Graphify SQLite database schemas and migrations tested against persistent stores using Testcontainers.
- **Complexity Metrics**: Parser and traversal methods adhere strictly to cyclomatic complexity $\le 10$.
