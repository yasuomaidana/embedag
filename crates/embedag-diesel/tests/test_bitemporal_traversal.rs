mod common;

use chrono::{Duration, Utc};
use embedag_diesel::harness::PostgresHarness;
use embedag_diesel::ops::{NodeUpsertRecord, batch_upsert_nodes};
use embedag_diesel::traversal::RecursiveTraversalBuilder;
use uuid::Uuid;

#[tokio::test]
async fn test_point_in_time_dag_traversal() {
    let harness = PostgresHarness::start()
        .await
        .expect("start postgres container");
    let client = harness.connect().await.expect("connect client");
    common::setup_graph_test_schema(&client).await;

    let cat_old_id = Uuid::new_v4();
    let cat_new_id = Uuid::new_v4();
    let product_id = Uuid::new_v4();

    let nodes = vec![
        NodeUpsertRecord::new(
            cat_old_id,
            "cat_confectionery_old",
            "category",
            "Confectionery (Historic)",
        ),
        NodeUpsertRecord::new(
            cat_new_id,
            "cat_chocolate_new",
            "category",
            "Chocolate (Modern)",
        ),
        NodeUpsertRecord::new(product_id, "sku_carlos_v", "sku", "Carlos V Dark 50g"),
    ];
    batch_upsert_nodes(&client, &nodes)
        .await
        .expect("insert test nodes");

    // Timeline setup:
    // T0: 10 days ago (Old edge active)
    // T1: 5 days ago (Reorganization: Old edge ends, New edge begins)
    // T2: Now (New edge active)
    let now = Utc::now();
    let t_past_active = now - Duration::days(8);
    let t_reorg = now - Duration::days(5);
    let t_past_start = now - Duration::days(10);

    // Old Edge: valid from -10d to -5d
    let insert_old_edge = format!(
        "INSERT INTO semantic_edges (parent_id, child_id, relationship_type, confidence, provenance, valid_from, valid_to, created_at)
         VALUES ('{cat_old_id}', '{product_id}', 'BELONGS_TO', 1.0, 'legacy_tax', '{t_past_start}', '{t_reorg}', '{t_past_start}');"
    );
    client
        .batch_execute(&insert_old_edge)
        .await
        .expect("insert historical old edge");

    // New Edge: valid from -5d to NULL (current)
    let insert_new_edge = format!(
        "INSERT INTO semantic_edges (parent_id, child_id, relationship_type, confidence, provenance, valid_from, valid_to, created_at)
         VALUES ('{cat_new_id}', '{product_id}', 'BELONGS_TO', 1.0, 'new_tax', '{t_reorg}', NULL, '{t_reorg}');"
    );
    client
        .batch_execute(&insert_new_edge)
        .await
        .expect("insert modern edge");

    // 1. Point-in-time traversal at T_past (-8 days): Must resolve to Historic parent
    let past_ancestors =
        RecursiveTraversalBuilder::fetch_ancestors_as_of(&client, product_id, 10, t_past_active)
            .await
            .expect("point-in-time past ancestor query");

    assert_eq!(
        past_ancestors.len(),
        1,
        "Must retrieve exactly 1 historical ancestor"
    );
    assert_eq!(
        past_ancestors[0].business_key, "cat_confectionery_old",
        "At T0 product belonged to Confectionery"
    );

    // 2. Point-in-time traversal at Now: Must resolve to Modern parent
    let modern_ancestors =
        RecursiveTraversalBuilder::fetch_ancestors_as_of(&client, product_id, 10, now)
            .await
            .expect("point-in-time modern ancestor query");

    assert_eq!(
        modern_ancestors.len(),
        1,
        "Must retrieve exactly 1 modern ancestor"
    );
    assert_eq!(
        modern_ancestors[0].business_key, "cat_chocolate_new",
        "At Now product belongs to Chocolate"
    );
}
