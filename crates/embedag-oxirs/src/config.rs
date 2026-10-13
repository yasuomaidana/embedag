//! Configuration options for OxiRS TDB2 storage engine.

use std::path::{Path, PathBuf};
pub use oxirs_core::rdf_store::SyncPolicy;

/// Storage mode for OxiRS TDB2 engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageMode {
    /// In-memory transient store (ideal for unit testing and fast ephemeral graphs).
    InMemory,
    /// Durable disk persistence via native N-Quads append log and mmap snapshot (`snapshot.oxsnap`).
    Persistent(PathBuf),
}

/// Configuration settings for [`OxiRsTdbEngine`].
///
/// Follows the developer-defined abstraction pattern: developers have 100% control
/// over base URI namespaces, persistence directories, sync policies, and named graphs.
#[derive(Debug, Clone)]
pub struct OxiRsTdbConfig {
    /// Storage mode (in-memory or persistent disk path).
    pub mode: StorageMode,

    /// Base IRI prefix for nodes (e.g. `"urn:embedag:node:"` or `"https://domain.org/nodes/"`).
    pub base_iri: String,

    /// Schema namespace URI for predicates (e.g. `"https://embedag.dev/ns#"`).
    pub schema_ns: String,

    /// Optional named graph IRI (if `None`, quads are stored in default graph).
    pub named_graph: Option<String>,

    /// Fsync durability policy for persistent appends.
    pub sync_policy: SyncPolicy,
}

impl Default for OxiRsTdbConfig {
    fn default() -> Self {
        Self {
            mode: StorageMode::InMemory,
            base_iri: "urn:embedag:node:".to_string(),
            schema_ns: "https://embedag.dev/ns#".to_string(),
            named_graph: None,
            sync_policy: SyncPolicy::default(),
        }
    }
}

impl OxiRsTdbConfig {
    /// Creates a persistent storage configuration rooted at `path`.
    pub fn persistent(path: impl AsRef<Path>) -> Self {
        Self {
            mode: StorageMode::Persistent(path.as_ref().to_path_buf()),
            ..Default::default()
        }
    }

    /// Creates an in-memory storage configuration.
    pub fn in_memory() -> Self {
        Self {
            mode: StorageMode::InMemory,
            ..Default::default()
        }
    }

    /// Sets the base node IRI prefix.
    pub fn with_base_iri(mut self, base_iri: impl Into<String>) -> Self {
        self.base_iri = base_iri.into();
        self
    }

    /// Sets the schema namespace URI.
    pub fn with_schema_ns(mut self, schema_ns: impl Into<String>) -> Self {
        self.schema_ns = schema_ns.into();
        self
    }

    /// Sets an optional named graph URI.
    pub fn with_named_graph(mut self, graph: impl Into<String>) -> Self {
        self.named_graph = Some(graph.into());
        self
    }

    /// Sets the fsync sync policy.
    pub fn with_sync_policy(mut self, policy: SyncPolicy) -> Self {
        self.sync_policy = policy;
        self
    }

    /// Returns `true` if this configuration targets persistent disk storage.
    pub fn is_persistent(&self) -> bool {
        matches!(self.mode, StorageMode::Persistent(_))
    }

    /// Returns the dataset directory path, if persistent.
    pub fn dataset_path(&self) -> Option<&Path> {
        match &self.mode {
            StorageMode::Persistent(p) => Some(p.as_path()),
            StorageMode::InMemory => None,
        }
    }
}
