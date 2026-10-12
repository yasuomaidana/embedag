use thiserror::Error;

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("Diesel database error: {0}")]
    Diesel(#[from] diesel::result::Error),

    #[cfg(feature = "postgres-async")]
    #[error("PostgreSQL driver error: {0}")]
    Postgres(#[from] tokio_postgres::Error),

    #[error("Migration step failed: {0}")]
    ExecutionFailed(String),
}

/// Baseline extension setup for PostgreSQL pgvector.
///
/// Users define their own tables, columns, indexes, and migrations.
/// embedag-diesel only prepares necessary engine extensions.
pub const POSTGRES_SETUP: &str = "CREATE EXTENSION IF NOT EXISTS vector;";

/// Baseline SQLite configuration.
pub const SQLITE_SETUP: &str = "PRAGMA foreign_keys = ON;";

/// Initializes SQLite database prerequisites.
#[cfg(feature = "sqlite")]
pub fn setup_sqlite(conn: &mut diesel::SqliteConnection) -> Result<(), MigrationError> {
    use diesel::connection::SimpleConnection;
    conn.batch_execute(SQLITE_SETUP)?;
    Ok(())
}

/// Initializes PostgreSQL database extensions.
#[cfg(feature = "postgres-async")]
pub async fn setup_postgres(client: &tokio_postgres::Client) -> Result<(), MigrationError> {
    client.batch_execute(POSTGRES_SETUP).await?;
    Ok(())
}

/// Executes a user-defined SQL migration on SQLite.
#[cfg(feature = "sqlite")]
pub fn run_sqlite_migration(
    conn: &mut diesel::SqliteConnection,
    migration_sql: &str,
) -> Result<(), MigrationError> {
    use diesel::connection::SimpleConnection;
    setup_sqlite(conn)?;
    conn.batch_execute(migration_sql)?;
    Ok(())
}

/// Executes a user-defined SQL migration on PostgreSQL.
#[cfg(feature = "postgres-async")]
pub async fn run_postgres_migration(
    client: &tokio_postgres::Client,
    migration_sql: &str,
) -> Result<(), MigrationError> {
    setup_postgres(client).await?;
    client.batch_execute(migration_sql).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::Connection;

    #[test]
    fn test_sqlite_setup_lifecycle() {
        let mut conn = diesel::SqliteConnection::establish(":memory:").expect("in-memory sqlite");

        // Run baseline setup
        assert!(setup_sqlite(&mut conn).is_ok());

        // Run custom user migration
        let user_migration = "CREATE TABLE user_items (id TEXT PRIMARY KEY, title TEXT NOT NULL);";
        assert!(run_sqlite_migration(&mut conn, user_migration).is_ok());

        // Verify user table exists
        use diesel::connection::SimpleConnection;
        let insert_res = conn.batch_execute(
            "INSERT INTO user_items (id, title) VALUES ('item_1', 'Item 1');",
        );
        assert!(insert_res.is_ok(), "Insert into user migrated table must succeed");
    }
}
