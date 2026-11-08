use embedag_core::dag::MemoryDag;
use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_core::mutation::MutationDirective;
use embedag_core::testing::TestNode;
use embedag_core::traits::{
    ReadOnlyGraphRepository, TransactionalGraphRepository, VectorSearchRepository,
};
use embedag_oxirs::{OxiRsTdbConfig, OxiRsTdbEngine, SyncPolicy};
use tempfile::tempdir;

#[tokio::test]
async fn test_in_memory_basic_operations() {
    let engine = OxiRsTdbEngine::in_memory().expect("in-memory engine must initialize");

    let cat1 = TestNode::new("CAT-1", "Dairy", "Department");
    let cat2 = TestNode::new("CAT-2", "Cheese", "Category");

    engine.insert_node(&cat1).expect("insert_node cat1");
    engine.insert_node(&cat2).expect("insert_node cat2");

    let edge = EdgePayload::new("PARENT_OF", 0.95, ProvenanceKind::DeterministicRule)
        .expect("valid edge payload");
    engine
        .insert_edge(&cat1.id, &cat2.id, &edge)
        .expect("insert_edge");

    assert!(engine.len().expect("len") > 0);

    // Retrieve node
    let retrieved: Option<TestNode> = engine.get_node(&cat1.id).expect("get_node");
    assert_eq!(retrieved, Some(cat1.clone()));

    // Retrieve children
    let children: Vec<TestNode> = engine.get_children(&cat1.id).expect("get_children");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].id, "CAT-2");

    // Retrieve parents
    let parents: Vec<TestNode> = engine.get_parents(&cat2.id).expect("get_parents");
    assert_eq!(parents.len(), 1);
    assert_eq!(parents[0].id, "CAT-1");
}

#[tokio::test]
async fn test_persistent_disk_roundtrip_across_reopen() {
    let tmp = tempdir().expect("tempdir");
    let db_path = tmp.path().join("oxirs_db");

    let config = OxiRsTdbConfig::persistent(&db_path)
        .with_sync_policy(SyncPolicy::EveryN(1))
        .with_base_iri("urn:custom:knowledge:");

    let cat_root = TestNode::new("ROOT-1", "Groceries", "Department");
    let cat_sub = TestNode::new("SUB-10", "Beverages", "Aisle");
    let cat_item = TestNode::new("SKU-99", "Cold Brew 250ml", "Product");

    // Phase 1: Write and flush
    {
        let engine = OxiRsTdbEngine::open(config.clone()).expect("open persistent engine");
        engine.insert_node(&cat_root).expect("insert root");
        engine.insert_node(&cat_sub).expect("insert sub");
        engine.insert_node(&cat_item).expect("insert item");

        let edge1 = EdgePayload::new("CONTAINS", 1.0, ProvenanceKind::DeterministicRule).unwrap();
        let edge2 = EdgePayload::new("CONTAINS", 0.9, ProvenanceKind::DeterministicRule).unwrap();

        engine
            .insert_edge(&cat_root.id, &cat_sub.id, &edge1)
            .unwrap();
        engine
            .insert_edge(&cat_sub.id, &cat_item.id, &edge2)
            .unwrap();

        engine.flush().expect("flush to disk");
        assert!(engine.len().unwrap() >= 15);
    } // Engine dropped here

    // Verify data.nq exists on disk
    let nq_file = db_path.join("data.nq");
    assert!(nq_file.exists(), "data.nq must exist on disk");
    assert!(std::fs::metadata(&nq_file).unwrap().len() > 0);

    // Phase 2: Reopen persistent store from disk
    {
        let reopened = OxiRsTdbEngine::open(config).expect("reopen persistent engine");

        let item: Option<TestNode> = reopened.get_node(&"SKU-99".to_string()).unwrap();
        assert_eq!(item, Some(cat_item));

        let sub_children: Vec<TestNode> = reopened.get_children(&"SUB-10".to_string()).unwrap();
        assert_eq!(sub_children.len(), 1);
        assert_eq!(sub_children[0].id, "SKU-99");

        let sub_parents: Vec<TestNode> = reopened.get_parents(&"SUB-10".to_string()).unwrap();
        assert_eq!(sub_parents.len(), 1);
        assert_eq!(sub_parents[0].id, "ROOT-1");
    }
}

#[tokio::test]
async fn test_sparql_query_execution() {
    let engine = OxiRsTdbEngine::in_memory().unwrap();

    let node1 = TestNode::new("DEPT-1", "Produce", "Department");
    let node2 = TestNode::new("CAT-5", "Organic Apples", "Category");

    engine.insert_node(&node1).unwrap();
    engine.insert_node(&node2).unwrap();

    let edge = EdgePayload::new("INCLUDES", 0.98, ProvenanceKind::DeterministicRule).unwrap();
    engine.insert_edge(&node1.id, &node2.id, &edge).unwrap();

    // Query all entities with rdfs:label
    let sparql = r#"
        PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
        SELECT ?label WHERE {
            ?s rdfs:label ?label .
        }
    "#;

    let results = engine.query_sparql(sparql).expect("sparql query executes");
    assert!(results.len() >= 2);
}

#[tokio::test]
async fn test_dag_export_and_load_roundtrip() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("dag_store");

    let engine = OxiRsTdbEngine::open_persistent(&db_path).unwrap();

    // Build memory DAG
    let mut original_dag = MemoryDag::new();
    let n1 = TestNode::new("N1", "Electronics", "Division");
    let n2 = TestNode::new("N2", "Audio", "Category");
    let n3 = TestNode::new("N3", "Headphones", "Subcategory");

    original_dag.upsert_node(n1);
    original_dag.upsert_node(n2);
    original_dag.upsert_node(n3);

    let e1 = EdgePayload::new("CATEGORY_OF", 1.0, ProvenanceKind::DeterministicRule).unwrap();
    let e2 = EdgePayload::new("SUBCATEGORY_OF", 1.0, ProvenanceKind::DeterministicRule).unwrap();

    original_dag
        .add_edge_by_keys(&"N1".to_string(), &"N2".to_string(), e1)
        .unwrap();
    original_dag
        .add_edge_by_keys(&"N2".to_string(), &"N3".to_string(), e2)
        .unwrap();

    // Export MemoryDag to OxiRS store
    let quad_count = engine.export_dag(&original_dag).expect("export dag");
    assert!(quad_count > 0);

    // Reconstruct MemoryDag from OxiRS store
    let loaded_dag: MemoryDag<TestNode> = engine.load_dag().expect("load dag");

    assert_eq!(loaded_dag.node_count(), 3);
    assert_eq!(loaded_dag.edge_count(), 2);
    assert!(loaded_dag.validate_acyclic().is_ok());

    let n1_children = loaded_dag.children_of(&"N1".to_string());
    assert_eq!(n1_children.len(), 1);
    assert_eq!(n1_children[0].id, "N2");

    let n2_children = loaded_dag.children_of(&"N2".to_string());
    assert_eq!(n2_children.len(), 1);
    assert_eq!(n2_children[0].id, "N3");
}

#[tokio::test]
async fn test_repository_capability_traits() {
    let engine = OxiRsTdbEngine::in_memory().unwrap();

    let node_a = TestNode::new("A", "Alpha", "Level1");
    let node_b = TestNode::new("B", "Beta", "Level2");

    let edge = EdgePayload::new("LINK", 0.85, ProvenanceKind::DeterministicRule).unwrap();

    // Test TransactionalGraphRepository
    let directives = vec![
        MutationDirective::UpsertNode(node_a.clone()),
        MutationDirective::UpsertNode(node_b.clone()),
        MutationDirective::AddEdge {
            parent: "A".to_string(),
            child: "B".to_string(),
            edge,
        },
    ];

    engine
        .apply_mutation_batch(&directives)
        .await
        .expect("apply_mutation_batch");

    // Test ReadOnlyGraphRepository trait methods
    let fetched = ReadOnlyGraphRepository::<TestNode>::get_node(&engine, &"A".to_string())
        .await
        .unwrap();
    assert_eq!(fetched, Some(node_a));

    let children = ReadOnlyGraphRepository::<TestNode>::get_children(&engine, &"A".to_string())
        .await
        .unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].id, "B");

    // Test VectorSearchRepository trait methods
    engine.index_embedding("A".to_string(), vec![1.0, 0.0, 0.0]);
    engine.index_embedding("B".to_string(), vec![0.0, 1.0, 0.0]);

    let nearest =
        VectorSearchRepository::<TestNode>::find_nearest_neighbors(&engine, &[0.9, 0.1, 0.0], 1)
            .await
            .unwrap();
    assert_eq!(nearest.len(), 1);
    assert_eq!(nearest[0].0.id, "A");
    assert!(nearest[0].1 > 0.8);
}

#[tokio::test]
async fn test_mmap_snapshot_cold_start() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("snapshot_db");

    let cat1 = TestNode::new("C1", "Hardware", "Category");
    let cat2 = TestNode::new("C2", "Fasteners", "Subcategory");

    // Phase 1: create persistent dataset and write snapshot
    {
        let engine = OxiRsTdbEngine::open_persistent(&db_path).unwrap();
        engine.insert_node(&cat1).unwrap();
        engine.insert_node(&cat2).unwrap();
        let edge = EdgePayload::new("PARENT", 1.0, ProvenanceKind::DeterministicRule).unwrap();
        engine.insert_edge(&cat1.id, &cat2.id, &edge).unwrap();
        engine.flush().unwrap();

        let snap_file = db_path.join("snapshot.oxsnap");
        engine.save_snapshot(&snap_file).expect("save snapshot");
        assert!(
            snap_file.exists(),
            "snapshot.oxsnap must be written to disk"
        );
    }

    // Phase 2: reopen store with snapshot present
    {
        let reopened = OxiRsTdbEngine::open_persistent(&db_path).unwrap();
        let fetched: Option<TestNode> = reopened.get_node(&"C1".to_string()).unwrap();
        assert_eq!(fetched, Some(cat1));

        let children: Vec<TestNode> = reopened.get_children(&"C1".to_string()).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].id, "C2");
    }
}
