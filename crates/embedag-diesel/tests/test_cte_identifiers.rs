//! The configured table and column names are quoted in the traversal SQL, so reserved words, spaces
//! and quotes in them work and cannot add to the statement.
#![cfg(feature = "sqlite")]

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use embedag_diesel::cte::{CteHierarchyBuilder, HierarchicalTableConfig};
use rstest::rstest;

/// `name` as a quoted SQL identifier.
fn q(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn connection(table: &str, id: &str, parent: &str, name: &str) -> SqliteConnection {
    let (table, id, parent, name) = (q(table), q(id), q(parent), q(name));
    let mut conn = SqliteConnection::establish(":memory:").expect("in-memory sqlite");
    conn.batch_execute(&format!(
        "CREATE TABLE {table} ({id} TEXT PRIMARY KEY, {parent} TEXT, {name} TEXT);\
         INSERT INTO {table} VALUES ('root', NULL, 'Root'), ('mid', 'root', 'Mid'), ('leaf', 'mid', 'Leaf');"
    ))
    .expect("schema");
    conn
}

#[rstest]
#[case::reserved_words("order", "group", "select", "from")]
#[case::spaces("my nodes", "node id", "parent id", "node name")]
#[case::quote_inside("a\"b", "i\"d", "p\"arent", "n\"ame")]
fn configured_names_are_quoted(
    #[case] table: &str,
    #[case] id: &str,
    #[case] parent: &str,
    #[case] name: &str,
) {
    let mut conn = connection(table, id, parent, name);
    let config = HierarchicalTableConfig::new(table, id, parent, name);

    let up = CteHierarchyBuilder::fetch_ancestors_sqlite(&mut conn, &config, "leaf", 5).unwrap();
    assert_eq!(
        up.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
        ["Mid", "Root"]
    );
    let down =
        CteHierarchyBuilder::fetch_descendants_sqlite(&mut conn, &config, "root", 5).unwrap();
    assert_eq!(
        down.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
        ["Mid", "Leaf"]
    );
}

#[test]
fn a_hostile_table_name_cannot_add_a_statement() {
    let mut conn = connection("nodes", "id", "parent_id", "name");
    let config =
        HierarchicalTableConfig::new("nodes; DROP TABLE nodes; --", "id", "parent_id", "name");

    // The name does not exist as a table, so the query fails, and `nodes` is still there.
    assert!(CteHierarchyBuilder::fetch_ancestors_sqlite(&mut conn, &config, "leaf", 5).is_err());
    let ok = HierarchicalTableConfig::new("nodes", "id", "parent_id", "name");
    assert_eq!(
        CteHierarchyBuilder::fetch_ancestors_sqlite(&mut conn, &ok, "leaf", 5)
            .unwrap()
            .len(),
        2
    );
}
