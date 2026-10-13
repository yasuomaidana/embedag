//! Constrained LLM extraction, schema derivation, oxillama & candle inference and multi-tiered waterfall resolution.

pub mod delta_synthesizer;
pub mod diff;
pub mod extractor;
pub mod gbnf;
pub mod prompt;
pub mod provider;
pub mod schema;
pub mod waterfall;

pub use async_trait::async_trait;
pub use delta_synthesizer::{
    AttributeDelta, CandidateEntity, CandidateItem, DeltaSynthesizerConfig, IncomingEntity,
    IncomingItem, ScalarRatioRule, SemanticDeltaSummary, SemanticDeltaSynthesizer,
};
pub use diff::{ChangesetDiffEngine, GraphDiff};
#[allow(deprecated)]
pub use extractor::{
    extract_entity_text_from_value, extract_incoming_entity_text,
    extract_incoming_entity_text_with_config, extract_incoming_entity_text_with_keys,
    is_heuristic_entity_key, EntityExtractor, EntityExtractorConfig, EntityProperties,
    DEFAULT_ENTITY_KEYS,
};
pub use gbnf::{compile_schema_to_gbnf, dereference_schema, GbnfError, PushdownAutomaton};
pub use prompt::{PromptError, PromptFormatter, ProvenanceNotes};
pub use provider::{
    CandleProvider, CloudProvider, LlmError, LlmProvider, ModelMetadata, OxibonsaiProvider,
    OxillamaProvider,
};
pub use schema::{
    generate_extraction_schema, generate_schema_for, ExtractionDirective, ExtractionEnvelope,
};
pub use waterfall::{AliasStore, CandidateMatch, ResolutionOutcome, WaterfallPipeline};
