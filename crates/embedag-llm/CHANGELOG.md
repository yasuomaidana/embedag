# Changelog

All notable changes to this crate are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0]

### Added

- `ExtractionDirective` (`MatchExisting`, `InsertNode`, `CreateRootNode`, `AttachEdge`, `RetargetNode`), `ExtractionEnvelope`, JSON Schema generators (`generate_extraction_schema`, `generate_schema_for`), and recursive `$ref` inlining (`dereference_schema`) (Ticket 4.1.1).
- JSON Schema-to-GBNF grammar compiler (`compile_schema_to_gbnf`), stack-based `PushdownAutomaton` character/token JSON validator, `PromptFormatter`, and `ProvenanceNotes` (Ticket 4.1.2).
- `LlmProvider` async trait and `CandleProvider`, `OxillamaProvider` (with `oxillama` GBNF validation), `OxibonsaiProvider`, and `CloudProvider` implementations (Ticket 4.1.3).
- 3-tier `WaterfallPipeline` combining normalized `AliasStore` exact lookup (Tier 1), dense vector similarity threshold matching (Tier 2), and `LlmProvider` constrained inference fallback (Tier 3) (Ticket 4.2.1).
- `ChangesetDiffEngine` for previewing `GraphDiff` node/edge additions and removals prior to DAG commit (Ticket 4.3.2).
- `SemanticDeltaSynthesizer`, `DeltaSynthesizerConfig`, and `ScalarRatioRule` for deterministic scalar-ratio packaging hierarchy detection and LLM semantic delta synthesis (Ticket 4.3.3).
- Generic `EntityExtractor`, `EntityExtractorConfig`, `EntityProperties`, and declarative extraction macros (`extract_entity_text!`, `entity_properties!`, `define_entity_extractor!`).
