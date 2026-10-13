use embedag_core::dag::MemoryDag;
use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_core::mutation::MutationDirective;
use embedag_core::traits::{NodePayload, TransactionalGraphRepository};
use embedag_neo4j::{Neo4jConfig, Neo4jEngine, StorageMode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestProduct {
    sku: String,
    title: String,
    category: String,
}

impl NodePayload for TestProduct {
    type Key = String;

    fn key(&self) -> &Self::Key {
        &self.sku
    }

    fn name(&self) -> &str {
        &self.title
    }

    fn entity_level(&self) -> &str {
        &self.category
    }

    fn attributes(&self) -> serde_json::Value {
        serde_json::json!({
            "title": self.title,
            "category": self.category,
        })
    }
}

#[tokio::test]
async fn test_in_memory_basic_operations() {
    let engine = Neo4jEngine::in_memory()
        .await
        .expect("in-memory engine start");

    let p1 = TestProduct {
        sku: "SKU-100".into(),
        title: "Espresso Beans".into(),
        category: "Coffee".into(),
    };

    engine.insert_node(&p1).await.expect("insert node");

    let retrieved = engine
        .get_node::<TestProduct>(&"SKU-100".to_string())
        .await
        .expect("get node");

    assert_eq!(retrieved, Some(p1));

    engine
        .remove_node_by_key("SKU-100")
        .await
        .expect("remove node");

    let deleted = engine
        .get_node::<TestProduct>(&"SKU-100".to_string())
        .await
        .expect("get deleted node");
    assert_eq!(deleted, None);
}

#[tokio::test]
async fn test_graph_relate_traversal() {
    let engine = Neo4jEngine::in_memory()
        .await
        .expect("in-memory engine start");

    let root = TestProduct {
        sku: "DEPT-FOOD".into(),
        title: "Food & Beverage".into(),
        category: "Department".into(),
    };
    let cat_tea = TestProduct {
        sku: "CAT-TEA".into(),
        title: "Teas".into(),
        category: "Category".into(),
    };
    let cat_coffee = TestProduct {
        sku: "CAT-COFFEE".into(),
        title: "Coffees".into(),
        category: "Category".into(),
    };

    engine.insert_node(&root).await.unwrap();
    engine.insert_node(&cat_tea).await.unwrap();
    engine.insert_node(&cat_coffee).await.unwrap();

    let edge_tea = EdgePayload::new("CONTAINS", 0.95, ProvenanceKind::DeterministicRule).unwrap();
    let edge_coffee =
        EdgePayload::new("CONTAINS", 0.90, ProvenanceKind::DeterministicRule).unwrap();

    engine
        .insert_edge(&root.sku, &cat_tea.sku, &edge_tea)
        .await
        .unwrap();
    engine
        .insert_edge(&root.sku, &cat_coffee.sku, &edge_coffee)
        .await
        .unwrap();

    // Verify children traversal
    let children = engine
        .get_children::<TestProduct>(&"DEPT-FOOD".to_string())
        .await
        .unwrap();
    assert_eq!(children.len(), 2);
    let child_skus: Vec<String> = children.into_iter().map(|c| c.sku).collect();
    assert!(child_skus.contains(&"CAT-TEA".to_string()));
    assert!(child_skus.contains(&"CAT-COFFEE".to_string()));

    // Verify parent traversal
    let parents = engine
        .get_parents::<TestProduct>(&"CAT-TEA".to_string())
        .await
        .unwrap();
    assert_eq!(parents.len(), 1);
    assert_eq!(parents[0].sku, "DEPT-FOOD");

    // Remove one edge
    engine
        .remove_edge(&root.sku, &cat_tea.sku, Some("CONTAINS"))
        .await
        .unwrap();

    let children_after = engine
        .get_children::<TestProduct>(&"DEPT-FOOD".to_string())
        .await
        .unwrap();
    assert_eq!(children_after.len(), 1);
    assert_eq!(children_after[0].sku, "CAT-COFFEE");
}

#[tokio::test]
async fn test_repository_capability_traits() {
    let engine = Neo4jEngine::in_memory()
        .await
        .expect("in-memory engine start");

    let p1 = TestProduct {
        sku: "P1".into(),
        title: "Product 1".into(),
        category: "General".into(),
    };
    let p2 = TestProduct {
        sku: "P2".into(),
        title: "Product 2".into(),
        category: "General".into(),
    };

    // Test TransactionalGraphRepository
    let directives = vec![
        MutationDirective::UpsertNode(p1.clone()),
        MutationDirective::UpsertNode(p2.clone()),
        MutationDirective::AddEdge {
            parent: "P1".to_string(),
            child: "P2".to_string(),
            edge: EdgePayload::new("RELATED_TO", 1.0, ProvenanceKind::DeterministicRule).unwrap(),
        },
    ];

    engine
        .apply_mutation_batch(&directives)
        .await
        .expect("apply mutation batch");

    // Test ReadOnlyGraphRepository
    let fetched = engine.get_node(&"P1".to_string()).await.unwrap();
    assert_eq!(fetched, Some(p1));

    let children: Vec<TestProduct> = engine.get_children(&"P1".to_string()).await.unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].sku, "P2");

    // Test VectorSearchRepository
    engine.index_embedding("P1", vec![1.0, 0.0, 0.0]);
    engine.index_embedding("P2", vec![0.0, 1.0, 0.0]);

    let results: Vec<(TestProduct, f32)> = engine
        .find_nearest_neighbors(&[0.9, 0.1, 0.0], 1)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0.sku, "P1");
    assert!(results[0].1 > 0.8);
}

#[tokio::test]
async fn test_dag_export_and_load_roundtrip() {
    let engine = Neo4jEngine::in_memory()
        .await
        .expect("in-memory engine start");

    let mut dag = MemoryDag::<TestProduct>::new();

    let root = TestProduct {
        sku: "ROOT".into(),
        title: "Catalog Root".into(),
        category: "L0".into(),
    };
    let child1 = TestProduct {
        sku: "C1".into(),
        title: "Child 1".into(),
        category: "L1".into(),
    };
    let child2 = TestProduct {
        sku: "C2".into(),
        title: "Child 2".into(),
        category: "L2".into(),
    };

    dag.upsert_node(root.clone());
    dag.upsert_node(child1.clone());
    dag.upsert_node(child2.clone());

    let edge1 = EdgePayload::new("PARENT_OF", 0.95, ProvenanceKind::DeterministicRule).unwrap();
    let edge2 = EdgePayload::new("PARENT_OF", 0.90, ProvenanceKind::DeterministicRule).unwrap();

    dag.add_edge_by_keys(&root.sku, &child1.sku, edge1).unwrap();
    dag.add_edge_by_keys(&child1.sku, &child2.sku, edge2)
        .unwrap();

    assert_eq!(dag.node_count(), 3);
    assert_eq!(dag.edge_count(), 2);

    // Export to Neo4j
    engine.export_dag(&dag).await.expect("export dag");

    // Load back from Neo4j
    let loaded = engine.load_dag::<TestProduct>().await.expect("load dag");

    assert_eq!(loaded.node_count(), 3);
    assert_eq!(loaded.edge_count(), 2);
    assert!(loaded.contains_key(&"ROOT".to_string()));
    assert!(loaded.contains_key(&"C1".to_string()));
    assert!(loaded.contains_key(&"C2".to_string()));
}

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
