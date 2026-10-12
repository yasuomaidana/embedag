//! `embedag-core`: Core domain contracts, in-memory DAG representations, and deterministic encoders.
//!
//! # Overview
//! `embedag-core` provides zero-database, zero-heavy-runtime domain abstractions
//! for directed acyclic graph (DAG) multi-modal knowledge representations.
//!
//! # Modules
//! - [`dag`]: In-memory directed acyclic graph arena backed by `petgraph`.
//! - [`encoder`]: Deterministic mathematical feature encoders for numbers, time, geography, and hybrid distances.
//! - [`error`]: Typed domain and cycle errors.
//! - [`export`]: Visual graph exporters (Mermaid, Graphviz DOT) and interchange formats (GraphML, JSON-LD).
//! - [`migration`]: Declarative graph schema migrations and reparenting transformations.
//! - [`model`]: Core value types including edges, classification targets, and media blobs.
//! - [`mutation`]: Typestate mutation plans and atomic graph modification directives.
//! - [`traits`]: Contracts for graph ingestion ([`AsGraphContext`]), node payloads ([`NodePayload`]), and capability storage ports.

#![warn(missing_docs)]

pub mod dag;
pub mod encoder;
pub mod error;
pub mod export;
pub mod migration;
pub mod model;
pub mod mutation;
pub mod traits;

// Re-exports for clean prelude access
pub use dag::MemoryDag;
pub use encoder::{
    CyclicalTimeEncoder, GeospatialEncoder, ModalityWeights, ScalarScaling,
    categorical_tag_distance, composite_distance, cosine_distance, encode_cyclical_time,
    encode_linear, encode_log, encode_standard_score, lexical_token_distance,
    scalar_gower_distance, temporal_distance, temporal_distance_opt,
};
#[cfg(feature = "h3o")]
pub use encoder::{h3_distance, h3_encode, h3_normalized_distance};
pub use error::{DagError, EncodingError};
pub use export::GraphExporter;
pub use migration::{GraphMigration, MigrationStep};
pub use model::{EdgePayload, FeatureMap, GraphTarget, MultimodalBlob, ProvenanceKind, TagMap};
pub use mutation::{Committed, CycleValidated, MutationDirective, MutationPlan, Unverified};
pub use traits::{
    AsGraphContext, DynNodePayload, GraphRepository, NodePayload, ReadOnlyGraphRepository,
    TransactionalGraphRepository, VectorSearchRepository,
};
