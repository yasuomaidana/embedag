# embedag-matcher

`embedag-matcher` provides language-agnostic structural similarity matching and thresholded similarity graph construction over generic labeled trees (`Tree`). It solves the problem of detecting structurally similar syntax trees, configuration hierarchies, or document outlines across items without relying on identifier names or language-specific parsers, and grouping matching items into connected components (`SimilarityGraph`).

## When to use it

Use `embedag-matcher` when you already have a parser (such as `syn`, `tree-sitter`, or a JSON/YAML loader) that can project domain items into a `Tree` of `{ label, children }`, and you want:

- Identifier-insensitive structural similarity scores in `[0.0, 1.0]` using bottom-up BLAKE3 subtree hashing and node-mass-weighted Jaccard similarity (`AstMatcher`).
- A custom pairwise similarity metric implemented via the `Matcher` trait.
- Thresholded undirected similarity graphs (`SimilarityGraph`) with score-sorted neighbor queries and connected-component cluster discovery.

Do not use `embedag-matcher` to parse raw source code directly (it deliberately contains no language grammar or lexer) or to enforce acyclic dependency ordering (use [`embedag-core`](https://docs.rs/embedag-core) and `MemoryDag<T>` for directed acyclic graphs).

## Main types and functions

- `Tree`: A rose-tree node with `kind: String`, `label: Option<String>`, and `children: Vec<Tree>`.
  - `Tree::new(kind, label, children)`, `Tree::branch(kind, children)`, `Tree::leaf(kind, label)`.
  - `Tree::size(&self) -> usize`: Total node count (`1 + sum(child.size())`).
  - `Tree::subtree_hashes(&self, min_size: usize) -> Vec<(u64, usize)>`: Bottom-up structural hashes using the first 8 bytes of BLAKE3 in little-endian order. Critically, node `label` values are **not** fed into the structural hash—only `kind` and ordered child structural hashes—making the hash invariant under identifier and literal renaming. Subtrees smaller than `min_size` are excluded from the returned multiset.
- `Matcher`: Trait for pairwise similarity strategies (`fn score(&self, a: &Tree, b: &Tree) -> f32`).
- `AstMatcher`: Implements `Matcher` using multiset weighted Jaccard similarity where each structural hash is weighted by its subtree node count (`size`).
  - `AstMatcher::new(min_subtree: usize)` (clamped to at least `1`) and `AstMatcher::default()` (`min_subtree = 3`).
- `SimilarityGraph`: Undirected `petgraph::graph::UnGraph<String, f32>` built over `(id, Tree)` pairs.
  - `SimilarityGraph::build(&[(String, Tree)], &M, threshold: f32) -> Self`: Adds an edge for every pair with `score >= threshold`.
  - `neighbors(&self, id: &str) -> Vec<(String, f32)>`: Neighbors sorted by descending similarity score, then ascending ID.
  - `clusters(&self) -> Vec<Vec<String>>`: Connected components sorted lexicographically by member ID.

### Example

```rust
use embedag_matcher::{AstMatcher, Matcher, SimilarityGraph, Tree};

fn main() {
    let fn_alpha = Tree::branch("fn", vec![Tree::branch("block", vec![
        Tree::branch("binop", vec![Tree::leaf("ident", "x"), Tree::leaf("ident", "y")]),
    ])]);
    let fn_beta = Tree::branch("fn", vec![Tree::branch("block", vec![
        Tree::branch("binop", vec![Tree::leaf("ident", "lhs"), Tree::leaf("ident", "rhs")]),
    ])]);
    let leaf_only = Tree::leaf("const", "42");

    let matcher = AstMatcher::new(3);
    assert_eq!(matcher.score(&fn_alpha, &fn_beta), 1.0);
    assert_eq!(matcher.score(&fn_alpha, &leaf_only), 0.0);

    let items = vec![
        ("alpha".to_string(), fn_alpha),
        ("beta".to_string(), fn_beta),
        ("const".to_string(), leaf_only),
    ];
    let graph = SimilarityGraph::build(&items, &matcher, 0.8);
    assert_eq!(graph.neighbors("alpha"), vec![("beta".to_string(), 1.0)]);
    assert_eq!(
        graph.clusters(),
        vec![
            vec!["alpha".to_string(), "beta".to_string()],
            vec!["const".to_string()],
        ]
    );
}
```

## Feature flags

`embedag-matcher` defines no Cargo feature flags.

## Workspace relationship

Within the workspace, `embedag-matcher` depends only on `embedag-core`, `petgraph`, `blake3`, and `thiserror`, and is re-exported by [`embedag`](https://docs.rs/embedag) under the `matcher` feature (`embedag::matcher`).


## Build and test

```bash
cargo build -p embedag-matcher
cargo test -p embedag-matcher
```

## Specifications

- [`doc/22_Dependency_Resolution_and_Merkle_Delta_API.md`](https://github.com/yasuomaidana/embedag/blob/main/doc/22_Dependency_Resolution_and_Merkle_Delta_API.md) — Section 3 ("`embedag-matcher`: Structural AST Similarity & Similarity Graphs") specifies `Tree`, `Matcher`, `AstMatcher`, and `SimilarityGraph`.
