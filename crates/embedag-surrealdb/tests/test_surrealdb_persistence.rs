use embedag_core::dag::MemoryDag;
use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_core::mutation::MutationDirective;
use embedag_core::traits::{NodePayload, TransactionalGraphRepository};
use embedag_surrealdb::SurrealDbEngine;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestCategory {
    id: String,
    title: String,
    level: String,
}

impl NodePayload for TestCategory {
    type Key = String;

    fn key(&self) -> &Self::Key {
        &self.id
    }

    fn name(&self) -> &str {
        &self.title
    }

    fn entity_level(&self) -> &str {
        &self.level
    }

    fn attributes(&self) -> serde_json::Value {
        serde_json::json!({
            "title": self.title,
            "level": self.level,
        })
    }
}

#[tokio::test]
async fn test_in_memory_basic_operations() {
    let engine = SurrealDbEngine::in_memory()
        .await
        .expect("in-memory engine start");

    let cat1 = TestCategory {
        id: "cat_beverages".into(),
        title: "Beverages".into(),
        level: "L1".into(),
    };

    engine.insert_node(&cat1).await.expect("insert node");

    let retrieved = engine
        .get_node::<TestCategory>(&"cat_beverages".to_string())
        .await
        .expect("get node");

    assert_eq!(retrieved, Some(cat1));

    engine
        .remove_node_by_key("cat_beverages")
        .await
        .expect("remove node");

    let deleted = engine
        .get_node::<TestCategory>(&"cat_beverages".to_string())
        .await
        .expect("get deleted node");
    assert_eq!(deleted, None);
}

#[tokio::test]
async fn test_graph_relate_traversal() {
    let engine = SurrealDbEngine::in_memory()
        .await
        .expect("in-memory engine start");

    let root = TestCategory {
        id: "root".into(),
        title: "All Products".into(),
        level: "L0".into(),
    };
    let food = TestCategory {
        id: "food".into(),
        title: "Food & Grocery".into(),
        level: "L1".into(),
    };
    let drinks = TestCategory {
        id: "drinks".into(),
        title: "Beverages".into(),
        level: "L1".into(),
    };

    engine.insert_node(&root).await.unwrap();
    engine.insert_node(&food).await.unwrap();
    engine.insert_node(&drinks).await.unwrap();

    let edge_food = EdgePayload::new("CONTAINS", 0.95, ProvenanceKind::DeterministicRule).unwrap();
    let edge_drinks =
        EdgePayload::new("CONTAINS", 0.90, ProvenanceKind::DeterministicRule).unwrap();

    engine
        .insert_edge(&root.id, &food.id, &edge_food)
        .await
        .unwrap();
    engine
        .insert_edge(&root.id, &drinks.id, &edge_drinks)
        .await
        .unwrap();

    // Verify children traversal
    let children = engine
        .get_children::<TestCategory>(&"root".to_string())
        .await
        .unwrap();
    assert_eq!(children.len(), 2);
    let child_ids: Vec<String> = children.into_iter().map(|c| c.id).collect();
    assert!(child_ids.contains(&"food".to_string()));
    assert!(child_ids.contains(&"drinks".to_string()));

    // Verify parent traversal
    let parents = engine
        .get_parents::<TestCategory>(&"food".to_string())
        .await
        .unwrap();
    assert_eq!(parents.len(), 1);
    assert_eq!(parents[0].id, "root");

    // Remove one edge
    engine
        .remove_edge(&root.id, &food.id, Some("CONTAINS"))
        .await
        .unwrap();

    let children_after = engine
        .get_children::<TestCategory>(&"root".to_string())
        .await
        .unwrap();
    assert_eq!(children_after.len(), 1);
    assert_eq!(children_after[0].id, "drinks");
}

#[tokio::test]
async fn test_repository_capability_traits() {
    let engine = SurrealDbEngine::in_memory()
        .await
        .expect("in-memory engine start");

    let node1 = TestCategory {
        id: "c1".into(),
        title: "Category 1".into(),
        level: "L1".into(),
    };
    let node2 = TestCategory {
        id: "c2".into(),
        title: "Category 2".into(),
        level: "L2".into(),
    };

    // Test TransactionalGraphRepository
    let directives = vec![
        MutationDirective::UpsertNode(node1.clone()),
        MutationDirective::UpsertNode(node2.clone()),
        MutationDirective::AddEdge {
            parent: "c1".to_string(),
            child: "c2".to_string(),
            edge: EdgePayload::new("SUB_CATEGORY", 1.0, ProvenanceKind::DeterministicRule).unwrap(),
        },
    ];

    engine
        .apply_mutation_batch(&directives)
        .await
        .expect("apply mutation batch");

    // Test ReadOnlyGraphRepository
    let fetched = engine.get_node(&"c1".to_string()).await.unwrap();
    assert_eq!(fetched, Some(node1));

    let children: Vec<TestCategory> = engine.get_children(&"c1".to_string()).await.unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].id, "c2");

    // Test VectorSearchRepository
    engine.index_embedding("c1", vec![1.0, 0.0, 0.0]);
    engine.index_embedding("c2", vec![0.0, 1.0, 0.0]);

    let results: Vec<(TestCategory, f32)> = engine
        .find_nearest_neighbors(&[0.9, 0.1, 0.0], 1)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0.id, "c1");
    assert!(results[0].1 > 0.8);
}

#[tokio::test]
async fn test_dag_export_and_load_roundtrip() {
    let engine = SurrealDbEngine::in_memory()
        .await
        .expect("in-memory engine start");

    let mut dag = MemoryDag::<TestCategory>::new();

    let c_root = TestCategory {
        id: "all".into(),
        title: "All".into(),
        level: "L0".into(),
    };
    let c_tech = TestCategory {
        id: "tech".into(),
        title: "Tech".into(),
        level: "L1".into(),
    };
    let c_gadget = TestCategory {
        id: "gadget".into(),
        title: "Gadgets".into(),
        level: "L2".into(),
    };

    dag.upsert_node(c_root.clone());
    dag.upsert_node(c_tech.clone());
    dag.upsert_node(c_gadget.clone());

    let edge1 = EdgePayload::new("BELONGS_TO", 0.95, ProvenanceKind::DeterministicRule).unwrap();
    let edge2 = EdgePayload::new("BELONGS_TO", 0.90, ProvenanceKind::DeterministicRule).unwrap();

    dag.add_edge_by_keys(&c_root.id, &c_tech.id, edge1).unwrap();
    dag.add_edge_by_keys(&c_tech.id, &c_gadget.id, edge2)
        .unwrap();

    assert_eq!(dag.node_count(), 3);
    assert_eq!(dag.edge_count(), 2);

    // Export to SurrealDB
    engine.export_dag(&dag).await.expect("export dag");

    // Load back from SurrealDB
    let loaded = engine.load_dag::<TestCategory>().await.expect("load dag");

    assert_eq!(loaded.node_count(), 3);
    assert_eq!(loaded.edge_count(), 2);
    assert!(loaded.contains_key(&"all".to_string()));
    assert!(loaded.contains_key(&"tech".to_string()));
    assert!(loaded.contains_key(&"gadget".to_string()));
}
