# Changelog

All notable changes to this crate are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0]

### Added

- Language-agnostic `Tree` rose-tree representation with `new`, `branch`, `leaf`, `size`, and bottom-up BLAKE3 structural `subtree_hashes` invariant under label renaming (EMBEDAG-78 / Ticket 9.1.3).
- Generic `Matcher` trait and `AstMatcher` implementing node-mass-weighted multiset Jaccard similarity with configurable `min_subtree` threshold (EMBEDAG-78 / Ticket 9.1.3).
- `SimilarityGraph` builder over `(String, M::Item)` pairs with threshold validation (`[0.0, 1.0]`), score-sorted `neighbors`, and connected-component `clusters` discovery (EMBEDAG-78 / Ticket 9.1.3).
