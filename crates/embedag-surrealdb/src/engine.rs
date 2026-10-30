//! High-level SurrealDB storage engine adapter for embedag.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::Arc;

use embedag_core::dag::MemoryDag;
use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_core::mutation::dag_directives;
use embedag_core::similarity::rank_by_cosine;
use embedag_core::traits::NodePayload;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb::opt::auth::Root;
use surrealdb::types::SurrealValue;

use crate::config::{StorageMode, SurrealDbConfig};
use crate::error::SurrealDbStorageError;
use crate::mapping::{SurrealEdgeRecord, SurrealNodeRecord};

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct EdgeLinkRow {
    in_key: String,
    out_key: String,
    relationship: Option<String>,
    confidence: Option<f32>,
    provenance: Option<String>,
}

/// High-performance native SurrealDB multi-model engine adapter.
///
/// Features:
/// - Supports embedded in-process memory (`Mem`) and remote connections via WebSocket/HTTP.
/// - Native document storage and graph relations (`RELATE` / edge tables).
/// - Fast cosine vector search indexing.
/// - Seamless bidirectional import and export with [`MemoryDag`].
#[derive(Clone)]
pub struct SurrealDbEngine {
    db: Surreal<Any>,
    config: SurrealDbConfig,
    vector_index: Arc<RwLock<HashMap<String, Vec<f32>>>>,
}

impl SurrealDbEngine {
    /// Opens or connects to a SurrealDB instance according to the given configuration.
    pub async fn open(config: SurrealDbConfig) -> Result<Self, SurrealDbStorageError> {
        let endpoint = match &config.mode {
            StorageMode::InMemory => "mem://".to_string(),
            StorageMode::Remote(ep) => ep.clone(),
        };

        let db = surrealdb::engine::any::connect(&endpoint)
            .await
            .map_err(|e| SurrealDbStorageError::Connection(e.to_string()))?;

        if let (Some(user), Some(pass)) = (&config.username, &config.password) {
            db.signin(Root {
                username: user.clone(),
                password: pass.clone(),
            })
            .await?;
        }

        db.use_ns(&config.namespace)
            .use_db(&config.database)
            .await?;

        Ok(Self {
            db,
            config,
            vector_index: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Creates an in-memory transient SurrealDB engine for testing or ephemeral workflows.
    pub async fn in_memory() -> Result<Self, SurrealDbStorageError> {
        Self::open(SurrealDbConfig::in_memory()).await
    }

    /// Returns a reference to the active configuration.
    pub fn config(&self) -> &SurrealDbConfig {
        &self.config
    }

    /// Returns a reference to the underlying Surreal client instance.
    pub fn inner(&self) -> &Surreal<Any> {
        &self.db
    }

    /// Inserts or updates a domain node `T: NodePayload + Serialize`.
    pub async fn insert_node<T: NodePayload + Serialize>(
        &self,
        node: &T,
    ) -> Result<(), SurrealDbStorageError> {
        let key_str = node.key().to_string();
        let embedding = self.vector_index.read().get(&key_str).cloned();
        let record = SurrealNodeRecord::from_node_with_embedding(node, embedding)?;

        let _: Option<SurrealNodeRecord> = self
            .db
            .upsert((self.config.node_table.as_str(), key_str.as_str()))
            .content(record)
            .await?;

        Ok(())
    }

    /// Fetches a node by its business key.
    pub async fn get_node<T: NodePayload + DeserializeOwned>(
        &self,
        key: &T::Key,
    ) -> Result<Option<T>, SurrealDbStorageError> {
        let key_str = key.to_string();
        let record: Option<SurrealNodeRecord> = self
            .db
            .select((self.config.node_table.as_str(), key_str.as_str()))
            .await?;

        match record {
            Some(rec) => {
                let node = rec.to_node()?;
                Ok(Some(node))
            }
            None => Ok(None),
        }
    }

    /// Removes a node and any attached edges by its business key.
    pub async fn remove_node_by_key(&self, key: &str) -> Result<(), SurrealDbStorageError> {
        let _: Option<SurrealNodeRecord> = self
            .db
            .delete((self.config.node_table.as_str(), key))
            .await?;

        // Clean up connected edges
        let delete_edges_query = format!(
            "DELETE {} WHERE in_key = $key OR out_key = $key",
            self.config.edge_table
        );
        self.db
            .query(delete_edges_query)
            .bind(("key", key.to_string()))
            .await?;

        self.vector_index.write().remove(key);
        Ok(())
    }

    /// Inserts or replaces a directed graph edge (`parent -> child`).
    pub async fn insert_edge<K: Display>(
        &self,
        parent: &K,
        child: &K,
        edge: &EdgePayload,
    ) -> Result<(), SurrealDbStorageError> {
        let parent_key = parent.to_string();
        let child_key = child.to_string();
        let edge_rec = SurrealEdgeRecord::from_edge(edge);

        let query = format!(
            "CREATE {} CONTENT {{ \
                in_key: $parent, \
                out_key: $child, \
                relationship: $rel, \
                confidence: $conf, \
                provenance: $prov, \
                created_at: $created \
            }}",
            self.config.edge_table
        );

        self.db
            .query(query)
            .bind(("parent", parent_key))
            .bind(("child", child_key))
            .bind(("rel", edge_rec.relationship))
            .bind(("conf", edge_rec.confidence))
            .bind(("prov", edge_rec.provenance))
            .bind(("created", edge_rec.created_at))
            .await?;

        Ok(())
    }

    /// Removes directed graph edges between parent and child, optionally filtered by relationship.
    pub async fn remove_edge<K: Display>(
        &self,
        parent: &K,
        child: &K,
        relationship: Option<&str>,
    ) -> Result<(), SurrealDbStorageError> {
        let parent_key = parent.to_string();
        let child_key = child.to_string();

        let query = if let Some(rel) = relationship {
            format!(
                "DELETE {} WHERE in_key = $parent AND out_key = $child AND relationship = '{}'",
                self.config.edge_table, rel
            )
        } else {
            format!(
                "DELETE {} WHERE in_key = $parent AND out_key = $child",
                self.config.edge_table
            )
        };

        self.db
            .query(query)
            .bind(("parent", parent_key))
            .bind(("child", child_key))
            .await?;

        Ok(())
    }

    /// Fetches immediate parents of a node identified by business key.
    pub async fn get_parents<T: NodePayload + DeserializeOwned>(
        &self,
        child_key: &T::Key,
    ) -> Result<Vec<T>, SurrealDbStorageError> {
        self.neighbours(child_key.to_string(), true).await
    }

    /// Fetches immediate children of a node identified by business key.
    pub async fn get_children<T: NodePayload + DeserializeOwned>(
        &self,
        parent_key: &T::Key,
    ) -> Result<Vec<T>, SurrealDbStorageError> {
        self.neighbours(parent_key.to_string(), false).await
    }

    /// The nodes one edge away from `key`: its parents when `parents`, else its children.
    async fn neighbours<T: NodePayload + DeserializeOwned>(
        &self,
        key: String,
        parents: bool,
    ) -> Result<Vec<T>, SurrealDbStorageError> {
        let column = if parents { "out_key" } else { "in_key" };
        let query = format!(
            "SELECT in_key, out_key, relationship, confidence, provenance FROM {} WHERE {column} = $key",
            self.config.edge_table
        );

        let mut res = self.db.query(query).bind(("key", key)).await?;
        let rows: Vec<EdgeLinkRow> = res.take(0)?;

        let mut found = Vec::new();
        for row in rows {
            let other = if parents { &row.in_key } else { &row.out_key };
            let rec: Option<SurrealNodeRecord> = self
                .db
                .select((self.config.node_table.as_str(), other.as_str()))
                .await?;
            if let Some(r) = rec {
                found.push(r.to_node()?);
            }
        }

        Ok(found)
    }

    /// Indexes a dense vector embedding for a node key.
    pub fn index_embedding(&self, key: impl Into<String>, embedding: Vec<f32>) {
        self.vector_index.write().insert(key.into(), embedding);
    }

    /// Finds approximate nearest neighbors by cosine similarity across indexed embeddings.
    pub async fn find_nearest_neighbors<T: NodePayload + DeserializeOwned>(
        &self,
        query: &[f32],
        limit: usize,
    ) -> Result<Vec<(T, f32)>, SurrealDbStorageError> {
        let scored = rank_by_cosine(&self.vector_index.read(), query, limit);

        let mut results = Vec::with_capacity(scored.len());
        for (key_str, score) in scored {
            let rec: Option<SurrealNodeRecord> = self
                .db
                .select((self.config.node_table.as_str(), key_str.as_str()))
                .await?;
            if let Some(r) = rec {
                results.push((r.to_node()?, score));
            }
        }

        Ok(results)
    }

    /// Exports an in-memory [`MemoryDag<T>`] into the SurrealDB multi-model store.
    pub async fn export_dag<T: NodePayload + Serialize>(
        &self,
        dag: &MemoryDag<T>,
    ) -> Result<(), SurrealDbStorageError> {
        self.apply_directives(&dag_directives(dag)).await
    }

    /// Loads graph contents from SurrealDB into a fresh [`MemoryDag<T>`].
    pub async fn load_dag<T: NodePayload + DeserializeOwned>(
        &self,
    ) -> Result<MemoryDag<T>, SurrealDbStorageError> {
        let mut dag = MemoryDag::<T>::new();

        // 1. Fetch all nodes
        let query_nodes = format!("SELECT * FROM {}", self.config.node_table);
        let mut res_nodes = self.db.query(query_nodes).await?;
        let node_recs: Vec<SurrealNodeRecord> = res_nodes.take(0)?;

        let mut key_map = HashMap::new();
        for rec in node_recs {
            let node: T = rec.to_node()?;
            let key = node.key().clone();
            dag.upsert_node(node);
            key_map.insert(rec.business_key, key);
        }

        // 2. Fetch all edges
        let query_edges = format!("SELECT * FROM {}", self.config.edge_table);
        let mut res_edges = self.db.query(query_edges).await?;
        let edge_rows: Vec<EdgeLinkRow> = res_edges.take(0)?;

        for row in edge_rows {
            if let (Some(parent_key), Some(child_key)) =
                (key_map.get(&row.in_key), key_map.get(&row.out_key))
            {
                let relationship = row.relationship.unwrap_or_else(|| "RELATE".to_string());
                let confidence = row.confidence.unwrap_or(1.0);
                let provenance = match row.provenance.as_deref() {
                    Some("deterministic_rule") => ProvenanceKind::DeterministicRule,
                    Some("dense_similarity_match") => ProvenanceKind::DenseSimilarityMatch,
                    Some("llm_constrained_inference") => ProvenanceKind::LlmConstrainedInference,
                    _ => ProvenanceKind::HumanVerified,
                };

                let edge = EdgePayload::new(relationship, confidence, provenance)?;
                dag.add_edge_by_keys(parent_key, child_key, edge)?;
            }
        }

        Ok(dag)
    }
}
