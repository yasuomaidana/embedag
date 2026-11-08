# embedag-llm

`embedag-llm` provides JSON Schema derivation, GBNF context-free grammar compilation, token-level pushdown automaton validation, multi-tiered waterfall entity resolution, and semantic delta synthesis for the `embedag` workspace. It solves the problem of turning unstructured text and domain records into schema-validated graph mutation directives (`ExtractionDirective`) while short-circuiting expensive inference calls through deterministic alias lookups and dense vector similarity thresholds.

## When to use it

Use `embedag-llm` when you need to:

- Derive JSON Schemas (`schemars::Schema`) for graph mutation directives or custom `GraphTarget` types (`generate_extraction_schema`, `generate_schema_for`) and compile them into BNF/GBNF production rules (`compile_schema_to_gbnf`).
- Enforce character- and token-level JSON structural validity during generation via `PushdownAutomaton`.
- Resolve incoming raw strings against known entities using the 3-tier `WaterfallPipeline` (Tier 1 exact normalized `AliasStore` lookup -> Tier 2 dense cosine similarity threshold -> Tier 3 `LlmProvider` constrained inference).
- Synthesize semantic parent-child or equivalence relationships (`SemanticDeltaSynthesizer`) or preview the structural diff a directive would produce against a `MemoryDag` (`ChangesetDiffEngine`).

Do not use `embedag-llm` expecting out-of-the-box remote HTTP calls to OpenAI/Anthropic or full neural weight loading in v0.1.0; see the implementation status notes below.

### Implemented vs. planned capabilities

Per [`doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md`](../../doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md) (§2):

- **Local providers (`CandleProvider`, `OxillamaProvider`, `OxibonsaiProvider`)**: Currently evaluate prompts using deterministic token-overlap and candidate-matching heuristics (`src/provider.rs`) validated step-by-step through `PushdownAutomaton`, or return a caller-configured `simulated_directive` (`with_simulated_response`). Under the `oxillama` feature, `OxillamaProvider::validate_gbnf_grammar` compiles GBNF grammars via `oxillama::grammar::Grammar::from_gbnf`.
- **`CloudProvider`**: Currently a configuration stub that returns `LlmError::ProviderFailure` when `extract_directive` is called without a simulated response (`src/provider.rs:412`). A real OpenAI-compatible HTTP `JsonSchemaHttpProvider` (`/v1/chat/completions` with `response_format: { type: "json_schema" }`) is planned in Ticket 10.3.1.
- **Merchant normalization and kNN classification**: Deterministic merchant-descriptor cleaning (Ticket 10.2.1) and weighted kNN majority-vote categorization in `WaterfallPipeline` (Ticket 10.2.2) are planned in Milestone 10.

## Main types and functions

- **Schema and grammar (`schema`, `gbnf`)**:
  - `ExtractionDirective`: Tagged enum (`InsertNode`, `InsertMultiParentNode`, `UpdateAttributes`, `RetargetEdge`, `CreateRootNode`, `MarkAmbiguous`).
  - `ExtractionEnvelope`: Pairs an `ExtractionDirective` with a `chain_of_thought` reasoning scratchpad string.
  - `generate_extraction_schema()` and `generate_schema_for<T: JsonSchema>()`: Produce JSON Schema `serde_json::Value` definitions.
  - `dereference_schema(&Value)` and `compile_schema_to_gbnf(&Value)`: Inline `$ref`/`$defs` and compile JSON Schema objects into GBNF rules (`root ::= ...`).
  - `PushdownAutomaton`: Stack-based character/token validator checking balanced braces, brackets, and string escapes (`from_schema`, `can_accept_token`, `mask_candidates`, `consume`, `is_complete`).
- **Prompts and providers (`prompt`, `provider`)**:
  - `PromptFormatter::build_prompt` and `PromptFormatter::build_prompt_full` (`ProvenanceNotes`).
  - `LlmProvider` async trait (`generate_structured`, `model_info`, `extract_directive`).
  - `CandleProvider`, `OxillamaProvider`, `OxibonsaiProvider`, `CloudProvider`.
- **Waterfall resolution and synthesis (`waterfall`, `delta_synthesizer`, `diff`, `extractor`)**:
  - `WaterfallPipeline<P>`, `AliasStore`, `CandidateMatch`, `ResolutionOutcome` (`Tier1DirectMatch`, `Tier2NearMatch`, `Tier2NovelEntity`, `Tier3LlmResolved`, `Tier4HumanReview`).
  - `SemanticDeltaSynthesizer<P>`, `DeltaSynthesizerConfig`, `ScalarRatioRule`, `CandidateEntity`, `IncomingEntity`.
  - `ChangesetDiffEngine::diff_directive` and `GraphDiff`.
  - `EntityExtractor<P>`, `EntityExtractorConfig`, `EntityProperties`, and macros `extract_entity_text!`, `entity_properties!`, `define_entity_extractor!`.

### Example

```rust
use embedag_llm::gbnf::{PushdownAutomaton, compile_schema_to_gbnf};
use embedag_llm::provider::CandleProvider;
use embedag_llm::schema::generate_extraction_schema;
use embedag_llm::waterfall::{CandidateMatch, ResolutionOutcome, WaterfallPipeline};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let schema = generate_extraction_schema();
    assert!(compile_schema_to_gbnf(&schema)?.contains("root ::="));

    let mut pda = PushdownAutomaton::from_schema(&schema)?;
    pda.consume(r#"{"chain_of_thought":"ok","directive":{"action":"create_root_node","entity_name":"Food","entity_level":"root","confidence":0.95}}"#)?;
    assert!(pda.is_complete());

    let pipeline = WaterfallPipeline::new(CandleProvider::new("phi-3-mini"));
    pipeline.alias_store.insert_alias("AMZN MKTPLACE", "merch_amazon");
    let tier1 = pipeline.resolve("amzn mktplace", &[]).await?;
    assert_eq!(tier1, ResolutionOutcome::Tier1DirectMatch { target_key: "merch_amazon".into(), confidence: 1.0 });

    let candidates = vec![CandidateMatch { key: "merch_target".into(), similarity_score: 0.92 }];
    let tier2 = pipeline.resolve("TARGET STORE 0192", &candidates).await?;
    assert_eq!(tier2, ResolutionOutcome::Tier2NearMatch { target_key: "merch_target".into(), similarity: 0.92 });
    Ok(())
}
```

## Feature flags

| Feature | Default | Description |
|---|---|---|
| `candle` | Yes | Enables optional `dep:candle-core` tensor/device integration |
| `oxillama` | Yes | Enables `dep:oxillama` GBNF grammar validation in `OxillamaProvider::validate_gbnf_grammar` |

## Workspace relationship

Within the workspace:

- `embedag-llm` depends on [`embedag-core`](../embedag-core/README.md) (`MemoryDag`, `MutationDirective`, `EdgePayload`, `GraphContext`).
- [`embedag-example-receipts`](../embedag-example-receipts/README.md) uses `embedag-llm` (`PromptFormatter`, `LlmProvider`, `CandleProvider`, `WaterfallPipeline`, `SemanticDeltaSynthesizer`, `ChangesetDiffEngine`) to categorize receipts and invoices into hierarchical taxonomies.
- [`embedag`](../embedag/README.md) re-exports `embedag-llm` under `embedag::llm` and `embedag::prelude`.


## Build and test

```bash
cargo build -p embedag-llm
cargo test -p embedag-llm
```

## Specifications

- [`doc/10_Structured_Extraction_and_Schema_Derivation.md`](../../doc/10_Structured_Extraction_and_Schema_Derivation.md) — `ExtractionDirective`, `schemars` derivation, and `$ref` dereferencing.
- [`doc/11_Pure_Rust_LLM_Inference_and_Constrained_Decoding.md`](../../doc/11_Pure_Rust_LLM_Inference_and_Constrained_Decoding.md) — GBNF compilation, `PushdownAutomaton`, and `LlmProvider` backends.
- [`doc/12_Multi_Tiered_Waterfall_Resolution.md`](../../doc/12_Multi_Tiered_Waterfall_Resolution.md) — 3-tier `WaterfallPipeline` and `AliasStore`.
- [`doc/14_Dynamic_Graph_Mutations_and_Diffing.md`](../../doc/14_Dynamic_Graph_Mutations_and_Diffing.md) — `ChangesetDiffEngine` and `SemanticDeltaSynthesizer`.
- [`doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md`](../../doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md) — Current provider audit (§2), planned `JsonSchemaHttpProvider` (Ticket 10.3.1), and kNN waterfall tier (Ticket 10.2.2).
