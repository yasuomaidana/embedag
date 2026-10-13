//! Native OxiRS TDB2 Persistent Storage Engine for embedag.
//!
//! Provides pure-Rust, zero-daemon graph persistence backed by OxiRS RDF store,
//! append-only N-Quads durability (`data.nq`), fast memory-mapped snapshots
//! (`snapshot.oxsnap`), SPARQL 1.1/1.2 query execution, and seamless `MemoryDag`
//! synchronization.

pub mod config;
pub mod engine;
pub mod error;
pub mod mapping;
pub mod repository;

pub use config::{OxiRsTdbConfig, StorageMode, SyncPolicy};
pub use engine::OxiRsTdbEngine;
pub use error::OxiRsStorageError;
pub use mapping::{SchemaVocab, make_graph_name, make_node_iri};
pub use oxirs_core::rdf_store::{OxirsQueryResults, QueryResults};
