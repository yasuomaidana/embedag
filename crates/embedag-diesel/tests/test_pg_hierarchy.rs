//! Async PostgreSQL hierarchy traversals, savepoints and empty batches against a container.

mod common;

use diesel_async::{AsyncConnection, AsyncPgConnection, SimpleAsyncConnection};
use embedag_diesel::cte::{CteHierarchyBuilder, HierarchicalModel, HierarchyItem};
use embedag_diesel::harness::PostgresHarness;
use embedag_diesel::ops::{
    MutationDirectiveRecord, NodeUpsertRecord, apply_mutation_lifecycle, batch_upsert_edges,
    batch_upsert_nodes, execute_with_savepoint,
};
use embedag_diesel::traversal::RecursiveTraversalBuilder;
use uuid::Uuid;

struct Team;

impl HierarchicalModel for Team {
    fn table_name() -> &'static str {
        "team"
    }
    fn id_column() -> &'static str {
        "id"
    }
    fn parent_column() -> &'static str {
        "parent_id"
    }
    fn name_column() -> &'static str {
        "name"
    }
}

fn names(items: &[HierarchyItem]) -> Vec<&str> {
    items.iter().map(|i| i.name.as_str()).collect()
}

/// `root -> (a -> c), b` in `team` and in the default `semantic_nodes` shape, with UUID keys.
async fn seeded_connection(harness: &PostgresHarness) -> AsyncPgConnection {
    let mut conn = AsyncPgConnection::establish(&harness.conn_str)
        .await
        .expect("diesel-async connection");
    for table in ["team", "semantic_nodes"] {
        conn.batch_execute(&format!(
            "CREATE TABLE {table} (id UUID PRIMARY KEY, parent_id UUID, name TEXT NOT NULL);
             INSERT INTO {table} VALUES
               ('00000000-0000-0000-0000-000000000001', NULL, 'Root'),
               ('00000000-0000-0000-0000-00000000000a', '00000000-0000-0000-0000-000000000001', 'A'),
               ('00000000-0000-0000-0000-00000000000b', '00000000-0000-0000-0000-000000000001', 'B'),
               ('00000000-0000-0000-0000-00000000000c', '00000000-0000-0000-0000-00000000000a', 'C');"
        ))
        .await
        .expect("seed");
    }
    conn
}

const ROOT: &str = "00000000-0000-0000-0000-000000000001";
const LEAF: &str = "00000000-0000-0000-0000-00000000000c";

#[tokio::test]
#[ignore = "starts a Postgres container: runs in the persistence-testcontainers and coverage jobs"]
async fn hierarchies_are_traversed_asynchronously_on_postgres() {
    let harness = PostgresHarness::start().await.expect("start harness");
    let mut conn = seeded_connection(&harness).await;

    let up = Team::fetch_ancestors_pg_async(&mut conn, LEAF, 10)
        .await
        .unwrap();
    assert_eq!(names(&up), ["A", "Root"]);
    let down = Team::fetch_descendants_pg_async(&mut conn, ROOT, 10)
        .await
        .unwrap();
    assert_eq!(down.len(), 3);

    let model_up = CteHierarchyBuilder::fetch_model_ancestors_pg_async::<Team>(&mut conn, LEAF, 10);
    assert_eq!(model_up.await.unwrap().len(), 2);
    let model_down =
        CteHierarchyBuilder::fetch_model_descendants_pg_async::<Team>(&mut conn, ROOT, 10);
    assert_eq!(model_down.await.unwrap().len(), 3);

    let semantic_up = RecursiveTraversalBuilder::fetch_ancestors_async(&mut conn, LEAF, 10);
    assert_eq!(names(&semantic_up.await.unwrap()), ["A", "Root"]);
    let semantic_down = RecursiveTraversalBuilder::fetch_descendants_async(&mut conn, ROOT, 10);
    assert_eq!(semantic_down.await.unwrap().len(), 3);
}

#[tokio::test]
#[ignore = "starts a Postgres container: runs in the persistence-testcontainers and coverage jobs"]
async fn savepoints_release_on_success_and_empty_batches_do_nothing() {
    let harness = PostgresHarness::start().await.expect("start harness");
    let mut client = harness.connect().await.expect("connect");
    common::setup_graph_test_schema(&client).await;

    assert_eq!(batch_upsert_nodes(&client, &[]).await.unwrap(), 0);
    assert_eq!(batch_upsert_edges(&client, &[]).await.unwrap(), 0);

    let tx = client.transaction().await.expect("transaction");
    let value = execute_with_savepoint(&tx, "sp_ok", || async { Ok(7) }).await;
    assert_eq!(value.unwrap(), 7);
    tx.commit().await.expect("commit");
}

#[tokio::test]
#[ignore = "starts a Postgres container: runs in the persistence-testcontainers and coverage jobs"]
async fn attaching_a_parent_writes_the_edge() {
    let harness = PostgresHarness::start().await.expect("start harness");
    let mut client = harness.connect().await.expect("connect");
    common::setup_graph_test_schema(&client).await;

    let (parent, child) = (Uuid::new_v4(), Uuid::new_v4());
    let tx = client.transaction().await.expect("transaction");
    let directives = [
        MutationDirectiveRecord::InsertNode(NodeUpsertRecord::new(parent, "p", "dept", "P")),
        MutationDirectiveRecord::InsertNode(NodeUpsertRecord::new(child, "c", "sku", "C")),
        MutationDirectiveRecord::AttachParent {
            parent_id: parent,
            child_id: child,
            relationship_type: "PARENT_OF".into(),
            confidence: 1.0,
            provenance: "test".into(),
        },
    ];
    apply_mutation_lifecycle(&tx, &directives)
        .await
        .expect("apply");
    tx.commit().await.expect("commit");

    let rows = client
        .query("SELECT count(*) FROM semantic_edges", &[])
        .await
        .unwrap();
    assert_eq!(rows[0].get::<_, i64>(0), 1);
}
