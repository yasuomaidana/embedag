mod common;

use embedag_diesel::harness::PostgresHarness;
use embedag_diesel::ops::{
    EdgeUpsertRecord, MutationDirectiveRecord, NodeUpsertRecord, apply_mutation_lifecycle,
    batch_upsert_edges, batch_upsert_nodes, execute_with_savepoint,
};
use uuid::Uuid;

#[tokio::test]
async fn test_transaction_rollback_on_constraint_violation() {
    let harness = PostgresHarness::start()
        .await
        .expect("start postgres harness");
    let mut client = harness.connect().await.expect("connect client");
    common::setup_graph_test_schema(&client).await;

    let tx = client.transaction().await.expect("start transaction");

    // 1. Insert a valid node in the transaction
    let valid_node_id = Uuid::new_v4();
    let valid_node = NodeUpsertRecord {
        attributes: serde_json::json!({"department": "grocery"}),
        ..NodeUpsertRecord::new(valid_node_id, "valid_root_1", "category", "Beverages")
    };
    batch_upsert_nodes(&tx, &[valid_node])
        .await
        .expect("upsert valid node");

    // 2. Execute an operation in a savepoint that fails a foreign key constraint
    let non_existent_parent = Uuid::new_v4();
    let savepoint_res = execute_with_savepoint(&tx, "sp_fk_test", || async {
        let bad_edge =
            EdgeUpsertRecord::new(non_existent_parent, valid_node_id, "CHILD_OF", 0.90, "test");
        batch_upsert_edges(&tx, &[bad_edge]).await
    })
    .await;

    // Verify savepoint failed due to foreign key violation
    assert!(savepoint_res.is_err(), "Foreign key violation must fail");

    // 3. Verify outer transaction was NOT poisoned and can still commit
    let child_node_id = Uuid::new_v4();
    let child_node = NodeUpsertRecord::new(child_node_id, "valid_child_1", "sku", "Tea");
    batch_upsert_nodes(&tx, &[child_node])
        .await
        .expect("subsequent insert in same outer tx must succeed");

    tx.commit()
        .await
        .expect("outer transaction must commit successfully");

    // 4. Verify in new connection: valid nodes exist, bad edge was rolled back
    let check_client = harness.connect().await.expect("connect check client");
    let node_rows = check_client
        .query(
            "SELECT count(*) FROM semantic_nodes WHERE id IN ($1, $2);",
            &[&valid_node_id, &child_node_id],
        )
        .await
        .expect("query nodes");
    let count: i64 = node_rows[0].get(0);
    assert_eq!(count, 2, "Both valid nodes must be committed");

    let edge_rows = check_client
        .query("SELECT count(*) FROM semantic_edges;", &[])
        .await
        .expect("query edges");
    let edge_count: i64 = edge_rows[0].get(0);
    assert_eq!(
        edge_count, 0,
        "No orphaned edge should exist after rollback"
    );
}

#[tokio::test]
async fn test_batch_upsert_performance() {
    let harness = PostgresHarness::start()
        .await
        .expect("start postgres harness");
    let client = harness.connect().await.expect("connect client");
    common::setup_graph_test_schema(&client).await;

    // Generate 1,000 nodes for high-throughput batch upsert
    let nodes: Vec<NodeUpsertRecord> = (0..1000)
        .map(|i| NodeUpsertRecord {
            id: Uuid::new_v4(),
            business_key: format!("batch_sku_{i}"),
            entity_level: "sku".into(),
            name: format!("Product {i}"),
            attributes: serde_json::json!({"index": i, "status": "active"}),
            embedding: None,
        })
        .collect();

    let inserted = batch_upsert_nodes(&client, &nodes)
        .await
        .expect("batch upsert 1000 nodes");
    assert_eq!(inserted, 1000);

    // Verify row count in database
    let rows = client
        .query("SELECT count(*) FROM semantic_nodes;", &[])
        .await
        .expect("query count");
    let count: i64 = rows[0].get(0);
    assert_eq!(count, 1000);

    // Verify idempotency on second run with attribute updates
    let updated_nodes: Vec<NodeUpsertRecord> = (0..1000)
        .map(|i| NodeUpsertRecord {
            id: nodes[i].id,
            business_key: format!("batch_sku_{i}"),
            entity_level: "sku".into(),
            name: format!("Product {i} Updated"),
            attributes: serde_json::json!({"index": i, "status": "verified"}),
            embedding: None,
        })
        .collect();

    let updated_count = batch_upsert_nodes(&client, &updated_nodes)
        .await
        .expect("re-running batch upsert with updates");
    assert_eq!(updated_count, 1000);

    let rows_after = client
        .query("SELECT count(*) FROM semantic_nodes;", &[])
        .await
        .expect("query count after");
    let count_after: i64 = rows_after[0].get(0);
    assert_eq!(
        count_after, 1000,
        "Idempotent upsert must not create duplicate rows"
    );
}

#[tokio::test]
async fn test_dynamic_mutation_lifecycle_commit() {
    let harness = PostgresHarness::start()
        .await
        .expect("start postgres harness");
    let mut client = harness.connect().await.expect("connect client");
    common::setup_graph_test_schema(&client).await;

    let tx = client.transaction().await.expect("start tx");

    let root_id = Uuid::new_v4();
    let old_parent_id = Uuid::new_v4();
    let new_parent_id = Uuid::new_v4();
    let child_id = Uuid::new_v4();

    let mutations = vec![
        MutationDirectiveRecord::InsertNode(NodeUpsertRecord::new(
            root_id,
            "root_dept",
            "department",
            "Supermarket",
        )),
        MutationDirectiveRecord::InsertNode(NodeUpsertRecord::new(
            old_parent_id,
            "old_cat",
            "category",
            "Old Category",
        )),
        MutationDirectiveRecord::InsertNode(NodeUpsertRecord::new(
            new_parent_id,
            "new_cat",
            "category",
            "New Category",
        )),
        MutationDirectiveRecord::InsertNode(NodeUpsertRecord {
            attributes: serde_json::json!({"volume_ml": 250}),
            ..NodeUpsertRecord::new(child_id, "item_cold_brew", "sku", "Cold Brew")
        }),
        // Connect child under old parent
        MutationDirectiveRecord::AttachParent {
            parent_id: old_parent_id,
            child_id,
            relationship_type: "BELONGS_TO".into(),
            confidence: 0.95,
            provenance: "initial".into(),
        },
        // Update attributes on child
        MutationDirectiveRecord::UpdateNodePayload {
            id: child_id,
            updated_attributes: serde_json::json!({"organic": true}),
        },
        // Retarget edge from old parent to new parent
        MutationDirectiveRecord::RetargetEdge {
            child_id,
            old_parent_id,
            new_parent_id,
            relationship_type: "BELONGS_TO".into(),
            confidence: 0.98,
        },
    ];

    apply_mutation_lifecycle(&tx, &mutations)
        .await
        .expect("mutation lifecycle application");

    tx.commit().await.expect("commit mutation lifecycle");

    // Verify results
    let check_client = harness.connect().await.expect("connect client");

    // Child must have merged attributes
    let child_row = check_client
        .query_one(
            "SELECT attributes FROM semantic_nodes WHERE id = $1;",
            &[&child_id],
        )
        .await
        .expect("query child");
    let attrs: serde_json::Value = child_row.get(0);
    assert_eq!(attrs["volume_ml"], 250);
    assert_eq!(attrs["organic"], true);

    // Edge must point from new_parent_id, not old_parent_id
    let edges = check_client
        .query(
            "SELECT parent_id FROM semantic_edges WHERE child_id = $1;",
            &[&child_id],
        )
        .await
        .expect("query edges");
    assert_eq!(edges.len(), 1);
    let current_parent: Uuid = edges[0].get(0);
    assert_eq!(current_parent, new_parent_id);
}
