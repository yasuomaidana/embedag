mod common;

use embedag_diesel::harness::PostgresHarness;
use embedag_diesel::ops::{
    batch_upsert_edges, batch_upsert_nodes, EdgeUpsertRecord, NodeUpsertRecord,
};
use embedag_diesel::traversal::RecursiveTraversalBuilder;
use uuid::Uuid;

#[tokio::test]
async fn test_recursive_ancestor_traversal() {
    let harness = PostgresHarness::start().await.expect("start harness");
    let client = harness.connect().await.expect("connect client");
    common::setup_graph_test_schema(&client).await;

    // Build 4-level lineage: Department (L1) -> Category (L2) -> Subcategory (L3) -> SKU (L4)
    let dept_id = Uuid::new_v4();
    let cat_id = Uuid::new_v4();
    let subcat_id = Uuid::new_v4();
    let sku_id = Uuid::new_v4();

    let nodes = vec![
        NodeUpsertRecord {
            id: dept_id,
            business_key: "dept_groceries".into(),
            entity_level: "department".into(),
            name: "Groceries".into(),
            attributes: serde_json::json!({}),
            embedding: None,
        },
        NodeUpsertRecord {
            id: cat_id,
            business_key: "cat_dairy".into(),
            entity_level: "category".into(),
            name: "Dairy".into(),
            attributes: serde_json::json!({}),
            embedding: None,
        },
        NodeUpsertRecord {
            id: subcat_id,
            business_key: "subcat_milk".into(),
            entity_level: "subcategory".into(),
            name: "Milk".into(),
            attributes: serde_json::json!({}),
            embedding: None,
        },
        NodeUpsertRecord {
            id: sku_id,
            business_key: "sku_oat_milk".into(),
            entity_level: "sku".into(),
            name: "Organic Oat Milk".into(),
            attributes: serde_json::json!({}),
            embedding: None,
        },
    ];
    batch_upsert_nodes(&client, &nodes)
        .await
        .expect("insert nodes");

    let edges = vec![
        EdgeUpsertRecord {
            parent_id: dept_id,
            child_id: cat_id,
            relationship_type: "PARENT_OF".into(),
            confidence: 1.0,
            provenance: "taxonomy".into(),
        },
        EdgeUpsertRecord {
            parent_id: cat_id,
            child_id: subcat_id,
            relationship_type: "PARENT_OF".into(),
            confidence: 1.0,
            provenance: "taxonomy".into(),
        },
        EdgeUpsertRecord {
            parent_id: subcat_id,
            child_id: sku_id,
            relationship_type: "PARENT_OF".into(),
            confidence: 1.0,
            provenance: "taxonomy".into(),
        },
    ];
    batch_upsert_edges(&client, &edges)
        .await
        .expect("insert edges");

    // Fetch full ancestor lineage of SKU in a single round-trip
    let ancestors = RecursiveTraversalBuilder::fetch_ancestors(&client, sku_id, 10)
        .await
        .expect("recursive CTE ancestors query");

    assert_eq!(
        ancestors.len(),
        3,
        "Must retrieve all 3 hierarchical ancestors"
    );
    assert_eq!(ancestors[0].depth, 1);
    assert_eq!(ancestors[0].business_key, "subcat_milk");
    assert_eq!(ancestors[1].depth, 2);
    assert_eq!(ancestors[1].business_key, "cat_dairy");
    assert_eq!(ancestors[2].depth, 3);
    assert_eq!(ancestors[2].business_key, "dept_groceries");

    // Fetch descendants from root Department
    let descendants = RecursiveTraversalBuilder::fetch_descendants(&client, dept_id, 10)
        .await
        .expect("recursive CTE descendants query");

    assert_eq!(
        descendants.len(),
        3,
        "Must retrieve all 3 hierarchical descendants"
    );
    assert_eq!(descendants[0].depth, 1);
    assert_eq!(descendants[0].business_key, "cat_dairy");
    assert_eq!(descendants[2].depth, 3);
    assert_eq!(descendants[2].business_key, "sku_oat_milk");
}

#[tokio::test]
async fn test_sql_cycle_guardrail() {
    let harness = PostgresHarness::start().await.expect("start harness");
    let client = harness.connect().await.expect("connect client");
    common::setup_graph_test_schema(&client).await;

    // Construct a cyclic triangle: Node A -> Node B -> Node C -> Node A
    let node_a = Uuid::new_v4();
    let node_b = Uuid::new_v4();
    let node_c = Uuid::new_v4();

    let nodes = vec![
        NodeUpsertRecord {
            id: node_a,
            business_key: "cycle_a".into(),
            entity_level: "node".into(),
            name: "A".into(),
            attributes: serde_json::json!({}),
            embedding: None,
        },
        NodeUpsertRecord {
            id: node_b,
            business_key: "cycle_b".into(),
            entity_level: "node".into(),
            name: "B".into(),
            attributes: serde_json::json!({}),
            embedding: None,
        },
        NodeUpsertRecord {
            id: node_c,
            business_key: "cycle_c".into(),
            entity_level: "node".into(),
            name: "C".into(),
            attributes: serde_json::json!({}),
            embedding: None,
        },
    ];
    batch_upsert_nodes(&client, &nodes)
        .await
        .expect("insert cycle nodes");

    let cyclic_edges = vec![
        EdgeUpsertRecord {
            parent_id: node_a,
            child_id: node_b,
            relationship_type: "CONNECTS".into(),
            confidence: 1.0,
            provenance: "test".into(),
        },
        EdgeUpsertRecord {
            parent_id: node_b,
            child_id: node_c,
            relationship_type: "CONNECTS".into(),
            confidence: 1.0,
            provenance: "test".into(),
        },
        // Back edge completing the cycle: C -> A
        EdgeUpsertRecord {
            parent_id: node_c,
            child_id: node_a,
            relationship_type: "CONNECTS".into(),
            confidence: 1.0,
            provenance: "test".into(),
        },
    ];
    batch_upsert_edges(&client, &cyclic_edges)
        .await
        .expect("insert cyclic edges");

    // Execute recursive traversal: the SQL cycle filter `NOT (parent_id = ANY(visited))`
    // must terminate cleanly without looping infinitely!
    let ancestors = RecursiveTraversalBuilder::fetch_ancestors(&client, node_c, 50)
        .await
        .expect("query with cycle must terminate cleanly via SQL guardrail");

    // Lineage from C: parent B (depth 1), grandparent A (depth 2). When trying to visit C again, guard stops it!
    assert_eq!(
        ancestors.len(),
        2,
        "Cycle guardrail must prune recursion at the cycle back-edge"
    );
    assert_eq!(ancestors[0].business_key, "cycle_b");
    assert_eq!(ancestors[1].business_key, "cycle_a");
}
