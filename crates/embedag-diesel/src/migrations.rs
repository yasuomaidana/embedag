use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("Diesel database error: {0}")]
    Diesel(#[from] diesel::result::Error),

    #[error("Diesel connection error: {0}")]
    Connection(#[from] diesel::ConnectionError),

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

/// Baseline Turso / libSQL configuration.
pub const TURSO_SETUP: &str = "PRAGMA foreign_keys = ON;";

/// Connection mode descriptor for Turso / libSQL distributed database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TursoConnectionMode {
    /// In-memory ephemeral libSQL instance (`:memory:`).
    InMemory,
    /// Local SQLite-compatible file path.
    LocalFile(PathBuf),
    /// Remote Turso database (URL, e.g. `libsql://<db>.turso.io`, and auth token).
    Remote { url: String, auth_token: String },
    /// Embedded replica synchronizing a local file with a remote Turso primary.
    Replica {
        local_path: PathBuf,
        remote_url: String,
        auth_token: String,
    },
}

impl TursoConnectionMode {
    /// Formats the connection mode into a connection URL or specifier string.
    pub fn to_connection_string(&self) -> String {
        match self {
            Self::InMemory => ":memory:".to_string(),
            Self::LocalFile(path) => path.to_string_lossy().to_string(),
            Self::Remote { url, auth_token } => {
                if url.contains("authToken=") {
                    url.clone()
                } else if url.contains('?') {
                    format!("{url}&authToken={auth_token}")
                } else {
                    format!("{url}?authToken={auth_token}")
                }
            }
            Self::Replica { local_path, .. } => local_path.to_string_lossy().to_string(),
        }
    }
}

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
    run_after_setup(conn, setup_sqlite, migration_sql)
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

/// Establishes a synchronous connection to Turso / libSQL using the specified mode.
#[cfg(feature = "turso")]
pub fn establish_turso_sync(
    mode: &TursoConnectionMode,
) -> Result<diesel_libsql::LibSqlConnection, MigrationError> {
    use diesel::Connection;
    let conn = match mode {
        TursoConnectionMode::InMemory => diesel_libsql::LibSqlConnection::establish(":memory:")?,
        TursoConnectionMode::LocalFile(p) => {
            diesel_libsql::LibSqlConnection::establish(&p.to_string_lossy())?
        }
        TursoConnectionMode::Remote { url, auth_token } => {
            let conn_url = if url.contains("authToken=") {
                url.clone()
            } else if url.contains('?') {
                format!("{url}&authToken={auth_token}")
            } else {
                format!("{url}?authToken={auth_token}")
            };
            diesel_libsql::LibSqlConnection::establish(&conn_url)?
        }
        TursoConnectionMode::Replica {
            local_path,
            remote_url,
            auth_token,
        } => diesel_libsql::LibSqlConnection::establish_replica(
            &local_path.to_string_lossy(),
            remote_url,
            auth_token,
        )?,
    };
    Ok(conn)
}

/// Initializes Turso / libSQL database prerequisites synchronously.
#[cfg(feature = "turso")]
pub fn setup_turso_sync(conn: &mut diesel_libsql::LibSqlConnection) -> Result<(), MigrationError> {
    use diesel::connection::SimpleConnection;
    conn.batch_execute(TURSO_SETUP)?;
    Ok(())
}

/// Executes a user-defined SQL migration on Turso / libSQL synchronously.
#[cfg(feature = "turso")]
pub fn run_turso_migration_sync(
    conn: &mut diesel_libsql::LibSqlConnection,
    migration_sql: &str,
) -> Result<(), MigrationError> {
    run_after_setup(conn, setup_turso_sync, migration_sql)
}

/// Runs the baseline `setup` on `conn`, then the user's migration.
#[cfg(any(feature = "sqlite", feature = "turso"))]
fn run_after_setup<C: diesel::connection::SimpleConnection>(
    conn: &mut C,
    setup: fn(&mut C) -> Result<(), MigrationError>,
    migration_sql: &str,
) -> Result<(), MigrationError> {
    setup(conn)?;
    conn.batch_execute(migration_sql)?;
    Ok(())
}

/// Async connection wrapper for Turso / libSQL using `diesel-async`'s `SyncConnectionWrapper`.
#[cfg(feature = "turso-async")]
pub type AsyncLibSqlConnection =
    diesel_async::sync_connection_wrapper::SyncConnectionWrapper<diesel_libsql::LibSqlConnection>;

/// Establishes an asynchronous connection to Turso / libSQL using `diesel-async`.
#[cfg(feature = "turso-async")]
pub async fn establish_turso_async(
    mode: &TursoConnectionMode,
) -> Result<AsyncLibSqlConnection, MigrationError> {
    let sync_conn = establish_turso_sync(mode)?;
    Ok(diesel_async::sync_connection_wrapper::SyncConnectionWrapper::new(sync_conn))
}

/// Initializes Turso / libSQL database prerequisites asynchronously.
#[cfg(feature = "turso-async")]
pub async fn setup_turso_async(conn: &mut AsyncLibSqlConnection) -> Result<(), MigrationError> {
    use diesel_async::SimpleAsyncConnection;
    conn.batch_execute(TURSO_SETUP).await?;
    Ok(())
}

/// Executes a user-defined SQL migration on Turso / libSQL asynchronously.
#[cfg(feature = "turso-async")]
pub async fn run_turso_migration_async(
    conn: &mut AsyncLibSqlConnection,
    migration_sql: &str,
) -> Result<(), MigrationError> {
    use diesel_async::SimpleAsyncConnection;
    setup_turso_async(conn).await?;
    conn.batch_execute(migration_sql).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "sqlite")]
    #[test]
    fn test_sqlite_setup_lifecycle() {
        use diesel::Connection;
        let mut conn = diesel::SqliteConnection::establish(":memory:").expect("in-memory sqlite");

        // Run baseline setup
        assert!(setup_sqlite(&mut conn).is_ok());

        // Run custom user migration
        let user_migration = "CREATE TABLE user_items (id TEXT PRIMARY KEY, title TEXT NOT NULL);";
        assert!(run_sqlite_migration(&mut conn, user_migration).is_ok());

        // Verify user table exists
        use diesel::connection::SimpleConnection;
        let insert_res =
            conn.batch_execute("INSERT INTO user_items (id, title) VALUES ('item_1', 'Item 1');");
        assert!(
            insert_res.is_ok(),
            "Insert into user migrated table must succeed"
        );
    }

    #[test]
    fn test_turso_connection_mode_formatting() {
        let mem = TursoConnectionMode::InMemory;
        assert_eq!(mem.to_connection_string(), ":memory:");

        let local = TursoConnectionMode::LocalFile(PathBuf::from("/tmp/test.db"));
        assert_eq!(local.to_connection_string(), "/tmp/test.db");

        let remote = TursoConnectionMode::Remote {
            url: "libsql://my-db.turso.io".into(),
            auth_token: "secret_tok".into(),
        };
        assert_eq!(
            remote.to_connection_string(),
            "libsql://my-db.turso.io?authToken=secret_tok"
        );
    }
}
