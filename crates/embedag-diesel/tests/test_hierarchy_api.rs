//! The hierarchy builders, the model trait and the table configuration, on SQLite.

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use embedag_diesel::cte::{
    CteHierarchyBuilder, HierarchicalModel, HierarchicalTableConfig, HierarchyItem,
};
use embedag_diesel::harness::connect_with_retry;
use embedag_diesel::ops::{EdgeUpsertRecord, MutationDirectiveRecord};
use embedag_diesel::traversal::RecursiveTraversalBuilder;
use rstest::{fixture, rstest};
use std::time::Duration;
use uuid::Uuid;

struct Dept;

impl HierarchicalModel for Dept {
    fn table_name() -> &'static str {
        "dept"
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

struct Described;

impl HierarchicalModel for Described {
    fn table_name() -> &'static str {
        "dept"
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
    fn table_description() -> Option<&'static str> {
        Some("Company departments")
    }
}

/// `root -> (a -> c), b`, in the model's table `dept` and in the default `semantic_nodes`.
#[fixture]
fn conn() -> SqliteConnection {
    let mut conn = SqliteConnection::establish(":memory:").expect("in-memory sqlite");
    for table in ["dept", "semantic_nodes"] {
        conn.batch_execute(&format!(
            "CREATE TABLE {table} (id TEXT PRIMARY KEY, parent_id TEXT, name TEXT NOT NULL);
             INSERT INTO {table} VALUES
               ('root', NULL, 'Root'), ('a', 'root', 'A'), ('b', 'root', 'B'), ('c', 'a', 'C');"
        ))
        .expect("seed");
    }
    conn
}

fn names(items: &[HierarchyItem]) -> Vec<&str> {
    items.iter().map(|i| i.name.as_str()).collect()
}

#[test]
fn a_table_configuration_describes_itself() {
    let plain = HierarchicalTableConfig::new("t", "id", "parent", "label");
    assert!(plain.description.is_none() && plain.max_breadth_per_node.is_none());
    assert!(!plain.table_context().contains("Description"));

    let described = plain
        .with_description("Things")
        .with_max_breadth_per_node(3);
    assert_eq!(described.max_breadth_per_node, Some(3));
    assert!(described.table_context().contains("- Description: Things"));

    let semantic = HierarchicalTableConfig::default_semantic();
    assert_eq!(semantic.table_name, "semantic_nodes");
}

#[test]
fn models_build_their_configuration_with_an_optional_description() {
    assert!(Dept::table_config().description.is_none());
    assert!(!Dept::table_context().contains("Description"));
    assert!(Described::table_context().contains("Company departments"));
}

#[rstest]
fn models_traverse_on_sqlite(mut conn: SqliteConnection) {
    let ancestors = Dept::fetch_ancestors_sqlite(&mut conn, "c", 10).unwrap();
    assert_eq!(names(&ancestors), ["A", "Root"]);
    let deduplicated = Dept::fetch_ancestors_dedup_sqlite(&mut conn, "c", 10).unwrap();
    assert_eq!(names(&deduplicated), ["A", "Root"]);
    let descendants = Dept::fetch_descendants_sqlite(&mut conn, "root", 10).unwrap();
    assert_eq!(descendants.len(), 3);
}

#[rstest]
fn the_builder_traverses_with_a_model_or_a_configuration(mut conn: SqliteConnection) {
    let ancestors = CteHierarchyBuilder::fetch_model_ancestors_sqlite::<Dept>(&mut conn, "c", 10);
    assert_eq!(names(&ancestors.unwrap()), ["A", "Root"]);
    let dedup = CteHierarchyBuilder::fetch_model_ancestors_dedup_sqlite::<Dept>(&mut conn, "c", 10);
    assert_eq!(dedup.unwrap().len(), 2);
    let descendants =
        CteHierarchyBuilder::fetch_model_descendants_sqlite::<Dept>(&mut conn, "a", 10);
    assert_eq!(names(&descendants.unwrap()), ["C"]);

    // At most one child per node: the alphabetically first child of the root.
    let limited = HierarchicalTableConfig::new("dept", "id", "parent_id", "name")
        .with_max_breadth_per_node(1);
    let bounded = CteHierarchyBuilder::fetch_descendants_sqlite(&mut conn, &limited, "root", 10);
    assert_eq!(names(&bounded.unwrap()), ["A", "C"]);
}

#[rstest]
fn the_default_semantic_table_can_be_traversed_with_breadth_limits(mut conn: SqliteConnection) {
    let up = RecursiveTraversalBuilder::fetch_ancestors_sqlite(&mut conn, "c", 10).unwrap();
    assert_eq!(names(&up), ["A", "Root"]);
    let down = RecursiveTraversalBuilder::fetch_descendants_sqlite(&mut conn, "root", 10).unwrap();
    assert_eq!(down.len(), 3);
    let bounded =
        RecursiveTraversalBuilder::fetch_descendants_bounded_sqlite(&mut conn, "root", 10, 1);
    assert_eq!(names(&bounded.unwrap()), ["A", "C"]);
    let bounded_up =
        RecursiveTraversalBuilder::fetch_ancestors_bounded_sqlite(&mut conn, "c", 10, 1);
    assert_eq!(bounded_up.unwrap().len(), 2);
}

#[test]
fn only_attach_directives_carry_an_edge() {
    let (parent, child) = (Uuid::new_v4(), Uuid::new_v4());
    let attach = MutationDirectiveRecord::AttachParent {
        parent_id: parent,
        child_id: child,
        relationship_type: "PARENT_OF".into(),
        confidence: 1.0,
        provenance: "test".into(),
    };
    let edge: EdgeUpsertRecord = attach.attached_edge().expect("an edge");
    assert_eq!((edge.parent_id, edge.child_id), (parent, child));

    let update = MutationDirectiveRecord::UpdateNodePayload {
        id: parent,
        updated_attributes: serde_json::json!({}),
    };
    assert!(update.attached_edge().is_none());
}

#[tokio::test]
async fn connecting_to_nothing_gives_up_after_the_attempts() {
    let unreachable = "host=127.0.0.1 port=1 user=postgres connect_timeout=1";
    assert!(
        connect_with_retry(unreachable, 2, Duration::from_millis(1))
            .await
            .is_none()
    );
}
