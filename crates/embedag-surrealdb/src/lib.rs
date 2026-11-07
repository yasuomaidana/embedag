pub mod config;
pub mod engine;
pub mod error;
pub mod mapping;
pub mod repository;

pub use config::{StorageMode, SurrealDbConfig};
pub use engine::SurrealDbEngine;
pub use error::SurrealDbStorageError;
pub use mapping::{SurrealEdgeRecord, SurrealNodeRecord};

/// Compiles the Rust examples of the crate README as doctests, so they cannot drift from the code.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
