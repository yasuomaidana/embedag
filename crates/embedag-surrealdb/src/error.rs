//! Error types for SurrealDB storage adapter.

use embedag_core::error::DagError;
use thiserror::Error;

/// Error variants encountered during SurrealDB multi-model storage operations.
#[derive(Debug, Error)]
pub enum SurrealDbStorageError {
    /// SurrealDB driver or engine error.
    #[error("SurrealDB driver error: {0}")]
    Surreal(#[from] surrealdb::Error),

    /// Graph topology or acyclicity violation.
    #[error("Graph topology violation: {0}")]
    Dag(#[from] DagError),

    /// JSON serialization or deserialization failure.
    #[error("JSON serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Business key is invalid or cannot be converted to a record ID.
    #[error("Invalid business key: {0}")]
    InvalidKey(String),

    /// Requested node was not found in database.
    #[error("Node not found: {0}")]
    NodeNotFound(String),

    /// Connection failure or invalid endpoint.
    #[error("Database connection failure: {0}")]
    Connection(String),

    /// Generic adapter error.
    #[error("SurrealDB adapter error: {0}")]
    Other(String),
}

impl From<SurrealDbStorageError> for DagError {
    fn from(err: SurrealDbStorageError) -> Self {
        match err {
            SurrealDbStorageError::Dag(e) => e,
            SurrealDbStorageError::NodeNotFound(key) => DagError::NodeNotFound { key },
            other => DagError::StorageError(other.to_string()),
        }
    }
}
