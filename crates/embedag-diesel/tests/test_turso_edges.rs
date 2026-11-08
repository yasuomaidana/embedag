#![cfg(all(feature = "turso", feature = "turso-async"))]

mod common;

use common::attach_directives;
use embedag_diesel::harness::TursoHarness;
use embedag_diesel::migrations::run_turso_migration_async;
use embedag_diesel::ops::{apply_mutation_lifecycle_turso, batch_upsert_edges_turso};
use uuid::Uuid;

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS semantic_nodes (
    id TEXT PRIMARY KEY, business_key TEXT UNIQUE NOT NULL, entity_level TEXT NOT NULL,
    name TEXT NOT NULL, attributes TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS semantic_edges (
    parent_id TEXT NOT NULL, child_id TEXT NOT NULL, relationship_type TEXT NOT NULL,
    confidence REAL NOT NULL, provenance TEXT NOT NULL, created_at TEXT NOT NULL,
    PRIMARY KEY (parent_id, child_id, relationship_type)
);
"#;

#[tokio::test(flavor = "multi_thread")]
async fn empty_edge_batches_and_attached_parents() {
    let harness = TursoHarness::in_memory();
    let mut conn = harness.connect_async().await.expect("connect");
    run_turso_migration_async(&mut conn, SCHEMA_SQL)
        .await
        .expect("migrate");

    assert_eq!(batch_upsert_edges_turso(&mut conn, &[]).await.unwrap(), 0);

    let (parent, child) = (Uuid::new_v4(), Uuid::new_v4());
    let directives = attach_directives(parent, child);
    apply_mutation_lifecycle_turso(&mut conn, &directives)
        .await
        .expect("apply");
}

use embedag_diesel::migrations::{TursoConnectionMode, establish_turso_sync};
use rstest::rstest;
use std::path::PathBuf;

#[rstest]
#[case::in_memory(TursoConnectionMode::InMemory, ":memory:")]
#[case::file(
    TursoConnectionMode::LocalFile(PathBuf::from("/tmp/a.db")),
    "/tmp/a.db"
)]
#[case::remote_without_query(
    TursoConnectionMode::Remote { url: "libsql://x.turso.io".into(), auth_token: "t".into() },
    "libsql://x.turso.io?authToken=t"
)]
#[case::remote_with_query(
    TursoConnectionMode::Remote { url: "libsql://x.turso.io?tls=0".into(), auth_token: "t".into() },
    "libsql://x.turso.io?tls=0&authToken=t"
)]
#[case::remote_with_token(
    TursoConnectionMode::Remote { url: "libsql://x.turso.io?authToken=own".into(), auth_token: "t".into() },
    "libsql://x.turso.io?authToken=own"
)]
#[case::replica(
    TursoConnectionMode::Replica {
        local_path: PathBuf::from("/tmp/replica.db"),
        remote_url: "libsql://x.turso.io".into(),
        auth_token: "t".into(),
    },
    "/tmp/replica.db"
)]
fn connection_strings_follow_the_mode(#[case] mode: TursoConnectionMode, #[case] expected: &str) {
    assert_eq!(mode.to_connection_string(), expected);
}

#[test]
fn an_unreachable_replica_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let mode = TursoConnectionMode::Replica {
        local_path: dir.path().join("replica.db"),
        remote_url: "http://127.0.0.1:1".into(),
        auth_token: "t".into(),
    };
    assert!(establish_turso_sync(&mode).is_err());
}
