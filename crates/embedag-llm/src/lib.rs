//! Constrained LLM extraction, schema derivation, oxillama & candle inference and multi-tiered waterfall resolution.

pub mod delta_synthesizer;
pub mod diff;
pub mod gbnf;
pub mod prompt;
pub mod provider;
pub mod schema;
pub mod waterfall;

pub use delta_synthesizer::{
    AttributeDelta, CandidateEntity, CandidateItem, IncomingEntity, IncomingItem,
    SemanticDeltaSummary, SemanticDeltaSynthesizer,
};
pub use diff::{ChangesetDiffEngine, GraphDiff};
pub use gbnf::{GbnfError, PushdownAutomaton, compile_schema_to_gbnf, dereference_schema};
pub use prompt::{PromptError, PromptFormatter, ProvenanceNotes};
pub use provider::{
    CandleProvider, CloudProvider, LlmError, LlmProvider, ModelMetadata, OxibonsaiProvider,
    OxillamaProvider,
};
pub use schema::{
    ExtractionDirective, ExtractionEnvelope, generate_extraction_schema, generate_schema_for,
};
pub use waterfall::{AliasStore, CandidateMatch, ResolutionOutcome, WaterfallPipeline};
