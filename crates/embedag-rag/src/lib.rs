//! Two-stage hybrid Graph-RAG query engine with RRF, topological distance decay,
//! and grounded citation evidence packaging.
//!
//! Traceability: `doc/17_Hybrid_Vector_and_Lexical_Retrieval.md` and
//! `doc/18_Topological_Subgraph_Expansion_and_GraphRAG.md`.

pub mod community;
pub mod evidence;
pub mod expansion;
pub mod index;
pub mod oxirag_engine;
pub mod rrf;

pub use community::{CommunityDetector, CommunityPartition};
pub use evidence::GroundedEvidencePackager;
pub use expansion::{
    ExpandedEdge, ExpandedNode, ExpandedSubgraph, SeedRelationship, SubgraphExpander,
};
pub use index::{DenseVectorIndex, IndexError, LexicalDocument, SparseLexicalIndex};
pub use oxirag_engine::{OxiRagEngine, RagError};
pub use rrf::{RrfCandidate, RrfRanker};

/// Compiles the Rust examples of the crate README as doctests, so they cannot drift from the code.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
