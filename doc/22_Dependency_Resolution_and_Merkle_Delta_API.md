# Dependency Resolution & Merkle Delta API

| Metadata | Details |
| :--- | :--- |
| **Document ID** | `22_Dependency_Resolution_and_Merkle_Delta_API` |
| **Domain** | Dependency Scheduling, Change Detection, Structural Similarity |
| **Responsibility** | The slim, dependency-light API surface of embedag: deterministic DAG resolution (`toposort`, `levels`, `reverse_deps`, `critical_path`), BLAKE3 Merkle trees with structural deltas, and AST similarity matching. |
| **Target Crates** | `embedag-core`, `embedag-matcher`, `embedag` (facade) |
| **Status** | Definitive Architectural Specification |

---

## 1. Purpose

Many consumers need graph algorithms without databases, LLM engines or native drivers. Examples are build orchestrators ordering projects, schedulers prioritising work items, change detectors deciding what to re-process, and code tools clustering similar functions. This document specifies the subset of embedag that serves them:

- built with `default-features = false, features = ["core"]` or `["core", "matcher"]`,
- pulling in only `petgraph`, `blake3`, `thiserror`, `serde` and the domain types of `embedag-core`.

```toml
[dependencies]
embedag = { git = "https://github.com/yasuomaidana/embedag", default-features = false, features = ["core", "matcher"] }
```

## 2. Facade layout

| Path | Feature | Re-exports |
|---|---|---|
| `embedag::dag` | `core` | `MemoryDag`, `DagError`, `toposort`, `levels`, `reverse_deps`, `critical_path` |
| `embedag::dag::merkle` | `core` | `MerkleNode`, `Delta`, `diff` |
| `embedag::matcher` | `matcher` | `Tree`, `AstMatcher`, `Matcher`, `SimilarityGraph` |

## 3. Deterministic resolution (`embedag_core::resolve`)

All functions operate on a [`MemoryDag<T>`](05_In_Memory_DAG_and_Arena_Topology.md) whose edges point `parent → child` ("child depends on parent"). Every result is **deterministic**: whenever several nodes are equally eligible, the tie is broken by the `Display` string order of `T::Key`. Identical graphs always produce identical output, whatever order they were inserted in.

| Function | Result | Algorithm | Errors |
|---|---|---|---|
| `toposort(&dag)` | `Vec<Key>`: every parent before its children | Kahn's algorithm with an ordered ready set (`BTreeMap` keyed by the key's display string) | `CycleDetected(participants)` |
| `levels(&dag)` | `Vec<Vec<Key>>`: level 0 = roots; level *n* holds the nodes whose parents are all in levels < *n*; keys sorted within a level | Layered Kahn | `CycleDetected` |
| `reverse_deps(&dag, key)` | Transitive descendants of `key` in topological order (the rebuild set when `key` changes), excluding `key` | Transitive descendants, filtered through the topological order | `NodeNotFound`, `CycleDetected` |
| `critical_path(&dag, weight)` | `(total, path)`: the maximum-weight root-to-node path; `(0, [])` for an empty graph | DP over the topological order, `best[v] = weight(v) + max(best[parents])` | `CycleDetected` |

Typical uses:

- `levels` gives batches that can run in parallel,
- `reverse_deps` gives the impact set of a change,
- `critical_path` with a duration weight gives the schedule's bottleneck chain.

Cycle errors list the participating nodes, so callers can report the offending dependency loop.

## 4. Merkle trees and structural deltas (`embedag_core::merkle`)

### 4.1 Hashing
- **Leaf:** `blake3("l" ‖ key ‖ content)`
- **Branch:** `blake3("b" ‖ key ‖ (child.key ‖ child.hash)*)`, with children sorted by key

The prefixes keep leaves and branches from colliding. Sorting makes the root hash independent of insertion order. These byte layouts are the canonical wire format: any other implementation verifying embedag trees must reproduce them exactly.

### 4.2 Construction
- `MerkleNode::leaf(key, content)` and `MerkleNode::branch(key, children)` build trees explicitly.
- `MerkleNode::from_paths(root, [(path, bytes)])` builds a directory tree from `a/b/c.rs`-style relative paths. Empty path segments are ignored, and the result does not depend on iteration order. Callers that already have per-file digests can pass the digest as `bytes`, which avoids holding file contents in memory.

### 4.3 Diff
`diff(&old, &new) -> Delta { added, removed, changed }`. Paths are `/`-joined keys from the root.

Equal hashes prune whole subtrees, so the cost is proportional to the *changed* part of the tree, not its size. `changed` covers both content changes and leaf↔branch kind changes. `Delta::is_empty()` is the fast "nothing changed" check. An internal counting variant records how many nodes were visited, so the crate's tests assert the pruning.

Typical uses:

- incremental indexing: re-process only the `added` and `changed` paths,
- state-journal verification: compare the recorded root with the recomputed root,
- synchronisation: exchange roots, then descend only into differing subtrees.

## 5. Structural similarity (`embedag-matcher`)

- **`Tree`:** a language-agnostic labelled syntax tree. Any parser's output can be converted into it.
- **`AstMatcher`** (implements `Matcher`): structural similarity in `[0, 1]` as the weighted Jaccard index over the multisets of subtree structural hashes. Each subtree of at least `min_subtree` nodes contributes its hash, weighted by its size. Hashes cover shape, not labels, so renamed identifiers still score `1.0`, while structurally different trees score near `0.0`.
- **`SimilarityGraph::build(items, matcher, threshold)`:** an undirected weighted graph (`petgraph::UnGraph`) of the pairs scoring at least `threshold`. `neighbors(id)` lists similar items, and `clusters()` returns connected components: groups of duplicated or near-duplicated code.

## 6. Guarantees

- No I/O, no global state, no async runtime in this surface.
- All outputs are deterministic and stable across releases within a major version. A change to hashing layouts or tie-breaking is a breaking change.
- Planned extensions to this surface (DEV_STATUS Milestone 9):
  - SCC detection and condensation for possibly-cyclic input,
  - multi-root dependency manifests,
  - priority weights with distance decay,
  - Merkle journal verification,
  - Leiden community clustering for code-knowledge graphs.
