//! High-level Neo4j OpenCypher Bolt storage engine adapter for embedag.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::Arc;

use embedag_core::dag::MemoryDag;
use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_core::mutation::dag_directives;
use embedag_core::similarity::rank_by_cosine;
use embedag_core::traits::NodePayload;
use parking_lot::RwLock;
use serde::{Serialize, de::DeserializeOwned};

use crate::config::{Neo4jConfig, StorageMode};
use crate::error::Neo4jStorageError;
use crate::mapping::{Neo4jEdgeRecord, Neo4jNodeRecord};

#[derive(Clone, Default)]
struct MockStore {
    nodes: Arc<RwLock<HashMap<String, Neo4jNodeRecord>>>,
    edges: Arc<RwLock<Vec<(String, String, Neo4jEdgeRecord)>>>,
}

impl MockStore {
    fn remove_node(&self, key: &str) {
        self.nodes.write().remove(key);
        self.edges.write().retain(|(p, c, _)| p != key && c != key);
    }

    fn insert_edge(&self, parent: String, child: String, edge: Neo4jEdgeRecord) {
        let mut edges = self.edges.write();
        edges.retain(|(p, c, r)| {
            !(p == &parent && c == &child && r.relationship == edge.relationship)
        });
        edges.push((parent, child, edge));
    }

    /// Removes the edges between the pair, only the ones with `relationship` when it is given.
    fn remove_edge(&self, parent: &str, child: &str, relationship: Option<&str>) {
        self.edges.write().retain(|(p, c, r)| {
            let is_pair = p == parent && c == child;
            !is_pair || relationship.is_some_and(|rel| r.relationship != rel)
        });
    }

    fn get_node<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, Neo4jStorageError> {
        Ok(self
            .nodes
            .read()
            .get(key)
            .map(Neo4jNodeRecord::to_node)
            .transpose()?)
    }

    fn neighbours<T: DeserializeOwned>(
        &self,
        key: &str,
        parents: bool,
    ) -> Result<Vec<T>, Neo4jStorageError> {
        let edges = self.edges.read();
        let nodes = self.nodes.read();
        let mut found = Vec::new();
        for (p, c, _) in edges.iter() {
            let (own, other) = if parents { (c, p) } else { (p, c) };
            if own == key
                && let Some(n) = nodes.get(other)
            {
                found.push(n.to_node()?);
            }
        }
        Ok(found)
    }

    fn load_dag<T: NodePayload + DeserializeOwned>(
        &self,
    ) -> Result<MemoryDag<T>, Neo4jStorageError> {
        let mut dag = MemoryDag::<T>::new();
        let mut key_map = HashMap::new();
        for (k, rec) in self.nodes.read().iter() {
            let node: T = rec.to_node()?;
            key_map.insert(k.clone(), node.key().clone());
            dag.upsert_node(node);
        }
        for (p, c, r) in self.edges.read().iter() {
            let provenance =
                ProvenanceKind::from_label(&r.provenance).unwrap_or(ProvenanceKind::HumanVerified);
            let edge = EdgePayload::new(&r.relationship, r.confidence, provenance)?;
            add_edge_if_known(&mut dag, &key_map, p, c, edge)?;
        }
        Ok(dag)
    }
}

/// Adds the edge when both endpoints were loaded.
fn add_edge_if_known<T: NodePayload>(
    dag: &mut MemoryDag<T>,
    key_map: &HashMap<String, T::Key>,
    parent: &str,
    child: &str,
    edge: EdgePayload,
) -> Result<(), Neo4jStorageError> {
    if let (Some(pk), Some(ck)) = (key_map.get(parent), key_map.get(child)) {
        dag.add_edge_by_keys(pk, ck, edge)?;
    }
    Ok(())
}

fn neighbours_cypher(label: &str, parents: bool) -> String {
    if parents {
        format!(
            "MATCH (p:`{label}`)-[r]->(c:`{label}` {{ business_key: $key }}) RETURN p.payload_json AS payload_json"
        )
    } else {
        format!(
            "MATCH (p:`{label}` {{ business_key: $key }})-[r]->(c:`{label}`) RETURN c.payload_json AS payload_json"
        )
    }
}

fn insert_node_cypher(label: &str) -> String {
    format!(
        "MERGE (n:`{label}` {{ business_key: $key }}) \
        SET n.name = $name, \
            n.entity_level = $level, \
            n.attributes_json = $attr, \
            n.payload_json = $payload"
    )
}

fn get_node_cypher(label: &str) -> String {
    format!("MATCH (n:`{label}` {{ business_key: $key }}) RETURN n.payload_json AS payload_json")
}

fn remove_node_cypher(label: &str) -> String {
    format!("MATCH (n:`{label}` {{ business_key: $key }}) DETACH DELETE n")
}

/// The relationship type to store: the edge's own, else the configured default.
fn relationship_type<'a>(relationship: &'a str, default_relation: &'a str) -> &'a str {
    if relationship.is_empty() {
        default_relation
    } else {
        relationship
    }
}

fn insert_edge_cypher(label: &str, rel_type: &str) -> String {
    format!(
        "MATCH (p:`{label}` {{ business_key: $parent }}), (c:`{label}` {{ business_key: $child }}) \
        MERGE (p)-[r:`{rel_type}`]->(c) \
        SET r.confidence = $conf, \
            r.provenance = $prov, \
            r.created_at = $created"
    )
}

fn remove_edge_cypher(label: &str, relationship: Option<&str>) -> String {
    let rel = relationship.map_or_else(String::new, |rel| format!(":`{rel}`"));
    format!(
        "MATCH (p:`{label}` {{ business_key: $parent }})-[r{rel}]->(c:`{label}` {{ business_key: $child }}) DELETE r"
    )
}

/// Runs `query` and deserializes the `payload_json` column of every row.
async fn query_payloads<T: DeserializeOwned>(
    graph: &neo4rs::Graph,
    query: neo4rs::Query,
) -> Result<Vec<T>, Neo4jStorageError> {
    let mut stream = graph.execute(query).await?;
    let mut found = Vec::new();
    while let Ok(Some(row)) = stream.next().await {
        // A row without the column is skipped.
        let payload = row.get::<String>("payload_json").ok();
        found.extend(
            payload
                .map(|json| serde_json::from_str(&json))
                .transpose()?,
        );
    }
    Ok(found)
}

/// Loads every node into `dag` and returns the business-key to node-key map.
async fn load_graph_nodes<T: NodePayload + DeserializeOwned>(
    graph: &neo4rs::Graph,
    label: &str,
    dag: &mut MemoryDag<T>,
) -> Result<HashMap<String, T::Key>, Neo4jStorageError> {
    let cypher = format!("MATCH (n:`{label}`) RETURN n.payload_json AS payload_json");
    let mut key_map = HashMap::new();
    for node in query_payloads::<T>(graph, neo4rs::query(&cypher)).await? {
        key_map.insert(node.key().to_string(), node.key().clone());
        dag.upsert_node(node);
    }
    Ok(key_map)
}

async fn load_graph_edges<T: NodePayload>(
    graph: &neo4rs::Graph,
    label: &str,
    key_map: &HashMap<String, T::Key>,
    dag: &mut MemoryDag<T>,
) -> Result<(), Neo4jStorageError> {
    let cypher = format!(
        "MATCH (p:`{label}`)-[r]->(c:`{label}`) RETURN p.business_key AS parent, c.business_key AS child, type(r) AS rel, r.confidence AS conf, r.provenance AS prov"
    );
    let mut stream = graph.execute(neo4rs::query(&cypher)).await?;
    while let Ok(Some(row)) = stream.next().await {
        // A row without both endpoints yields nothing to add.
        edge_endpoints(&row).map_or(Ok(()), |(p, c)| {
            add_edge_if_known(dag, key_map, &p, &c, graph_edge(&row)?)
        })?;
    }
    Ok(())
}

fn edge_endpoints(row: &neo4rs::Row) -> Option<(String, String)> {
    Some((row.get("parent").ok()?, row.get("child").ok()?))
}

/// The relationship, confidence and provenance columns of an edge row, with defaults.
fn graph_edge(row: &neo4rs::Row) -> Result<EdgePayload, Neo4jStorageError> {
    let rel = row.get::<String>("rel").unwrap_or_else(|_| "REL".into());
    let conf = row.get::<f64>("conf").unwrap_or(1.0) as f32;
    let provenance = row
        .get::<String>("prov")
        .ok()
        .and_then(|label| ProvenanceKind::from_label(&label))
        .unwrap_or(ProvenanceKind::DeterministicRule);
    Ok(EdgePayload::new(rel, conf, provenance)?)
}

/// Where the engine keeps the graph: in memory, or in a Neo4j server reached over Bolt.
#[derive(Clone)]
enum Backend {
    Mock(MockStore),
    Bolt(Arc<neo4rs::Graph>),
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
    backend: Backend,
    vector_index: Arc<RwLock<HashMap<String, Vec<f32>>>>,
}

impl Neo4jEngine {
    /// Opens or connects to a Neo4j instance according to the given configuration.
    pub async fn open(config: Neo4jConfig) -> Result<Self, Neo4jStorageError> {
        match &config.mode {
            StorageMode::InMemoryMock => Ok(Self {
                config,
                backend: Backend::Mock(MockStore::default()),
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
                    backend: Backend::Bolt(Arc::new(graph)),
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

        match &self.backend {
            Backend::Mock(mock) => {
                mock.nodes.write().insert(key_str, record);
            }
            Backend::Bolt(graph) => {
                let q = neo4rs::query(&insert_node_cypher(&self.config.node_label))
                    .param("key", record.business_key)
                    .param("name", record.name)
                    .param("level", record.entity_level)
                    .param("attr", record.attributes_json)
                    .param("payload", record.payload_json);
                graph.run(q).await?;
            }
        }
        Ok(())
    }

    /// Fetches a node by its business key.
    pub async fn get_node<T: NodePayload + DeserializeOwned>(
        &self,
        key: &T::Key,
    ) -> Result<Option<T>, Neo4jStorageError> {
        self.get_node_by_string_key(&key.to_string()).await
    }

    /// Removes a node and any attached relationships by its business key.
    pub async fn remove_node_by_key(&self, key: &str) -> Result<(), Neo4jStorageError> {
        match &self.backend {
            Backend::Mock(mock) => mock.remove_node(key),
            Backend::Bolt(graph) => {
                let q =
                    neo4rs::query(&remove_node_cypher(&self.config.node_label)).param("key", key);
                graph.run(q).await?;
            }
        }
        self.vector_index.write().remove(key);
        Ok(())
    }

    /// Inserts or replaces a directed graph edge (`parent -> child`).
    pub async fn insert_edge<K: Display>(
        &self,
        parent: &K,
        child: &K,
        edge: &EdgePayload,
    ) -> Result<(), Neo4jStorageError> {
        let (parent_key, child_key) = (parent.to_string(), child.to_string());
        let edge_rec = Neo4jEdgeRecord::from_edge(edge);

        match &self.backend {
            Backend::Mock(mock) => mock.insert_edge(parent_key, child_key, edge_rec),
            Backend::Bolt(graph) => {
                let rel_type =
                    relationship_type(&edge_rec.relationship, &self.config.default_relation);
                let cypher = insert_edge_cypher(&self.config.node_label, rel_type);
                let q = neo4rs::query(&cypher)
                    .param("parent", parent_key)
                    .param("child", child_key)
                    .param("conf", edge_rec.confidence as f64)
                    .param("prov", edge_rec.provenance)
                    .param("created", edge_rec.created_at);
                graph.run(q).await?;
            }
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
        let (parent_key, child_key) = (parent.to_string(), child.to_string());

        match &self.backend {
            Backend::Mock(mock) => mock.remove_edge(&parent_key, &child_key, relationship),
            Backend::Bolt(graph) => {
                let cypher = remove_edge_cypher(&self.config.node_label, relationship);
                let q = neo4rs::query(&cypher)
                    .param("parent", parent_key)
                    .param("child", child_key);
                graph.run(q).await?;
            }
        }
        Ok(())
    }

    /// Fetches immediate parents of a node identified by business key.
    pub async fn get_parents<T: NodePayload + DeserializeOwned>(
        &self,
        child_key: &T::Key,
    ) -> Result<Vec<T>, Neo4jStorageError> {
        self.neighbours(child_key.to_string(), true).await
    }

    /// Fetches immediate children of a node identified by business key.
    pub async fn get_children<T: NodePayload + DeserializeOwned>(
        &self,
        parent_key: &T::Key,
    ) -> Result<Vec<T>, Neo4jStorageError> {
        self.neighbours(parent_key.to_string(), false).await
    }

    /// The nodes one edge away from `key`: its parents when `parents`, else its children.
    async fn neighbours<T: NodePayload + DeserializeOwned>(
        &self,
        key: String,
        parents: bool,
    ) -> Result<Vec<T>, Neo4jStorageError> {
        match &self.backend {
            Backend::Mock(mock) => mock.neighbours(&key, parents),
            Backend::Bolt(graph) => {
                let cypher = neighbours_cypher(&self.config.node_label, parents);
                query_payloads(graph, neo4rs::query(&cypher).param("key", key)).await
            }
        }
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
        let scored = rank_by_cosine(&self.vector_index.read(), query, limit);

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
        match &self.backend {
            Backend::Mock(mock) => mock.get_node(key),
            Backend::Bolt(graph) => {
                let q = neo4rs::query(&get_node_cypher(&self.config.node_label)).param("key", key);
                Ok(query_payloads(graph, q).await?.into_iter().next())
            }
        }
    }

    /// Exports an in-memory [`MemoryDag<T>`] into the Neo4j graph database.
    pub async fn export_dag<T: NodePayload + Serialize>(
        &self,
        dag: &MemoryDag<T>,
    ) -> Result<(), Neo4jStorageError> {
        self.apply_directives(&dag_directives(dag)).await
    }

    /// Loads graph contents from Neo4j into a fresh [`MemoryDag<T>`].
    pub async fn load_dag<T: NodePayload + DeserializeOwned>(
        &self,
    ) -> Result<MemoryDag<T>, Neo4jStorageError> {
        match &self.backend {
            Backend::Mock(mock) => mock.load_dag(),
            Backend::Bolt(graph) => {
                let label = &self.config.node_label;
                let mut dag = MemoryDag::<T>::new();
                let key_map = load_graph_nodes(graph, label, &mut dag).await?;
                load_graph_edges(graph, label, &key_map, &mut dag).await?;
                Ok(dag)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedag_core::testing::StoredNode;
    use rstest::rstest;

    fn node(sku: &str) -> StoredNode {
        StoredNode {
            sku: sku.into(),
            title: format!("{sku} title"),
            category: "Category".into(),
        }
    }

    fn edge(relationship: &str, provenance: &str) -> Neo4jEdgeRecord {
        Neo4jEdgeRecord {
            relationship: relationship.into(),
            confidence: 0.5,
            provenance: provenance.into(),
            created_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[rstest]
    #[case::own("CONTAINS", "REL", "CONTAINS")]
    #[case::default("", "REL", "REL")]
    fn relationship_type_falls_back_to_the_default(
        #[case] own: &str,
        #[case] default: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(relationship_type(own, default), expected);
    }

    #[rstest]
    #[case::any_relationship(None, "-[r]->")]
    #[case::one_relationship(Some("CONTAINS"), "-[r:`CONTAINS`]->")]
    fn remove_edge_cypher_filters_by_relationship(
        #[case] relationship: Option<&str>,
        #[case] pattern: &str,
    ) {
        assert!(remove_edge_cypher("Node", relationship).contains(pattern));
    }

    #[rstest]
    #[case::parents(true, "RETURN p.payload_json")]
    #[case::children(false, "RETURN c.payload_json")]
    fn neighbours_cypher_returns_the_requested_side(#[case] parents: bool, #[case] returns: &str) {
        assert!(neighbours_cypher("Node", parents).contains(returns));
    }

    #[test]
    fn the_other_cypher_statements_name_the_label() {
        for cypher in [
            insert_node_cypher("Lbl"),
            get_node_cypher("Lbl"),
            remove_node_cypher("Lbl"),
            insert_edge_cypher("Lbl", "REL"),
        ] {
            assert!(cypher.contains("`Lbl`"), "{cypher}");
        }
    }

    #[tokio::test]
    async fn exposes_its_configuration() {
        let engine = Neo4jEngine::in_memory().await.unwrap();
        assert_eq!(
            engine.config().node_label,
            Neo4jConfig::in_memory().node_label
        );
    }

    #[test]
    fn mock_edge_removal_respects_the_relationship_filter() {
        let mock = MockStore::default();
        for rel in ["A", "B"] {
            mock.insert_edge("p".into(), "c".into(), edge(rel, "human_verified"));
        }
        mock.insert_edge("p".into(), "other".into(), edge("A", "human_verified"));

        mock.remove_edge("p", "c", Some("A"));
        assert_eq!(mock.edges.read().len(), 2);
        mock.remove_edge("p", "c", None);
        assert_eq!(mock.edges.read().len(), 1);
    }

    #[test]
    fn mock_load_skips_edges_with_an_unknown_endpoint_and_defaults_the_provenance() {
        let mock = MockStore::default();
        for sku in ["a", "b"] {
            let record = Neo4jNodeRecord::from_node(&node(sku)).unwrap();
            mock.nodes.write().insert(sku.into(), record);
        }
        mock.insert_edge("a".into(), "b".into(), edge("CONTAINS", "unheard_of"));
        mock.insert_edge(
            "a".into(),
            "missing".into(),
            edge("CONTAINS", "human_verified"),
        );

        let dag: MemoryDag<StoredNode> = mock.load_dag().unwrap();
        assert_eq!(dag.edge_count(), 1);
    }

    #[test]
    fn mock_remove_node_drops_its_edges() {
        let mock = MockStore::default();
        mock.nodes
            .write()
            .insert("a".into(), Neo4jNodeRecord::from_node(&node("a")).unwrap());
        mock.insert_edge("a".into(), "b".into(), edge("CONTAINS", "human_verified"));
        mock.remove_node("a");
        assert!(mock.edges.read().is_empty());
        assert!(mock.get_node::<StoredNode>("a").unwrap().is_none());
    }
}
