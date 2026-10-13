//! Error types for Neo4j OpenCypher Bolt storage adapter.

use embedag_core::error::DagError;
use thiserror::Error;

/// Error variants encountered during Neo4j / Bolt graph storage operations.
#[derive(Debug, Error)]
pub enum Neo4jStorageError {
    /// Neo4j Bolt driver or transport failure.
    #[error("Neo4j Bolt driver error: {0}")]
    Driver(#[from] neo4rs::Error),

    /// Graph topology or acyclicity violation.
    #[error("Graph topology violation: {0}")]
    Dag(#[from] DagError),

    /// JSON serialization or deserialization failure.
    #[error("JSON serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Business key is invalid.
    #[error("Invalid business key: {0}")]
    InvalidKey(String),

    /// Requested node was not found in graph database.
    #[error("Node not found: {0}")]
    NodeNotFound(String),

    /// Connection failure or invalid Bolt endpoint.
    #[error("Bolt connection failure: {0}")]
    Connection(String),

    /// Generic adapter error.
    #[error("Neo4j adapter error: {0}")]
    Other(String),
}

impl From<Neo4jStorageError> for DagError {
    fn from(err: Neo4jStorageError) -> Self {
        match err {
            Neo4jStorageError::Dag(e) => e,
            Neo4jStorageError::NodeNotFound(key) => DagError::NodeNotFound { key },
            other => DagError::StorageError(other.to_string()),
        }
    }
}
