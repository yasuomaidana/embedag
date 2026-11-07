//! The persistence suite and the Bolt-only paths of the engine, against a real Neo4j container.

use std::ops::Deref;
use std::time::Duration;

use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_core::testing::StoredNode;
use embedag_neo4j::{Neo4jConfig, Neo4jEngine};
use testcontainers::core::WaitFor;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

/// An engine connected over Bolt, owning the container so it lives as long as the engine.
pub struct BoltEngine {
    engine: Neo4jEngine,
    pub uri: String,
    _container: ContainerAsync<GenericImage>,
}

impl Deref for BoltEngine {
    type Target = Neo4jEngine;

    fn deref(&self) -> &Neo4jEngine {
        &self.engine
    }
}

/// Starts a Neo4j container and connects to it, retrying while the server finishes booting.
pub async fn bolt_engine() -> Result<BoltEngine, Box<dyn std::error::Error + Send + Sync>> {
    let image = GenericImage::new("neo4j", "5")
        .with_exposed_port(7687.into())
        .with_wait_for(WaitFor::message_on_stdout("Started."))
        .with_env_var("NEO4J_AUTH", "neo4j/password");
    let container = image.start().await?;
    let port = container.get_host_port_ipv4(7687).await?;
    let uri = format!("127.0.0.1:{port}");
    let config = Neo4jConfig::bolt(&uri, "neo4j", "password");
    let mut last_error = None;
    for _ in 0..30 {
        match Neo4jEngine::open(config.clone()).await {
            Ok(engine) => {
                return Ok(BoltEngine {
                    engine,
                    uri: uri.clone(),
                    _container: container,
                });
            }
            Err(e) => last_error = Some(e),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err(last_error.map_or_else(|| "no connection attempt".into(), |e| e.to_string().into()))
}

embedag_core::persistence_suite!(
    bolt_engine(),
    #[ignore = "starts a Neo4j container: runs in the persistence-testcontainers and coverage jobs"]
);

fn stored(sku: &str) -> StoredNode {
    StoredNode {
        sku: sku.into(),
        title: format!("{sku} title"),
        category: "Category".into(),
    }
}

fn contains() -> EdgePayload {
    EdgePayload::new("CONTAINS", 0.9, ProvenanceKind::DeterministicRule).unwrap()
}

#[tokio::test]
#[ignore = "starts a Neo4j container: runs in the persistence-testcontainers and coverage jobs"]
async fn removing_edges_by_relationship_or_entirely() {
    let engine = bolt_engine().await.expect("neo4j container");
    for sku in ["a", "b"] {
        engine.insert_node(&stored(sku)).await.unwrap();
    }
    let (a, b) = ("a".to_string(), "b".to_string());
    engine.insert_edge(&a, &b, &contains()).await.unwrap();

    engine.remove_edge(&a, &b, Some("OTHER")).await.unwrap();
    assert_eq!(
        engine.get_children::<StoredNode>(&a).await.unwrap().len(),
        1
    );
    engine.remove_edge(&a, &b, Some("CONTAINS")).await.unwrap();
    assert!(
        engine
            .get_children::<StoredNode>(&a)
            .await
            .unwrap()
            .is_empty()
    );

    engine.insert_edge(&a, &b, &contains()).await.unwrap();
    engine.remove_edge(&a, &b, None).await.unwrap();
    assert!(
        engine
            .get_parents::<StoredNode>(&b)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        engine
            .get_node::<StoredNode>(&"zzz".to_string())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
#[ignore = "starts a Neo4j container: runs in the persistence-testcontainers and coverage jobs"]
async fn loading_edges_that_lack_properties_uses_the_defaults() {
    let engine = bolt_engine().await.expect("neo4j container");
    for sku in ["a", "b"] {
        engine.insert_node(&stored(sku)).await.unwrap();
    }
    // An edge written by another tool: no confidence and no provenance property.
    let config = neo4rs::ConfigBuilder::default()
        .uri(engine.uri.as_str())
        .user("neo4j")
        .password("password")
        .build()
        .unwrap();
    let graph = neo4rs::Graph::connect(config).await.unwrap();
    graph
        .run(neo4rs::query(
            "MATCH (p {business_key: 'a'}), (c {business_key: 'b'}) CREATE (p)-[:LINKS]->(c)",
        ))
        .await
        .unwrap();

    let dag = engine.load_dag::<StoredNode>().await.unwrap();
    assert_eq!(dag.edge_count(), 1);
}
