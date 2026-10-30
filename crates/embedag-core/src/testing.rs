//! Fixtures shared by the tests of this workspace (feature `testing`).

use serde::{Deserialize, Serialize};

/// A node with a string key, a display name and an entity level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestNode {
    /// Business key.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Entity level (the node's type).
    pub level: String,
}

impl TestNode {
    /// A node with the given key, name and level.
    pub fn new(id: &str, name: &str, level: &str) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            level: level.into(),
        }
    }
}

crate::impl_node_payload!(
    TestNode,
    key = |n| &n.id,
    name = |n| &n.name,
    level = |n| &n.level
);

/// A node the storage-adapter tests persist as JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredNode {
    /// Business key.
    pub sku: String,
    /// Display name.
    pub title: String,
    /// Entity level.
    pub category: String,
}

crate::impl_node_payload!(
    StoredNode,
    key = |n| &n.sku,
    name = |n| &n.title,
    level = |n| &n.category,
    attributes = |n| { serde_json::json!({ "title": n.title, "category": n.category }) }
);

/// Defines the persistence tests every storage adapter must pass. `$engine` is a future that
/// opens an empty in-memory engine; the engine must offer `insert_node`, `get_node`, `insert_edge`,
/// `export_dag`, `load_dag` and the repository traits. The caller needs `tokio` with its test macro.
#[macro_export]
macro_rules! persistence_suite {
    ($engine:expr) => {
        #[tokio::test]
        async fn test_in_memory_basic_operations() {
            use $crate::dag::MemoryDag;
            use $crate::model::{EdgePayload, ProvenanceKind};
            use $crate::mutation::MutationDirective;
            use $crate::testing::StoredNode;
            use $crate::traits::TransactionalGraphRepository;
            let engine = $engine.await.expect("in-memory engine start");

            let p1 = StoredNode {
                sku: "SKU-100".into(),
                title: "Espresso Beans".into(),
                category: "Coffee".into(),
            };

            engine.insert_node(&p1).await.expect("insert node");

            let retrieved = engine
                .get_node::<StoredNode>(&"SKU-100".to_string())
                .await
                .expect("get node");

            assert_eq!(retrieved, Some(p1));

            engine
                .remove_node_by_key("SKU-100")
                .await
                .expect("remove node");

            let deleted = engine
                .get_node::<StoredNode>(&"SKU-100".to_string())
                .await
                .expect("get deleted node");
            assert_eq!(deleted, None);
        }

        #[tokio::test]
        async fn test_graph_relate_traversal() {
            use $crate::dag::MemoryDag;
            use $crate::model::{EdgePayload, ProvenanceKind};
            use $crate::mutation::MutationDirective;
            use $crate::testing::StoredNode;
            use $crate::traits::TransactionalGraphRepository;
            let engine = $engine.await.expect("in-memory engine start");

            let root = StoredNode {
                sku: "DEPT-FOOD".into(),
                title: "Food & Beverage".into(),
                category: "Department".into(),
            };
            let cat_tea = StoredNode {
                sku: "CAT-TEA".into(),
                title: "Teas".into(),
                category: "Category".into(),
            };
            let cat_coffee = StoredNode {
                sku: "CAT-COFFEE".into(),
                title: "Coffees".into(),
                category: "Category".into(),
            };

            engine.insert_node(&root).await.unwrap();
            engine.insert_node(&cat_tea).await.unwrap();
            engine.insert_node(&cat_coffee).await.unwrap();

            let edge_tea =
                EdgePayload::new("CONTAINS", 0.95, ProvenanceKind::DeterministicRule).unwrap();
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
                .get_children::<StoredNode>(&"DEPT-FOOD".to_string())
                .await
                .unwrap();
            assert_eq!(children.len(), 2);
            let child_skus: Vec<String> = children.into_iter().map(|c| c.sku).collect();
            assert!(child_skus.contains(&"CAT-TEA".to_string()));
            assert!(child_skus.contains(&"CAT-COFFEE".to_string()));

            // Verify parent traversal
            let parents = engine
                .get_parents::<StoredNode>(&"CAT-TEA".to_string())
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
                .get_children::<StoredNode>(&"DEPT-FOOD".to_string())
                .await
                .unwrap();
            assert_eq!(children_after.len(), 1);
            assert_eq!(children_after[0].sku, "CAT-COFFEE");
        }

        #[tokio::test]
        async fn test_repository_capability_traits() {
            use $crate::dag::MemoryDag;
            use $crate::model::{EdgePayload, ProvenanceKind};
            use $crate::mutation::MutationDirective;
            use $crate::testing::StoredNode;
            use $crate::traits::TransactionalGraphRepository;
            let engine = $engine.await.expect("in-memory engine start");

            let p1 = StoredNode {
                sku: "P1".into(),
                title: "Product 1".into(),
                category: "General".into(),
            };
            let p2 = StoredNode {
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
                    edge: EdgePayload::new("RELATED_TO", 1.0, ProvenanceKind::DeterministicRule)
                        .unwrap(),
                },
            ];

            engine
                .apply_mutation_batch(&directives)
                .await
                .expect("apply mutation batch");

            // Test ReadOnlyGraphRepository
            let fetched = engine.get_node(&"P1".to_string()).await.unwrap();
            assert_eq!(fetched, Some(p1));

            let children: Vec<StoredNode> = engine.get_children(&"P1".to_string()).await.unwrap();
            assert_eq!(children.len(), 1);
            assert_eq!(children[0].sku, "P2");

            // Test VectorSearchRepository
            engine.index_embedding("P1", vec![1.0, 0.0, 0.0]);
            engine.index_embedding("P2", vec![0.0, 1.0, 0.0]);

            let results: Vec<(StoredNode, f32)> = engine
                .find_nearest_neighbors(&[0.9, 0.1, 0.0], 1)
                .await
                .unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].0.sku, "P1");
            assert!(results[0].1 > 0.8);
        }

        #[tokio::test]
        async fn test_dag_export_and_load_roundtrip() {
            use $crate::dag::MemoryDag;
            use $crate::model::{EdgePayload, ProvenanceKind};
            use $crate::mutation::MutationDirective;
            use $crate::testing::StoredNode;
            use $crate::traits::TransactionalGraphRepository;
            let engine = $engine.await.expect("in-memory engine start");

            let mut dag = MemoryDag::<StoredNode>::new();

            let root = StoredNode {
                sku: "ROOT".into(),
                title: "Catalog Root".into(),
                category: "L0".into(),
            };
            let child1 = StoredNode {
                sku: "C1".into(),
                title: "Child 1".into(),
                category: "L1".into(),
            };
            let child2 = StoredNode {
                sku: "C2".into(),
                title: "Child 2".into(),
                category: "L2".into(),
            };

            dag.upsert_node(root.clone());
            dag.upsert_node(child1.clone());
            dag.upsert_node(child2.clone());

            let edge1 =
                EdgePayload::new("PARENT_OF", 0.95, ProvenanceKind::DeterministicRule).unwrap();
            let edge2 =
                EdgePayload::new("PARENT_OF", 0.90, ProvenanceKind::DeterministicRule).unwrap();

            dag.add_edge_by_keys(&root.sku, &child1.sku, edge1).unwrap();
            dag.add_edge_by_keys(&child1.sku, &child2.sku, edge2)
                .unwrap();

            assert_eq!(dag.node_count(), 3);
            assert_eq!(dag.edge_count(), 2);

            // Export to the store
            engine.export_dag(&dag).await.expect("export dag");

            // Load back from the store
            let loaded = engine.load_dag::<StoredNode>().await.expect("load dag");

            assert_eq!(loaded.node_count(), 3);
            assert_eq!(loaded.edge_count(), 2);
            assert!(loaded.contains_key(&"ROOT".to_string()));
            assert!(loaded.contains_key(&"C1".to_string()));
            assert!(loaded.contains_key(&"C2".to_string()));
        }
    };
}
