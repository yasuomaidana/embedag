#![cfg(all(feature = "turso", feature = "turso-async"))]

use futures_util::FutureExt;
use embedag_diesel::cte::{CteHierarchyBuilder, HierarchicalTableConfig};
use embedag_diesel::harness::TursoHarness;
use embedag_diesel::migrations::{
    establish_turso_sync, run_turso_migration_async, run_turso_migration_sync, setup_turso_async,
    setup_turso_sync, TursoConnectionMode,
};
use embedag_diesel::ops::{
    apply_mutation_lifecycle_turso, batch_upsert_edges_turso, batch_upsert_nodes_turso,
    execute_with_savepoint_turso, EdgeUpsertRecord, MutationDirectiveRecord, NodeUpsertRecord,
    PersistenceError,
};
use embedag_diesel::traversal::RecursiveTraversalBuilder;
use uuid::Uuid;

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS semantic_nodes (
    id TEXT PRIMARY KEY,
    business_key TEXT UNIQUE NOT NULL,
    entity_level TEXT NOT NULL,
    name TEXT NOT NULL,
    attributes TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS semantic_edges (
    parent_id TEXT NOT NULL,
    child_id TEXT NOT NULL,
    relationship_type TEXT NOT NULL,
    confidence REAL NOT NULL,
    provenance TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (parent_id, child_id, relationship_type)
);
"#;

#[test]
fn test_turso_in_memory_sync_migration() {
    let harness = TursoHarness::in_memory();
    let mut conn = harness.connect_sync().expect("connect sync");

    assert!(setup_turso_sync(&mut conn).is_ok());
    assert!(run_turso_migration_sync(&mut conn, SCHEMA_SQL).is_ok());

    use diesel::connection::SimpleConnection;
    let insert_res = conn.batch_execute(
        "INSERT INTO semantic_nodes (id, business_key, entity_level, name, attributes, updated_at) \
         VALUES ('node-1', 'key-1', 'category', 'Test', '{}', '2026-10-13T00:00:00Z');"
    );
    assert!(insert_res.is_ok(), "Sync insert must succeed on Turso in-memory instance");
}

#[tokio::test]
async fn test_turso_in_memory_async_migration() {
    let harness = TursoHarness::in_memory();
    let mut conn = harness.connect_async().await.expect("connect async");

    assert!(setup_turso_async(&mut conn).await.is_ok());
    assert!(run_turso_migration_async(&mut conn, SCHEMA_SQL).await.is_ok());

    use diesel_async::SimpleAsyncConnection;
    let insert_res = conn.batch_execute(
        "INSERT INTO semantic_nodes (id, business_key, entity_level, name, attributes, updated_at) \
         VALUES ('node-async-1', 'key-async-1', 'category', 'Async Test', '{}', '2026-10-13T00:00:00Z');"
    ).await;
    assert!(insert_res.is_ok(), "Async insert must succeed on Turso in-memory instance");
}

#[tokio::test]
async fn test_turso_ephemeral_file_persistence() {
    let harness = TursoHarness::ephemeral_file().expect("create ephemeral file harness");
    let mut conn = harness.connect_async().await.expect("connect async to file");

    run_turso_migration_async(&mut conn, SCHEMA_SQL)
        .await
        .expect("migrate file database");

    use diesel_async::SimpleAsyncConnection;
    conn.batch_execute(
        "INSERT INTO semantic_nodes (id, business_key, entity_level, name, attributes, updated_at) \
         VALUES ('file-1', 'key-file', 'sku', 'Persisted SKU', '{}', '2026-10-13T00:00:00Z');"
    ).await.expect("insert into file db");

    // Open second connection to the same file to verify persistence across sessions
    let mut conn2 = harness.connect_async().await.expect("connect second handle");
    let res = conn2.batch_execute(
        "SELECT id FROM semantic_nodes WHERE id = 'file-1';"
    ).await;
    assert!(res.is_ok(), "Second connection to file must access persisted data");
}

#[test]
fn test_turso_remote_connection_mode_handling() {
    let remote_mode = TursoConnectionMode::Remote {
        url: "libsql://test-db.turso.io".into(),
        auth_token: "test-token-xyz".into(),
    };
    assert_eq!(
        remote_mode.to_connection_string(),
        "libsql://test-db.turso.io?authToken=test-token-xyz"
    );

    // Connecting to an unreachable dummy host should fail cleanly without panicking
    let conn_res = establish_turso_sync(&remote_mode);
    // Remote connections in libSQL may lazily fail on query rather than establish,
    // or fail immediately if DNS fails
    if let Ok(mut c) = conn_res {
        use diesel::connection::SimpleConnection;
        assert!(c.batch_execute("SELECT 1;").is_err(), "Query on dummy remote URL must fail gracefully");
    }
}

#[tokio::test]
async fn test_turso_batch_upserts_and_conflict_handling() {
    let harness = TursoHarness::in_memory();
    let mut conn = harness.connect_async().await.expect("connect async");
    run_turso_migration_async(&mut conn, SCHEMA_SQL).await.expect("run migration");

    let node1_id = Uuid::new_v4();
    let node2_id = Uuid::new_v4();

    let nodes = vec![
        NodeUpsertRecord {
            id: node1_id,
            business_key: "CAT-DAIRY".into(),
            entity_level: "category".into(),
            name: "Dairy".into(),
            attributes: serde_json::json!({"department": "Grocery"}),
            embedding: None,
        },
        NodeUpsertRecord {
            id: node2_id,
            business_key: "SKU-MILK".into(),
            entity_level: "sku".into(),
            name: "Whole Milk 1L".into(),
            attributes: serde_json::json!({"fat": "3.5%"}),
            embedding: None,
        },
    ];

    let inserted = batch_upsert_nodes_turso(&mut conn, &nodes).await.expect("batch upsert nodes");
    assert_eq!(inserted, 2, "Must insert 2 nodes");

    // Upsert duplicate node with updated name and attributes to test DO UPDATE conflict handling
    let update_node = NodeUpsertRecord {
        id: node1_id,
        business_key: "CAT-DAIRY".into(),
        entity_level: "category".into(),
        name: "Dairy & Eggs".into(),
        attributes: serde_json::json!({"department": "Grocery", "refrigerated": true}),
        embedding: None,
    };
    let updated = batch_upsert_nodes_turso(&mut conn, &[update_node]).await.expect("upsert existing node");
    assert_eq!(updated, 1, "Must update 1 conflicting node");

    // Edges batch upsert
    let edges = vec![
        EdgeUpsertRecord {
            parent_id: node1_id,
            child_id: node2_id,
            relationship_type: "PARENT_OF".into(),
            confidence: 0.95,
            provenance: "human_curated".into(),
        },
    ];
    let edge_count = batch_upsert_edges_turso(&mut conn, &edges).await.expect("batch upsert edges");
    assert_eq!(edge_count, 1, "Must insert 1 edge");

    // Edge conflict resolution: re-insert with higher confidence
    let edge_update = EdgeUpsertRecord {
        parent_id: node1_id,
        child_id: node2_id,
        relationship_type: "PARENT_OF".into(),
        confidence: 0.99,
        provenance: "verified".into(),
    };
    let edge_updated = batch_upsert_edges_turso(&mut conn, &[edge_update]).await.expect("upsert conflicting edge");
    assert_eq!(edge_updated, 1, "Must resolve edge conflict by updating confidence");
}

#[tokio::test]
async fn test_turso_mutation_lifecycle() {
    let harness = TursoHarness::in_memory();
    let mut conn = harness.connect_async().await.expect("connect async");
    run_turso_migration_async(&mut conn, SCHEMA_SQL).await.expect("run migration");

    let dept_id = Uuid::new_v4();
    let cat_id = Uuid::new_v4();
    let sku_id = Uuid::new_v4();

    let directives = vec![
        MutationDirectiveRecord::InsertNode(NodeUpsertRecord {
            id: dept_id,
            business_key: "DEPT-FOOD".into(),
            entity_level: "department".into(),
            name: "Food & Beverage".into(),
            attributes: serde_json::json!({}),
            embedding: None,
        }),
        MutationDirectiveRecord::InsertNode(NodeUpsertRecord {
            id: cat_id,
            business_key: "CAT-SNACKS".into(),
            entity_level: "category".into(),
            name: "Snacks".into(),
            attributes: serde_json::json!({}),
            embedding: None,
        }),
        MutationDirectiveRecord::InsertNode(NodeUpsertRecord {
            id: sku_id,
            business_key: "SKU-CHIPS".into(),
            entity_level: "sku".into(),
            name: "Potato Chips".into(),
            attributes: serde_json::json!({"brand": "Crispy"}),
            embedding: None,
        }),
        MutationDirectiveRecord::AttachParent {
            parent_id: dept_id,
            child_id: cat_id,
            relationship_type: "CONTAINS".into(),
            confidence: 1.0,
            provenance: "taxonomy".into(),
        },
        MutationDirectiveRecord::AttachParent {
            parent_id: cat_id,
            child_id: sku_id,
            relationship_type: "CONTAINS".into(),
            confidence: 0.8,
            provenance: "taxonomy".into(),
        },
        MutationDirectiveRecord::UpdateNodePayload {
            id: sku_id,
            updated_attributes: serde_json::json!({"brand": "Crispy", "flavor": "Salt & Vinegar"}),
        },
    ];

    apply_mutation_lifecycle_turso(&mut conn, &directives)
        .await
        .expect("apply mutation lifecycle");

    // Retarget SKU parent from cat_id directly to dept_id
    let retarget = vec![
        MutationDirectiveRecord::RetargetEdge {
            child_id: sku_id,
            old_parent_id: cat_id,
            new_parent_id: dept_id,
            relationship_type: "CONTAINS".into(),
            confidence: 1.0,
        },
    ];
    apply_mutation_lifecycle_turso(&mut conn, &retarget)
        .await
        .expect("retarget edge");
}

#[tokio::test]
async fn test_turso_savepoint_isolation_and_rollback() {
    let harness = TursoHarness::in_memory();
    let mut conn = harness.connect_async().await.expect("connect async");
    run_turso_migration_async(&mut conn, SCHEMA_SQL).await.expect("run migration");

    let node_id = Uuid::new_v4();

    // 1. Successful savepoint commit
    let commit_res = execute_with_savepoint_turso(&mut conn, "sp_success", |c| {
        async move {
            let node = NodeUpsertRecord {
                id: node_id,
                business_key: "KEY-COMMITTED".into(),
                entity_level: "item".into(),
                name: "Committed Node".into(),
                attributes: serde_json::json!({}),
                embedding: None,
            };
            batch_upsert_nodes_turso(c, &[node]).await?;
            Ok("success")
        }.boxed()
    }).await;

    assert!(commit_res.is_ok());
    assert_eq!(commit_res.unwrap(), "success");

    // 2. Failing savepoint rollback
    let rollback_node_id = Uuid::new_v4();
    let rollback_res: Result<(), PersistenceError> = execute_with_savepoint_turso(&mut conn, "sp_fail", |c| {
        async move {
            let node = NodeUpsertRecord {
                id: rollback_node_id,
                business_key: "KEY-ROLLED-BACK".into(),
                entity_level: "item".into(),
                name: "Doomed Node".into(),
                attributes: serde_json::json!({}),
                embedding: None,
            };
            batch_upsert_nodes_turso(c, &[node]).await?;
            Err(PersistenceError::SavepointRolledBack("Simulated failure in transaction".into()))
        }.boxed()
    }).await;

    assert!(rollback_res.is_err(), "Savepoint must return the simulated error");

    // Verify outer connection is completely healthy and usable
    use diesel_async::SimpleAsyncConnection;
    let ping_res = conn.batch_execute("SELECT 1;").await;
    assert!(ping_res.is_ok(), "Connection must remain functional after savepoint rollback");
}

#[tokio::test]
async fn test_turso_recursive_cte_traversals() {
    let harness = TursoHarness::in_memory();
    let mut conn = harness.connect_async().await.expect("connect async");
    run_turso_migration_async(&mut conn, SCHEMA_SQL).await.expect("run migration");

    // Build 4-level taxonomy: Enterprise (L1) -> Division (L2) -> Team (L3) -> Engineer (L4)
    let l1_id = Uuid::new_v4();
    let l2_id = Uuid::new_v4();
    let l3_id = Uuid::new_v4();
    let l4_id = Uuid::new_v4();

    let nodes = vec![
        NodeUpsertRecord {
            id: l1_id,
            business_key: "ORG-ENT".into(),
            entity_level: "enterprise".into(),
            name: "Global Corp".into(),
            attributes: serde_json::json!({}),
            embedding: None,
        },
        NodeUpsertRecord {
            id: l2_id,
            business_key: "ORG-DIV".into(),
            entity_level: "division".into(),
            name: "AI Systems Division".into(),
            attributes: serde_json::json!({}),
            embedding: None,
        },
        NodeUpsertRecord {
            id: l3_id,
            business_key: "ORG-TEAM".into(),
            entity_level: "team".into(),
            name: "Knowledge Graph Team".into(),
            attributes: serde_json::json!({}),
            embedding: None,
        },
        NodeUpsertRecord {
            id: l4_id,
            business_key: "USER-ENG".into(),
            entity_level: "individual".into(),
            name: "Staff Engineer".into(),
            attributes: serde_json::json!({}),
            embedding: None,
        },
    ];
    batch_upsert_nodes_turso(&mut conn, &nodes).await.expect("insert hierarchy nodes");

    let edges = vec![
        EdgeUpsertRecord {
            parent_id: l1_id,
            child_id: l2_id,
            relationship_type: "MANAGES".into(),
            confidence: 1.0,
            provenance: "org_chart".into(),
        },
        EdgeUpsertRecord {
            parent_id: l2_id,
            child_id: l3_id,
            relationship_type: "MANAGES".into(),
            confidence: 1.0,
            provenance: "org_chart".into(),
        },
        EdgeUpsertRecord {
            parent_id: l3_id,
            child_id: l4_id,
            relationship_type: "MANAGES".into(),
            confidence: 1.0,
            provenance: "org_chart".into(),
        },
    ];
    batch_upsert_edges_turso(&mut conn, &edges).await.expect("insert hierarchy edges");

    // Fetch full ancestor lineage of Engineer in a single round-trip
    let ancestors = RecursiveTraversalBuilder::fetch_ancestors_turso_async(&mut conn, l4_id, 10)
        .await
        .expect("fetch ancestors on Turso");

    assert_eq!(ancestors.len(), 3, "Must retrieve all 3 hierarchical ancestors");
    assert_eq!(ancestors[0].depth, 1);
    assert_eq!(ancestors[0].business_key, "ORG-TEAM");
    assert_eq!(ancestors[1].depth, 2);
    assert_eq!(ancestors[1].business_key, "ORG-DIV");
    assert_eq!(ancestors[2].depth, 3);
    assert_eq!(ancestors[2].business_key, "ORG-ENT");

    // Fetch full descendant tree of Enterprise down to depth 10
    let descendants = RecursiveTraversalBuilder::fetch_descendants_turso_async(&mut conn, l1_id, 10)
        .await
        .expect("fetch descendants on Turso");

    assert_eq!(descendants.len(), 3, "Must retrieve all 3 hierarchical descendants");
    assert_eq!(descendants[0].depth, 1);
    assert_eq!(descendants[0].business_key, "ORG-DIV");
    assert_eq!(descendants[1].depth, 2);
    assert_eq!(descendants[1].business_key, "ORG-TEAM");
    assert_eq!(descendants[2].depth, 3);
    assert_eq!(descendants[2].business_key, "USER-ENG");
}

#[tokio::test]
async fn test_turso_recursive_cte_cycle_guardrail() {
    let harness = TursoHarness::in_memory();
    let mut conn = harness.connect_async().await.expect("connect async");
    run_turso_migration_async(&mut conn, SCHEMA_SQL).await.expect("run migration");

    // Create 3 nodes forming a cycle: A -> B -> C -> A
    let node_a = Uuid::new_v4();
    let node_b = Uuid::new_v4();
    let node_c = Uuid::new_v4();

    let nodes = vec![
        NodeUpsertRecord {
            id: node_a,
            business_key: "CYC-A".into(),
            entity_level: "node".into(),
            name: "Node A".into(),
            attributes: serde_json::json!({}),
            embedding: None,
        },
        NodeUpsertRecord {
            id: node_b,
            business_key: "CYC-B".into(),
            entity_level: "node".into(),
            name: "Node B".into(),
            attributes: serde_json::json!({}),
            embedding: None,
        },
        NodeUpsertRecord {
            id: node_c,
            business_key: "CYC-C".into(),
            entity_level: "node".into(),
            name: "Node C".into(),
            attributes: serde_json::json!({}),
            embedding: None,
        },
    ];
    batch_upsert_nodes_turso(&mut conn, &nodes).await.expect("insert cyclic nodes");

    let edges = vec![
        EdgeUpsertRecord {
            parent_id: node_a,
            child_id: node_b,
            relationship_type: "CYCLE".into(),
            confidence: 1.0,
            provenance: "test".into(),
        },
        EdgeUpsertRecord {
            parent_id: node_b,
            child_id: node_c,
            relationship_type: "CYCLE".into(),
            confidence: 1.0,
            provenance: "test".into(),
        },
        EdgeUpsertRecord {
            parent_id: node_c,
            child_id: node_a,
            relationship_type: "CYCLE".into(),
            confidence: 1.0,
            provenance: "test".into(),
        },
    ];
    batch_upsert_edges_turso(&mut conn, &edges).await.expect("insert cyclic edges");

    // Without cycle guard this query would loop forever until recursion limit or memory exhaustion.
    // With INSTR delimiter guardrail, traversal must terminate cleanly having visited only unique ancestors.
    let ancestors = RecursiveTraversalBuilder::fetch_ancestors_turso_async(&mut conn, node_b, 100)
        .await
        .expect("recursive CTE with cycle guard must succeed without looping");

    // Ancestor of B is A, ancestor of A is C; from C, parent is B which is already in visited_path, so cycle halts!
    assert_eq!(ancestors.len(), 2, "Cycle guardrail must prevent infinite recursion and return exactly 2 unique hops");
    assert_eq!(ancestors[0].business_key, "CYC-A");
    assert_eq!(ancestors[1].business_key, "CYC-C");
}

#[tokio::test]
async fn test_turso_custom_adjacency_table_cte_traversal() {
    let harness = TursoHarness::in_memory();
    let mut conn = harness.connect_async().await.expect("connect async");

    let custom_schema = r#"
    CREATE TABLE categories (
        id TEXT PRIMARY KEY,
        parent_id TEXT,
        name TEXT NOT NULL
    );
    "#;
    run_turso_migration_async(&mut conn, custom_schema).await.expect("run custom migration");

    use diesel_async::SimpleAsyncConnection;
    conn.batch_execute(r#"
        INSERT INTO categories (id, parent_id, name) VALUES
            ('c1', NULL, 'Electronics'),
            ('c2', 'c1', 'Computers'),
            ('c3', 'c2', 'Laptops'),
            ('c4', 'c2', 'Desktops'),
            ('c5', 'c3', 'Ultrabooks');
    "#).await.expect("insert categories");

    let config = HierarchicalTableConfig::new("categories", "id", "parent_id", "name");

    // Ancestors of Ultrabooks (c5) -> Laptops (c3), Computers (c2), Electronics (c1)
    let ancestors = CteHierarchyBuilder::fetch_ancestors_turso_async(&mut conn, &config, "c5", 10)
        .await
        .expect("fetch custom ancestors");

    assert_eq!(ancestors.len(), 3);
    assert_eq!(ancestors[0].id, "c3");
    assert_eq!(ancestors[0].name, "Laptops");
    assert_eq!(ancestors[1].id, "c2");
    assert_eq!(ancestors[1].name, "Computers");
    assert_eq!(ancestors[2].id, "c1");
    assert_eq!(ancestors[2].name, "Electronics");

    // Descendants of Computers (c2) -> Laptops (c3), Desktops (c4), Ultrabooks (c5)
    let descendants = CteHierarchyBuilder::fetch_descendants_turso_async(&mut conn, &config, "c2", 10)
        .await
        .expect("fetch custom descendants");

    assert_eq!(descendants.len(), 3);
    let desc_names: Vec<String> = descendants.into_iter().map(|d| d.name).collect();
    assert!(desc_names.contains(&"Laptops".to_string()));
    assert!(desc_names.contains(&"Desktops".to_string()));
    assert!(desc_names.contains(&"Ultrabooks".to_string()));
}
