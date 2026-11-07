pub mod config;
pub mod engine;
pub mod error;
pub mod mapping;
pub mod repository;

pub use config::{Neo4jConfig, StorageMode};
pub use engine::Neo4jEngine;
pub use error::Neo4jStorageError;
pub use mapping::{Neo4jEdgeRecord, Neo4jNodeRecord};

/// Compiles the Rust examples of the crate README as doctests, so they cannot drift from the code.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
