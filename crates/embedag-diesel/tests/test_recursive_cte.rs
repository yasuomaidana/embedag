mod common;

use embedag_diesel::harness::PostgresHarness;
use embedag_diesel::ops::{batch_upsert_edges, batch_upsert_nodes, cycle_records, lineage_records};
use embedag_diesel::traversal::RecursiveTraversalBuilder;

#[tokio::test]
async fn test_recursive_ancestor_traversal() {
    let harness = PostgresHarness::start().await.expect("start harness");
    let client = harness.connect().await.expect("connect client");
    common::setup_graph_test_schema(&client).await;

    // Build 4-level lineage: Department (L1) -> Category (L2) -> Subcategory (L3) -> SKU (L4)
    let (nodes, edges) = lineage_records(
        &[
            ("dept_groceries", "department", "Groceries"),
            ("cat_dairy", "category", "Dairy"),
            ("subcat_milk", "subcategory", "Milk"),
            ("sku_oat_milk", "sku", "Organic Oat Milk"),
        ],
        "PARENT_OF",
        "taxonomy",
    );
    let (dept_id, sku_id) = (nodes[0].id, nodes[3].id);
    batch_upsert_nodes(&client, &nodes)
        .await
        .expect("insert nodes");
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
    let (nodes, cyclic_edges) = cycle_records(
        &[
            ("cycle_a", "node", "A"),
            ("cycle_b", "node", "B"),
            ("cycle_c", "node", "C"),
        ],
        "CONNECTS",
        "test",
    );
    let node_c = nodes[2].id;
    batch_upsert_nodes(&client, &nodes)
        .await
        .expect("insert cycle nodes");
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
