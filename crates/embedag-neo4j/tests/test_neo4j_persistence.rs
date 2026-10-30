use embedag_neo4j::{Neo4jConfig, Neo4jEngine, StorageMode};

embedag_core::persistence_suite!(Neo4jEngine::in_memory());

#[test]
fn test_bolt_configuration_builder() {
    let config = Neo4jConfig::bolt("127.0.0.1:7687", "neo4j", "secret")
        .with_database("custom_db")
        .with_node_label("CustomNode")
        .with_default_relation("CUSTOM_REL");

    assert_eq!(config.mode, StorageMode::Bolt("127.0.0.1:7687".to_string()));
    assert_eq!(config.user, "neo4j");
    assert_eq!(config.password, "secret");
    assert_eq!(config.database, "custom_db");
    assert_eq!(config.node_label, "CustomNode");
    assert_eq!(config.default_relation, "CUSTOM_REL");
}
