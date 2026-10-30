//! Error types for native OxiRS TDB2 storage engine.

use embedag_core::error::DagError;
use oxirs_core::OxirsError;
use thiserror::Error;

/// Storage errors arising from native OxiRS TDB2 operations.
#[derive(Debug, Error)]
pub enum OxiRsStorageError {
    /// OxiRS core database engine failure (RDF store, parser, serializer, SPARQL).
    #[error("OxiRS engine error: {0}")]
    Oxirs(#[from] OxirsError),

    /// DAG cycle or topological consistency violation.
    #[error("Graph topology violation: {0}")]
    Dag(#[from] DagError),

    /// JSON serialization or deserialization failure.
    #[error("Serialization failure: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Invalid IRI string formatting.
    #[error("Invalid IRI '{0}': {1}")]
    InvalidIri(String, String),

    /// Snapshot or mmap failure.
    #[error("Snapshot failure: {0}")]
    Snapshot(String),

    /// Node with given key not found in persistent store.
    #[error("Node not found for key: {0}")]
    NodeNotFound(String),

    /// Invariant or corrupted graph state.
    #[error("Topology corruption: {0}")]
    CorruptedState(String),

    /// Standard I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

embedag_core::impl_storage_error_into_dag!(OxiRsStorageError);
