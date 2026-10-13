//! # `embedag`
//!
//! Deterministic hybrid Graph-RAG, in-memory directed acyclic graph (DAG) arenas,
//! mixed-modality encoding, relational persistence, and constrained generative retrieval.
//!
//! ## Overview
//!
//! `embedag` provides an end-to-end framework for deterministic graph operations and RAG:
//! - **Core Domain & DAG Arena**: In-memory DAG representation with cycle detection and mixed-modality distance metrics.
//! - **Procedural Macros**: `#[derive(AsGraphContext)]` and `#[derive(GraphTarget)]` for domain structs.
//! - **Constrained LLM Extraction**: Pushdown automata token masking, GBNF grammars, and waterfall resolution.
//! - **Relational Persistence**: PostgreSQL with `pgvector` and SQLite with recursive CTE traversals.
//! - **Two-Stage Hybrid Graph-RAG**: Reciprocal Rank Fusion (RRF), bounded topological subgraph expansion, and grounded citation synthesis.
//!
//! ## Quick Start
//!
//! ```rust
//! use embedag::prelude::*;
//!
//! #[derive(Debug, Clone, PartialEq, Eq)]
//! struct Category {
//!     id: String,
//!     name: String,
//! }
//!
//! impl NodePayload for Category {
//!     type Key = String;
//!     fn key(&self) -> &Self::Key { &self.id }
//!     fn name(&self) -> &str { &self.name }
//!     fn entity_level(&self) -> &str { "Category" }
//! }
//!
//! let mut dag = MemoryDag::new();
//! dag.upsert_node(Category { id: "CAT-1".into(), name: "Dairy".into() });
//! assert_eq!(dag.node_count(), 1);
//! ```

#[cfg(feature = "core")]
pub use embedag_core as core;

#[cfg(feature = "derive")]
pub use embedag_derive as derive;

#[cfg(feature = "llm")]
pub use embedag_llm as llm;

#[cfg(feature = "diesel")]
pub use embedag_diesel as persistence;

#[cfg(feature = "rag")]
pub use embedag_rag as rag;

#[cfg(feature = "oxirs-tdb")]
pub use embedag_oxirs as oxirs;

// Top-level re-exports from core
#[cfg(feature = "core")]
pub use embedag_core::{
    dag::MemoryDag,
    error::DagError,
    model::{EdgePayload, GraphTarget, MultimodalBlob, ProvenanceKind},
    mutation::{Committed, CycleValidated, MutationDirective, MutationPlan, Unverified},
    traits::{
        AsGraphContext, NodePayload, ReadOnlyGraphRepository, TransactionalGraphRepository,
        VectorSearchRepository,
    },
};

// Derive macros
#[cfg(feature = "derive")]
pub use embedag_derive::{AsGraphContext, GraphTarget};

// LLM re-exports
#[cfg(feature = "llm")]
#[allow(deprecated)]
pub use embedag_llm::{
    define_entity_extractor, entity_properties, extract_entity_text, extract_entity_text_from_value,
    extract_incoming_entity_text, extract_incoming_entity_text_with_config,
    extract_incoming_entity_text_with_keys, EntityExtractor, EntityExtractorConfig,
    EntityProperties, ExtractionDirective, ExtractionEnvelope, LlmError, LlmProvider,
    ModelMetadata, OxibonsaiProvider, OxillamaProvider, DEFAULT_ENTITY_KEYS,
};

// RAG re-exports
#[cfg(feature = "rag")]
pub use embedag_rag::{
    evidence::GroundedEvidencePackager,
    expansion::{ExpandedNode, ExpandedSubgraph, SeedRelationship, SubgraphExpander},
    index::{DenseVectorIndex, SparseLexicalIndex},
    oxirag_engine::{OxiRagEngine, RagError},
    rrf::{RrfCandidate, RrfRanker},
};

// OxiRS TDB2 storage re-exports
#[cfg(feature = "oxirs-tdb")]
pub use embedag_oxirs::{
    OxiRsStorageError, OxiRsTdbConfig, OxiRsTdbEngine, OxirsQueryResults, QueryResults,
    StorageMode, SyncPolicy,
};

/// Ergonomic prelude for common imports across embedag applications.
pub mod prelude {
    #[cfg(feature = "core")]
    pub use embedag_core::{
        dag::MemoryDag,
        error::DagError,
        model::{EdgePayload, GraphTarget, MultimodalBlob, ProvenanceKind},
        mutation::{MutationDirective, MutationPlan},
        traits::{AsGraphContext, NodePayload},
    };

    #[cfg(feature = "derive")]
    pub use embedag_derive::{AsGraphContext, GraphTarget};

    #[cfg(feature = "llm")]
    pub use embedag_llm::{
        define_entity_extractor, entity_properties, extract_entity_text, EntityExtractor,
        EntityExtractorConfig, EntityProperties, ExtractionDirective, ExtractionEnvelope,
        LlmError, LlmProvider, OxibonsaiProvider, OxillamaProvider,
    };

    #[cfg(feature = "rag")]
    pub use embedag_rag::{
        evidence::GroundedEvidencePackager,
        expansion::{ExpandedSubgraph, SubgraphExpander},
        index::{DenseVectorIndex, SparseLexicalIndex},
        oxirag_engine::{OxiRagEngine, RagError},
        rrf::{RrfCandidate, RrfRanker},
    };

    #[cfg(feature = "oxirs-tdb")]
    pub use embedag_oxirs::{OxiRsTdbConfig, OxiRsTdbEngine, StorageMode, SyncPolicy};
}
