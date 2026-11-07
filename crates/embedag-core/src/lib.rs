//! `embedag-core`: Core domain contracts, in-memory DAG representations, and deterministic encoders.
//!
//! # Overview
//! `embedag-core` provides zero-database, zero-heavy-runtime domain abstractions
//! for directed acyclic graph (DAG) multi-modal knowledge representations.
//!
//! # Groups
//! - [`graph`]: the arena, traversal, strongly connected components, resolution and pagination.
//! - [`change`]: mutation plans, schema migrations and Merkle diffing.
//! - [`contract`]: typed errors, core value types and the traits ports implement.
//!
//! # Modules
//! Every module below is also reachable at the crate root.
//! - [`dag`]: In-memory directed acyclic graph arena backed by `petgraph`.
//! - [`dag_iter`]: Pull-based streaming traversal iterators.
//! - [`pagination`]: Topological degree pagination contracts and types for high-degree hubs.
//! - [`encoder`]: Deterministic mathematical feature encoders for numbers, time, geography, and hybrid distances.
//! - [`error`]: Typed domain and cycle errors.
//! - [`export`]: Visual graph exporters (Mermaid, Graphviz DOT) and interchange formats (GraphML, JSON-LD).
//! - [`merkle`]: Content-addressed Merkle trees and delta diffing for change detection.
//! - [`migration`]: Declarative graph schema migrations and reparenting transformations.
//! - [`model`]: Core value types including edges, classification targets, and media blobs.
//! - [`mutation`]: Typestate mutation plans and atomic graph modification directives.
//! - [`resolve`]: Deterministic DAG dependency resolution, topological sorting, levels, reverse dependencies, and critical paths.
//! - [`scc`]: Strongly connected components (iterative Tarjan) and acyclic condensation for index graphs.
//! - `testing`: a plain node payload for tests (feature `testing`).
//! - [`similarity`]: Cosine ranking over small in-memory embedding indexes.
//! - [`traits`]: Contracts for graph ingestion ([`AsGraphContext`]), node payloads ([`NodePayload`]), and capability storage ports.

#![warn(missing_docs)]

#[doc(hidden)]
pub use async_trait;
#[doc(hidden)]
pub use serde_json;
pub mod change;
pub mod contract;
pub mod encoder;
pub mod export;
pub mod graph;
mod macros;
pub mod similarity;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

// The modules keep their short paths (`embedag_core::dag`, `embedag_core::traits`, ...).
pub use change::{merkle, migration, mutation};
pub use contract::{error, model, traits};
pub use graph::{dag, dag_iter, pagination, resolve, scc};

// Re-exports for clean prelude access
pub use dag::MemoryDag;
pub use dag_iter::{AncestorsIter, DescendantsIter, TraversalIter};
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
pub use pagination::{NeighborPage, NeighborPageRequest};
pub use scc::{Condensation, condense, strongly_connected_components};
pub use traits::{
    AsGraphContext, DynNodePayload, GraphRepository, NodePayload, ReadOnlyGraphRepository,
    TransactionalGraphRepository, VectorSearchRepository,
};

/// Compiles the Rust examples of the crate README as doctests, so they cannot drift from the code.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
