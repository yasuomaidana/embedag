//! Typed error definitions for `embedag-core`.
//!
//! Traceability: `doc/04_Core_Domain_and_Entity_Contracts.md`,
//! `doc/13_Topological_Cycle_Detection_and_Guards.md`,
//! `doc/07_Deterministic_Feature_Encoding.md`.

use thiserror::Error;

/// Errors raised by the in-memory DAG arena, mutation pipeline, and persistence repositories.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum DagError {
    /// A directed cycle exists (or would exist) in the graph.
    ///
    /// `node_indices` lists the arena indices of the nodes participating in the cycle
    /// (empty when the exact participants are unknown).
    #[error("cycle detected among node indices {node_indices:?}")]
    CycleDetected {
        /// Arena indices of the nodes forming the strongly connected component.
        node_indices: Vec<usize>,
    },

    /// A referenced node key does not exist in the arena.
    #[error("node with key `{key}` was not found")]
    NodeNotFound {
        /// Display representation of the missing key.
        key: String,
    },

    /// A referenced edge does not exist in the arena.
    #[error("edge `{parent}` -> `{child}` with relationship `{relationship}` was not found")]
    EdgeNotFound {
        /// Display representation of the parent key.
        parent: String,
        /// Display representation of the child key.
        child: String,
        /// Relationship label of the missing edge.
        relationship: String,
    },

    /// An edge confidence value outside `[0.0, 1.0]` (or `NaN`) was supplied.
    #[error("edge confidence {0} is outside the closed interval [0.0, 1.0]")]
    InvalidConfidence(f32),

    /// A validated mutation plan was committed against a DAG that changed after validation.
    #[error(
        "mutation plan was validated at revision {validated} but the DAG is at revision {current}"
    )]
    StalePlan {
        /// Revision observed during validation.
        validated: u64,
        /// Revision observed at commit time.
        current: u64,
    },

    /// A persistent storage or driver failure occurred.
    #[error("storage error: {0}")]
    StorageError(String),
}

/// Errors raised by deterministic feature encoders and distance metrics.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum EncodingError {
    /// H3 resolution outside `0..=15`.
    #[error("invalid H3 resolution {0}; expected 0..=15")]
    InvalidResolution(u8),

    /// Latitude/longitude pair is not a valid WGS84 coordinate.
    #[error("invalid coordinates (lon: {lon}, lat: {lat})")]
    InvalidCoordinates {
        /// Longitude in degrees.
        lon: f64,
        /// Latitude in degrees.
        lat: f64,
    },

    /// A `u64` value is not a valid H3 cell index.
    #[error("invalid H3 cell index {0:#x}")]
    InvalidCellIndex(u64),

    /// Two H3 cells cannot be compared on a local grid (different resolution or too far apart).
    #[error("grid distance between cells {a:#x} and {b:#x} is undefined: {reason}")]
    GridDistanceUndefined {
        /// First cell.
        a: u64,
        /// Second cell.
        b: u64,
        /// Underlying reason.
        reason: String,
    },

    /// Two dense vectors have different dimensionality.
    #[error("vector dimension mismatch: {left} vs {right}")]
    DimensionMismatch {
        /// Dimension of the left vector.
        left: usize,
        /// Dimension of the right vector.
        right: usize,
    },

    /// Modality weights are negative, non-finite or sum to zero.
    #[error("invalid modality weights: {0}")]
    InvalidWeights(String),
}
