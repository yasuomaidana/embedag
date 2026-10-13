//! High-level Neo4j OpenCypher Bolt storage engine adapter for embedag.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::Arc;

use embedag_core::dag::MemoryDag;
use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_core::traits::NodePayload;
use parking_lot::RwLock;
use petgraph::visit::EdgeRef;
use serde::{Serialize, de::DeserializeOwned};

use crate::config::{Neo4jConfig, StorageMode};
use crate::error::Neo4jStorageError;
use crate::mapping::{Neo4jEdgeRecord, Neo4jNodeRecord};

#[derive(Clone, Default)]
struct MockStore {
    nodes: Arc<RwLock<HashMap<String, Neo4jNodeRecord>>>,
    edges: Arc<RwLock<Vec<(String, String, Neo4jEdgeRecord)>>>,
}

/// High-performance native Neo4j OpenCypher Bolt engine adapter.
///
/// Features:
/// - OpenCypher property graph model (`(p)-[:RELATION]->(c)`).
/// - Bolt protocol integration via `neo4rs` and zero-daemon in-memory mock mode.
/// - Exact cosine similarity vector search capability.
/// - Seamless bidirectional import and export with [`MemoryDag`].
#[derive(Clone)]
pub struct Neo4jEngine {
    config: Neo4jConfig,
    graph: Option<Arc<neo4rs::Graph>>,
    mock: Option<MockStore>,
    vector_index: Arc<RwLock<HashMap<String, Vec<f32>>>>,
}

impl Neo4jEngine {
    /// Opens or connects to a Neo4j instance according to the given configuration.
    pub async fn open(config: Neo4jConfig) -> Result<Self, Neo4jStorageError> {
        match &config.mode {
            StorageMode::InMemoryMock => Ok(Self {
                config,
                graph: None,
                mock: Some(MockStore::default()),
                vector_index: Arc::new(RwLock::new(HashMap::new())),
            }),
            StorageMode::Bolt(uri) => {
                let bolt_config = neo4rs::ConfigBuilder::default()
                    .uri(uri.as_str())
                    .user(config.user.as_str())
                    .password(config.password.as_str())
                    .db(config.database.as_str())
                    .build()
                    .map_err(|e| Neo4jStorageError::Connection(e.to_string()))?;

                let graph = neo4rs::Graph::connect(bolt_config)
                    .await
                    .map_err(|e| Neo4jStorageError::Connection(e.to_string()))?;

                Ok(Self {
                    config,
                    graph: Some(Arc::new(graph)),
                    mock: None,
                    vector_index: Arc::new(RwLock::new(HashMap::new())),
                })
            }
        }
    }

    /// Creates an in-memory transient mock Neo4j engine for testing and CI.
    pub async fn in_memory() -> Result<Self, Neo4jStorageError> {
        Self::open(Neo4jConfig::in_memory()).await
    }

    /// Returns a reference to the active configuration.
    pub fn config(&self) -> &Neo4jConfig {
        &self.config
    }

    /// Inserts or updates a domain node `T: NodePayload + Serialize`.
    pub async fn insert_node<T: NodePayload + Serialize>(
        &self,
        node: &T,
    ) -> Result<(), Neo4jStorageError> {
        let key_str = node.key().to_string();
        let embedding = self.vector_index.read().get(&key_str).cloned();
        let record = Neo4jNodeRecord::from_node_with_embedding(node, embedding)?;

        if let Some(ref mock) = self.mock {
            mock.nodes.write().insert(key_str, record);
            return Ok(());
        }

        if let Some(ref graph) = self.graph {
            let cypher = format!(
                "MERGE (n:`{}` {{ business_key: $key }}) \
                SET n.name = $name, \
                    n.entity_level = $level, \
                    n.attributes_json = $attr, \
                    n.payload_json = $payload",
                self.config.node_label
            );

            let q = neo4rs::query(&cypher)
                .param("key", record.business_key)
                .param("name", record.name)
                .param("level", record.entity_level)
                .param("attr", record.attributes_json)
                .param("payload", record.payload_json);

            graph.run(q).await?;
        }

        Ok(())
    }

    /// Fetches a node by its business key.
    pub async fn get_node<T: NodePayload + DeserializeOwned>(
        &self,
        key: &T::Key,
    ) -> Result<Option<T>, Neo4jStorageError> {
        let key_str = key.to_string();

        if let Some(ref mock) = self.mock {
            if let Some(rec) = mock.nodes.read().get(&key_str) {
                return Ok(Some(rec.to_node()?));
            }
            return Ok(None);
        }

        if let Some(ref graph) = self.graph {
            let cypher = format!(
                "MATCH (n:`{}` {{ business_key: $key }}) RETURN n.payload_json AS payload_json",
                self.config.node_label
            );

            let q = neo4rs::query(&cypher).param("key", key_str);
            let mut stream = graph.execute(q).await?;

            if let Ok(Some(row)) = stream.next().await
                && let Ok(payload_json) = row.get::<String>("payload_json")
            {
                let node: T = serde_json::from_str(&payload_json)?;
                return Ok(Some(node));
            }
        }

        Ok(None)
    }

    /// Removes a node and any attached relationships by its business key.
    pub async fn remove_node_by_key(&self, key: &str) -> Result<(), Neo4jStorageError> {
        if let Some(ref mock) = self.mock {
            mock.nodes.write().remove(key);
            mock.edges.write().retain(|(p, c, _)| p != key && c != key);
            self.vector_index.write().remove(key);
            return Ok(());
        }

        if let Some(ref graph) = self.graph {
            let cypher = format!(
                "MATCH (n:`{}` {{ business_key: $key }}) DETACH DELETE n",
                self.config.node_label
            );
            let q = neo4rs::query(&cypher).param("key", key);
            graph.run(q).await?;
            self.vector_index.write().remove(key);
        }

        Ok(())
    }

    /// Inserts or replaces a directed graph edge (`parent -> child`).
    pub async fn insert_edge<K: Display>(
        &self,
        parent: &K,
        child: &K,
        edge: &EdgePayload,
    ) -> Result<(), Neo4jStorageError> {
        let parent_key = parent.to_string();
        let child_key = child.to_string();
        let edge_rec = Neo4jEdgeRecord::from_edge(edge);

        if let Some(ref mock) = self.mock {
            let mut edges = mock.edges.write();
            edges.retain(|(p, c, r)| {
                !(p == &parent_key && c == &child_key && r.relationship == edge_rec.relationship)
            });
            edges.push((parent_key, child_key, edge_rec));
            return Ok(());
        }

        if let Some(ref graph) = self.graph {
            let rel_type = if edge_rec.relationship.is_empty() {
                self.config.default_relation.as_str()
            } else {
                edge_rec.relationship.as_str()
            };

            let cypher = format!(
                "MATCH (p:`{}` {{ business_key: $parent }}), (c:`{}` {{ business_key: $child }}) \
                MERGE (p)-[r:`{}`]->(c) \
                SET r.confidence = $conf, \
                    r.provenance = $prov, \
                    r.created_at = $created",
                self.config.node_label, self.config.node_label, rel_type
            );

            let q = neo4rs::query(&cypher)
                .param("parent", parent_key)
                .param("child", child_key)
                .param("conf", edge_rec.confidence as f64)
                .param("prov", edge_rec.provenance)
                .param("created", edge_rec.created_at);

            graph.run(q).await?;
        }

        Ok(())
    }

    /// Removes directed graph edges between parent and child, optionally filtered by relationship.
    pub async fn remove_edge<K: Display>(
        &self,
        parent: &K,
        child: &K,
        relationship: Option<&str>,
    ) -> Result<(), Neo4jStorageError> {
        let parent_key = parent.to_string();
        let child_key = child.to_string();

        if let Some(ref mock) = self.mock {
            let mut edges = mock.edges.write();
            edges.retain(|(p, c, r)| {
                let matches_pair = p == &parent_key && c == &child_key;
                if !matches_pair {
                    return true;
                }
                if let Some(rel) = relationship {
                    r.relationship != rel
                } else {
                    false
                }
            });
            return Ok(());
        }

        if let Some(ref graph) = self.graph {
            let cypher = if let Some(rel) = relationship {
                format!(
                    "MATCH (p:`{}` {{ business_key: $parent }})-[r:`{}`]->(c:`{}` {{ business_key: $child }}) DELETE r",
                    self.config.node_label, rel, self.config.node_label
                )
            } else {
                format!(
                    "MATCH (p:`{}` {{ business_key: $parent }})-[r]->(c:`{}` {{ business_key: $child }}) DELETE r",
                    self.config.node_label, self.config.node_label
                )
            };

            let q = neo4rs::query(&cypher)
                .param("parent", parent_key)
                .param("child", child_key);

            graph.run(q).await?;
        }

        Ok(())
    }

    /// Fetches immediate parents of a node identified by business key.
    pub async fn get_parents<T: NodePayload + DeserializeOwned>(
        &self,
        child_key: &T::Key,
    ) -> Result<Vec<T>, Neo4jStorageError> {
        let child_str = child_key.to_string();

        if let Some(ref mock) = self.mock {
            let edges = mock.edges.read();
            let nodes = mock.nodes.read();
            let mut parents = Vec::new();

            for (p, c, _) in edges.iter() {
                if c == &child_str
                    && let Some(n) = nodes.get(p)
                {
                    parents.push(n.to_node()?);
                }
            }
            return Ok(parents);
        }

        if let Some(ref graph) = self.graph {
            let cypher = format!(
                "MATCH (p:`{}`)-[r]->(c:`{}` {{ business_key: $child }}) RETURN p.payload_json AS payload_json",
                self.config.node_label, self.config.node_label
            );

            let q = neo4rs::query(&cypher).param("child", child_str);
            let mut stream = graph.execute(q).await?;
            let mut parents = Vec::new();

            while let Ok(Some(row)) = stream.next().await {
                if let Ok(payload_json) = row.get::<String>("payload_json") {
                    let node: T = serde_json::from_str(&payload_json)?;
                    parents.push(node);
                }
            }
            return Ok(parents);
        }

        Ok(Vec::new())
    }

    /// Fetches immediate children of a node identified by business key.
    pub async fn get_children<T: NodePayload + DeserializeOwned>(
        &self,
        parent_key: &T::Key,
    ) -> Result<Vec<T>, Neo4jStorageError> {
        let parent_str = parent_key.to_string();

        if let Some(ref mock) = self.mock {
            let edges = mock.edges.read();
            let nodes = mock.nodes.read();
            let mut children = Vec::new();

            for (p, c, _) in edges.iter() {
                if p == &parent_str
                    && let Some(n) = nodes.get(c)
                {
                    children.push(n.to_node()?);
                }
            }
            return Ok(children);
        }

        if let Some(ref graph) = self.graph {
            let cypher = format!(
                "MATCH (p:`{}` {{ business_key: $parent }})-[r]->(c:`{}`) RETURN c.payload_json AS payload_json",
                self.config.node_label, self.config.node_label
            );

            let q = neo4rs::query(&cypher).param("parent", parent_str);
            let mut stream = graph.execute(q).await?;
            let mut children = Vec::new();

            while let Ok(Some(row)) = stream.next().await {
                if let Ok(payload_json) = row.get::<String>("payload_json") {
                    let node: T = serde_json::from_str(&payload_json)?;
                    children.push(node);
                }
            }
            return Ok(children);
        }

        Ok(Vec::new())
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
    ) -> Result<Vec<(T, f32)>, Neo4jStorageError> {
        let scored = {
            let index = self.vector_index.read();
            let mut scored: Vec<(String, f32)> = Vec::with_capacity(index.len());

            let q_norm = query.iter().map(|v| v * v).sum::<f32>().sqrt();
            if q_norm == 0.0 {
                return Ok(Vec::new());
            }

            for (key, vec) in index.iter() {
                if vec.len() != query.len() {
                    continue;
                }
                let dot = query
                    .iter()
                    .zip(vec.iter())
                    .map(|(a, b)| a * b)
                    .sum::<f32>();
                let v_norm = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
                let score = if v_norm > 0.0 {
                    dot / (q_norm * v_norm)
                } else {
                    0.0
                };
                scored.push((key.clone(), score));
            }

            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            scored.truncate(limit);
            scored
        };

        let mut results = Vec::with_capacity(scored.len());
        for (key_str, score) in scored {
            if let Some(node) = self.get_node_by_string_key::<T>(&key_str).await? {
                results.push((node, score));
            }
        }

        Ok(results)
    }

    async fn get_node_by_string_key<T: NodePayload + DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Option<T>, Neo4jStorageError> {
        if let Some(ref mock) = self.mock {
            if let Some(rec) = mock.nodes.read().get(key) {
                return Ok(Some(rec.to_node()?));
            }
            return Ok(None);
        }

        if let Some(ref graph) = self.graph {
            let cypher = format!(
                "MATCH (n:`{}` {{ business_key: $key }}) RETURN n.payload_json AS payload_json",
                self.config.node_label
            );
            let q = neo4rs::query(&cypher).param("key", key);
            let mut stream = graph.execute(q).await?;
            if let Ok(Some(row)) = stream.next().await
                && let Ok(payload_json) = row.get::<String>("payload_json")
            {
                let node: T = serde_json::from_str(&payload_json)?;
                return Ok(Some(node));
            }
        }

        Ok(None)
    }

    /// Exports an in-memory [`MemoryDag<T>`] into the Neo4j graph database.
    pub async fn export_dag<T: NodePayload + Serialize>(
        &self,
        dag: &MemoryDag<T>,
    ) -> Result<(), Neo4jStorageError> {
        let inner = dag.inner();
        for idx in inner.node_indices() {
            self.insert_node(&inner[idx]).await?;
        }

        for edge_ref in inner.edge_references() {
            let source_node = &inner[edge_ref.source()];
            let target_node = &inner[edge_ref.target()];
            self.insert_edge(source_node.key(), target_node.key(), edge_ref.weight())
                .await?;
        }

        Ok(())
    }

    /// Loads graph contents from Neo4j into a fresh [`MemoryDag<T>`].
    pub async fn load_dag<T: NodePayload + DeserializeOwned>(
        &self,
    ) -> Result<MemoryDag<T>, Neo4jStorageError> {
        let mut dag = MemoryDag::<T>::new();

        if let Some(ref mock) = self.mock {
            let nodes = mock.nodes.read();
            let mut key_map = HashMap::new();

            for (k, rec) in nodes.iter() {
                let node: T = rec.to_node()?;
                let key = node.key().clone();
                dag.upsert_node(node);
                key_map.insert(k.clone(), key);
            }

            let edges = mock.edges.read();
            for (p, c, r) in edges.iter() {
                if let (Some(pk), Some(ck)) = (key_map.get(p), key_map.get(c)) {
                    let provenance = match r.provenance.as_str() {
                        "deterministic_rule" => ProvenanceKind::DeterministicRule,
                        "dense_similarity_match" => ProvenanceKind::DenseSimilarityMatch,
                        "llm_constrained_inference" => ProvenanceKind::LlmConstrainedInference,
                        _ => ProvenanceKind::HumanVerified,
                    };
                    let edge = EdgePayload::new(&r.relationship, r.confidence, provenance)?;
                    dag.add_edge_by_keys(pk, ck, edge)?;
                }
            }

            return Ok(dag);
        }

        if let Some(ref graph) = self.graph {
            let cypher_nodes = format!(
                "MATCH (n:`{}`) RETURN n.payload_json AS payload_json",
                self.config.node_label
            );
            let mut stream_nodes = graph.execute(neo4rs::query(&cypher_nodes)).await?;
            let mut key_map = HashMap::new();

            while let Ok(Some(row)) = stream_nodes.next().await {
                if let Ok(payload_json) = row.get::<String>("payload_json") {
                    let node: T = serde_json::from_str(&payload_json)?;
                    let key_str = node.key().to_string();
                    let key = node.key().clone();
                    dag.upsert_node(node);
                    key_map.insert(key_str, key);
                }
            }

            let cypher_edges = format!(
                "MATCH (p:`{}`)-[r]->(c:`{}`) RETURN p.business_key AS parent, c.business_key AS child, type(r) AS rel, r.confidence AS conf, r.provenance AS prov",
                self.config.node_label, self.config.node_label
            );
            let mut stream_edges = graph.execute(neo4rs::query(&cypher_edges)).await?;

            while let Ok(Some(row)) = stream_edges.next().await {
                if let (Ok(p), Ok(c)) = (row.get::<String>("parent"), row.get::<String>("child"))
                    && let (Some(pk), Some(ck)) = (key_map.get(&p), key_map.get(&c))
                {
                    let rel = row.get::<String>("rel").unwrap_or_else(|_| "REL".into());
                    let conf = row.get::<f64>("conf").unwrap_or(1.0) as f32;
                    let prov_str = row
                        .get::<String>("prov")
                        .unwrap_or_else(|_| "deterministic_rule".into());
                    let provenance = match prov_str.as_str() {
                        "dense_similarity_match" => ProvenanceKind::DenseSimilarityMatch,
                        "llm_constrained_inference" => ProvenanceKind::LlmConstrainedInference,
                        "human_verified" => ProvenanceKind::HumanVerified,
                        _ => ProvenanceKind::DeterministicRule,
                    };
                    let edge = EdgePayload::new(rel, conf, provenance)?;
                    dag.add_edge_by_keys(pk, ck, edge)?;
                }
            }
        }

        Ok(dag)
    }
}
