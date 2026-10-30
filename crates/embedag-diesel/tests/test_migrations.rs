use diesel::Connection;
use embedag_diesel::harness::PostgresHarness;
use embedag_diesel::migrations::{
    run_postgres_migration, run_sqlite_migration, setup_postgres, setup_sqlite,
};

#[test]
fn test_sqlite_user_migrations() {
    let mut conn = diesel::SqliteConnection::establish(":memory:").expect("in-memory sqlite");
    setup_sqlite(&mut conn).expect("setup sqlite");

    // Run user custom migrations up
    run_sqlite_migration(
        &mut conn,
        "CREATE TABLE custom_taxonomies (id TEXT PRIMARY KEY, parent_id TEXT, name TEXT NOT NULL);",
    )
    .expect("run up");

    use diesel::connection::SimpleConnection;
    conn.batch_execute(
        "INSERT INTO custom_taxonomies (id, parent_id, name) VALUES ('t1', NULL, 'Taxonomy 1');",
    )
    .expect("insert row");

    // Run user custom migration down
    run_sqlite_migration(&mut conn, "DROP TABLE custom_taxonomies;").expect("run down");
}

#[tokio::test]
async fn test_postgres_pgvector_container_migration() {
    let harness = PostgresHarness::start()
        .await
        .expect("start pgvector container");
    let client = harness
        .connect()
        .await
        .expect("must connect to postgres container");

    // 1. Run baseline pgvector extension setup
    setup_postgres(&client)
        .await
        .expect("setup postgres vector");

    // 2. Run user-defined migration
    run_postgres_migration(
        &client,
        "CREATE TABLE user_vectors (id UUID PRIMARY KEY, embedding vector(3));",
    )
    .await
    .expect("run user migration up");

    // Verify vector insertion works
    client
        .execute(
            "INSERT INTO user_vectors (id, embedding) VALUES (gen_random_uuid(), '[0.1, 0.2, 0.3]');",
            &[],
        )
        .await
        .expect("insert vector row");

    // Drop user table
    client
        .execute("DROP TABLE user_vectors;", &[])
        .await
        .expect("drop user table");
}
