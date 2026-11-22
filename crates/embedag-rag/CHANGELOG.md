# Changelog

All notable changes to this crate are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed
- BREAKING: `OxiRagEngine` (module `oxirag_engine`) is replaced by the pure-Rust `VerifiedRagEngine` (module `verified_engine`): lexical retrieval, RRF fusion and an answer cited as `[n]` from the retrieved chunks. A query no chunk supports returns `RagError::VerificationFailure`. The `oxirag` dependency (relicensed to BUSL-1.1) is removed (EMBEDAG-119).

## [0.1.0]

### Added

- `ReciprocalRankFusion` (RRF) ranker with configurable smoothing constant `k` (default `60.0`) and deterministic tie-breaking, plus `InMemoryHybridIndex` and `HybridRagEngine` (Ticket 6.1.1).
- `SubgraphExpander` performing multi-hop ancestor and descendant expansion with exponential hop-distance decay (`gamma`), edge-confidence path products, and node-budget caps (`max_expanded_nodes`) (Ticket 6.2.1, Ticket 7.1.3).
- `LeidenClusterer` implementing modularity-based community detection and deterministic `CommunitySummary` generation over `MemoryDag<T>` (Ticket 6.2.2).
- `EvidenceSynthesizer` for token-budgeted markdown context assembly with full `Root > ... > Node` ancestor lineage paths (Ticket 6.3.1).
- `OxiRagUnifiedEngine` integrating `HybridRagEngine`, `LeidenClusterer`, `EvidenceSynthesizer`, and `oxirag::prelude::VerificationPipeline` speculative verification (Ticket 6.3.2).
