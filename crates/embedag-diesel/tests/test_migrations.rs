use diesel::Connection;
use embedag_diesel::migrations::{
    run_postgres_migration, run_sqlite_migration, setup_postgres, setup_sqlite,
};
use testcontainers::core::WaitFor;
use testcontainers::runners::AsyncRunner;
use testcontainers::{GenericImage, ImageExt};

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
    conn.batch_execute("INSERT INTO custom_taxonomies (id, parent_id, name) VALUES ('t1', NULL, 'Taxonomy 1');")
        .expect("insert row");

    // Run user custom migration down
    run_sqlite_migration(&mut conn, "DROP TABLE custom_taxonomies;").expect("run down");
}

#[tokio::test]
async fn test_postgres_pgvector_container_migration() {
    let image = GenericImage::new("pgvector/pgvector", "pg16")
        .with_exposed_port(5432.into())
        .with_wait_for(WaitFor::message_on_stderr(
            "database system is ready to accept connections",
        ))
        .with_env_var("POSTGRES_PASSWORD", "postgres")
        .with_env_var("POSTGRES_USER", "postgres")
        .with_env_var("POSTGRES_DB", "postgres");

    let container = image.start().await.expect("start pgvector container");
    let port = container.get_host_port_ipv4(5432).await.expect("port");

    let conn_str =
        format!("host=127.0.0.1 port={port} user=postgres password=postgres dbname=postgres");
    println!("Connecting to: {conn_str}");

    let mut client_opt = None;
    for i in 0..15 {
        match tokio_postgres::connect(&conn_str, tokio_postgres::NoTls).await {
            Ok((c, connection)) => {
                tokio::spawn(async move {
                    if let Err(e) = connection.await {
                        eprintln!("connection error: {}", e);
                    }
                });
                client_opt = Some(c);
                break;
            }
            Err(e) => {
                println!("Attempt {i} detailed error: {e:#?}");
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        }
    }

    let client = client_opt.expect("must connect to postgres container");

    // 1. Run baseline pgvector extension setup
    setup_postgres(&client).await.expect("setup postgres vector");

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
