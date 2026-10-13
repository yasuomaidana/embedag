//! Configuration options for SurrealDB multi-model storage engine.

/// Mode of storage connection for SurrealDB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageMode {
    /// In-process transient memory engine (`Mem`).
    InMemory,
    /// Remote SurrealDB instance connected via WebSocket or HTTP.
    Remote(String),
}

/// Runtime configuration for the SurrealDB storage engine.
#[derive(Debug, Clone)]
pub struct SurrealDbConfig {
    /// Underlying storage backend mode.
    pub mode: StorageMode,
    /// Target namespace.
    pub namespace: String,
    /// Target database.
    pub database: String,
    /// Table name for graph nodes / entities.
    pub node_table: String,
    /// Table/relation name for graph edges.
    pub edge_table: String,
    /// Optional username for authentication.
    pub username: Option<String>,
    /// Optional password for authentication.
    pub password: Option<String>,
}

impl Default for SurrealDbConfig {
    fn default() -> Self {
        Self {
            mode: StorageMode::InMemory,
            namespace: "embedag".to_string(),
            database: "knowledge_graph".to_string(),
            node_table: "node".to_string(),
            edge_table: "graph_edge".to_string(),
            username: None,
            password: None,
        }
    }
}

impl SurrealDbConfig {
    /// Creates an in-memory embedded configuration.
    pub fn in_memory() -> Self {
        Self::default()
    }

    /// Creates a remote configuration connecting to a SurrealDB server endpoint.
    pub fn remote(endpoint: impl Into<String>) -> Self {
        Self {
            mode: StorageMode::Remote(endpoint.into()),
            ..Self::default()
        }
    }

    /// Sets the target namespace.
    pub fn with_namespace(mut self, ns: impl Into<String>) -> Self {
        self.namespace = ns.into();
        self
    }

    /// Sets the target database.
    pub fn with_database(mut self, db: impl Into<String>) -> Self {
        self.database = db.into();
        self
    }

    /// Sets the table name for nodes.
    pub fn with_node_table(mut self, table: impl Into<String>) -> Self {
        self.node_table = table.into();
        self
    }

    /// Sets the relation/edge table name.
    pub fn with_edge_table(mut self, table: impl Into<String>) -> Self {
        self.edge_table = table.into();
        self
    }

    /// Sets authentication credentials.
    pub fn with_credentials(mut self, user: impl Into<String>, pass: impl Into<String>) -> Self {
        self.username = Some(user.into());
        self.password = Some(pass.into());
        self
    }
}
