//! Upsert descriptors: their constructors and the SQL each dialect generates.

use embedag_diesel::ops::{ConflictStrategy, UpsertDescriptor};

#[test]
fn conflicts_can_be_ignored_or_update_chosen_columns() {
    let ignore = UpsertDescriptor::on_conflict_do_nothing("t", vec!["id", "name"], vec!["id"]);
    assert_eq!(ignore.strategy, ConflictStrategy::DoNothing);
    assert_eq!(
        ignore.build_sqlite_sql(),
        "INSERT INTO t (id, name) VALUES (?, ?) ON CONFLICT (id) DO NOTHING;"
    );

    let update =
        UpsertDescriptor::on_conflict_do_update("t", vec!["id", "name"], vec!["id"], vec!["name"]);
    assert!(
        matches!(update.strategy, ConflictStrategy::DoUpdate { ref columns } if columns == &["name"])
    );
    let postgres = update.build_postgres_sql();
    assert!(postgres.contains("VALUES ($1, $2)") && postgres.contains("name = EXCLUDED.name"));
}

#[test]
fn the_turso_dialect_matches_sqlite() {
    let descriptor = UpsertDescriptor::semantic_nodes();
    assert_eq!(descriptor.build_turso_sql(), descriptor.build_sqlite_sql());
    assert!(
        UpsertDescriptor::semantic_edges()
            .build_postgres_sql()
            .contains("ON CONFLICT (parent_id, child_id, relationship_type)")
    );
}
