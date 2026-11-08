//! The engine's configuration, accessors and the edge cases of its graph queries.

use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_core::testing::StoredNode;
use embedag_surrealdb::{StorageMode, SurrealDbConfig, SurrealDbEngine};
use rstest::rstest;

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

#[test]
fn the_configuration_builders_set_every_option() {
    let config = SurrealDbConfig::remote("ws://example.org:8000")
        .with_namespace("ns")
        .with_database("db")
        .with_node_table("nodes")
        .with_edge_table("edges")
        .with_credentials("root", "secret");
    assert_eq!(
        config.mode,
        StorageMode::Remote("ws://example.org:8000".into())
    );
    assert_eq!(
        (config.namespace.as_str(), config.database.as_str()),
        ("ns", "db")
    );
    assert_eq!(
        (config.node_table.as_str(), config.edge_table.as_str()),
        ("nodes", "edges")
    );
    assert_eq!(config.username.as_deref(), Some("root"));
    assert_eq!(config.password.as_deref(), Some("secret"));

    let local = SurrealDbConfig::in_memory();
    assert_eq!(local.mode, StorageMode::InMemory);
    assert!(local.username.is_none());
}

#[rstest]
#[case::websocket("ws://127.0.0.1:1")]
#[case::http("http://127.0.0.1:1")]
#[tokio::test]
async fn an_unreachable_endpoint_is_a_connection_error(#[case] endpoint: &str) {
    let config = SurrealDbConfig::remote(endpoint);
    assert!(SurrealDbEngine::open(config).await.is_err());
}

#[tokio::test]
async fn the_engine_exposes_its_configuration_and_client() {
    let engine = SurrealDbEngine::in_memory().await.unwrap();
    assert_eq!(engine.config().namespace, "embedag");
    // The client is usable directly.
    engine.inner().query("RETURN 1").await.unwrap();
}

#[tokio::test]
async fn edges_are_removed_by_relationship_or_entirely() {
    let engine = SurrealDbEngine::in_memory().await.unwrap();
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
}

#[tokio::test]
async fn edges_and_embeddings_that_point_nowhere_are_ignored() {
    let engine = SurrealDbEngine::in_memory().await.unwrap();
    engine.insert_node(&stored("a")).await.unwrap();
    let (a, ghost) = ("a".to_string(), "ghost".to_string());
    engine.insert_edge(&a, &ghost, &contains()).await.unwrap();
    engine.index_embedding("ghost", vec![1.0, 0.0]);

    assert!(
        engine
            .get_children::<StoredNode>(&a)
            .await
            .unwrap()
            .is_empty()
    );
    let near = engine
        .find_nearest_neighbors::<StoredNode>(&[1.0, 0.0], 3)
        .await
        .unwrap();
    assert!(near.is_empty());
    // Loading skips the edge whose endpoint is not a stored node.
    let dag = engine.load_dag::<StoredNode>().await.unwrap();
    assert_eq!((dag.node_count(), dag.edge_count()), (1, 0));
}
