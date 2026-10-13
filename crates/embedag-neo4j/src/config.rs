//! Configuration options for Neo4j OpenCypher Bolt storage engine.

/// Storage mode for Neo4j adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageMode {
    /// In-memory transient mock engine for rapid unit tests and CI without Docker.
    InMemoryMock,
    /// Live Neo4j or Memgraph server instance connected via Bolt protocol.
    Bolt(String),
}

/// Runtime configuration for the Neo4j Bolt adapter.
#[derive(Debug, Clone)]
pub struct Neo4jConfig {
    /// Connection mode.
    pub mode: StorageMode,
    /// Bolt endpoint URI (e.g. `127.0.0.1:7687` or `bolt://127.0.0.1:7687`).
    pub uri: String,
    /// Database username.
    pub user: String,
    /// Database password.
    pub password: String,
    /// Database name (default `neo4j`).
    pub database: String,
    /// Neo4j Node label applied to all embedag nodes (default `EmbedagNode`).
    pub node_label: String,
    /// Default Cypher relationship type label (default `EMBEDAG_RELATION`).
    pub default_relation: String,
}

impl Default for Neo4jConfig {
    fn default() -> Self {
        Self {
            mode: StorageMode::InMemoryMock,
            uri: "127.0.0.1:7687".to_string(),
            user: "neo4j".to_string(),
            password: "password".to_string(),
            database: "neo4j".to_string(),
            node_label: "EmbedagNode".to_string(),
            default_relation: "EMBEDAG_RELATION".to_string(),
        }
    }
}

impl Neo4jConfig {
    /// Creates an in-memory mock configuration for deterministic local execution.
    pub fn in_memory() -> Self {
        Self::default()
    }

    /// Creates a Bolt connection configuration.
    pub fn bolt(
        uri: impl Into<String>,
        user: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        let uri_str = uri.into();
        Self {
            mode: StorageMode::Bolt(uri_str.clone()),
            uri: uri_str,
            user: user.into(),
            password: password.into(),
            ..Self::default()
        }
    }

    /// Sets the target database name.
    pub fn with_database(mut self, db: impl Into<String>) -> Self {
        self.database = db.into();
        self
    }

    /// Sets the node label.
    pub fn with_node_label(mut self, label: impl Into<String>) -> Self {
        self.node_label = label.into();
        self
    }

    /// Sets the default relationship label.
    pub fn with_default_relation(mut self, rel: impl Into<String>) -> Self {
        self.default_relation = rel.into();
        self
    }
}
