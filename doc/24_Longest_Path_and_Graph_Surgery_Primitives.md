# Longest-Path & Graph Surgery Primitives

## 1. Purpose

This document specifies graph primitives for callers that need timing analysis over a directed acyclic graph and that modify the graph while it is in use: a longest-path pass with slack, two weights per node, subgraph replacement, range fingerprints for set reconciliation, and an optional capacity-token check. All are pure graph or set algorithms with no domain vocabulary. No tickets exist yet; this is the requirement and its acceptance criteria.

## 2. Assessment of the Current Implementation

1. `embedag-core` offers `MemoryDag`, cycle detection (`graph/scc.rs`), and dependency resolution (`graph/resolve.rs`). There is no forward/backward longest-path pass and no slack.
2. Node weights, where used, are single values. A consumer that distinguishes two components of a node's weight must keep a parallel structure.
3. There is no operation that replaces a connected set of nodes with another subgraph while preserving the edges entering and leaving the set.
4. `change/merkle.rs` builds a tree from paths and diffs two trees. It rebuilds on every change and cannot fingerprint a range of an ordered set, which range-based reconciliation between two replicas needs. (Incremental updates are already requested in document 23.)
5. No primitive models bounded tokens on ordered pairs of node classes.

## 3. Requirements

| ID | Priority | Crate | Requirement |
|---|---|---|---|
| `PATH-1` | high | `embedag-core` | Longest-path forward and backward pass with integer weights, per-node lower bounds, and an upper bound; returns earliest/latest start and finish and slack per node |
| `PATH-2` | high | `embedag-core` | Two weights per node (active, passive) and both totals along the longest path |
| `SURG-1` | high | `embedag-core` | `replace_subgraph`: swap a node set for a subgraph, preserving boundary edges, validated acyclic, atomic |
| `SYNC-1` | medium | `embedag-core` | Range fingerprints over an ordered set of `(key, digest)` and a two-party reconciliation routine |
| `TOKEN-1` | low | `embedag-core` | Bounded tokens on ordered class pairs with a static circular-wait check |

## 4. Specifications

### PATH-1: Longest-path pass with slack

**Request:** Given a DAG, a non-negative integer duration per node, an optional lower bound per node, a start value, and an upper bound, compute in O(V + E):
- forward: `es(u) = max(start, lower(u), max ef(pred))`, `ef(u) = es(u) + d(u)`;
- backward: `lf(u) = min(upper, min ls(succ))`, `ls(u) = lf(u) - d(u)`;
- `slack(u) = ls(u) - es(u)` as a signed integer.

Iteration order within a topological level must be deterministic (by node key). A cycle returns an error naming one cycle.

**Acceptance criteria:**
- Results match a reference table for a five-node converging graph (documented in the test).
- For random DAGs: `ef` is monotone in any duration; minimum slack is non-negative iff the latest `ef` is at or below the upper bound.
- Two runs on equal inputs produce identical outputs, independent of insertion order.
- No floating point.

### PATH-2: Active and passive weights

**Request:** Each node may expose `(active, passive)`; duration is their sum. The pass additionally returns one longest path and the sums of active and passive weights along it.

**Acceptance criteria:** active + passive along the returned path equals the path length; with all passive weights zero, results equal PATH-1.

### SURG-1: Subgraph replacement

**Request:** `replace_subgraph(removed: &[NodeKey], replacement: SubgraphSpec, entry_map, exit_map)`. Edges from outside into the removed set are re-attached to the replacement nodes named in `entry_map`; edges from the removed set to outside are re-attached from the nodes named in `exit_map`. The operation validates before mutating: every boundary edge is mapped, the result is acyclic, no outside node is left depending on a removed node. On any failure the graph is unchanged.

**Acceptance criteria:**
- After success: no edge references a removed node; every former external successor of the removed set has at least one predecessor in the replacement or outside.
- After failure: the graph equals its prior state (checked by digest).
- Property test: random DAG, random connected removed set, random replacement DAG → result is acyclic or the call fails cleanly.

### SYNC-1: Range fingerprints and reconciliation

**Request:** An ordered container of `(key, digest)` supporting insert in O(log n) and `fingerprint(range)` in O(log n), where the fingerprint is a collision-resistant combination of the digests in the range; and a protocol routine that, given two parties' containers and a message channel abstraction, determines the keys each side lacks by recursive range comparison.

**Acceptance criteria:**
- Two containers with equal contents have equal fingerprints for every range, regardless of insertion order.
- For sets differing by `d` items out of `n`, messages exchanged are O(d · log n).
- The routine is transport-agnostic and has no notion of what an item is.

### TOKEN-1: Bounded tokens on class pairs (optional)

**Request:** Given a mapping from nodes to classes and a token count per ordered class pair, provide acquire/release bookkeeping and a static check that the class-pair graph admits no circular wait; if it does, report the cycle.

**Acceptance criteria:** tokens in use plus free equals the total at all times; an acyclic class-pair graph passes the check; a two-class back-and-forth configuration is reported.
