#[cfg(feature = "postgres-async")]
use crate::migrations::setup_postgres;
#[cfg(feature = "test-harness")]
use testcontainers::core::WaitFor;
#[cfg(feature = "test-harness")]
use testcontainers::runners::AsyncRunner;
#[cfg(feature = "test-harness")]
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

/// Reusable Testcontainers PostgreSQL + pgvector testing harness.
#[cfg(all(feature = "postgres-async", feature = "test-harness"))]
pub struct PostgresHarness {
    _container: ContainerAsync<GenericImage>,
    pub conn_str: String,
    pub host: String,
    pub port: u16,
}

#[cfg(all(feature = "postgres-async", feature = "test-harness"))]
impl PostgresHarness {
    /// Launches an ephemeral PostgreSQL container with pgvector enabled and runs baseline setup.
    pub async fn start() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let image = GenericImage::new("pgvector/pgvector", "pg16")
            .with_exposed_port(5432.into())
            .with_wait_for(WaitFor::message_on_stderr(
                "database system is ready to accept connections",
            ))
            .with_env_var("POSTGRES_PASSWORD", "postgres")
            .with_env_var("POSTGRES_USER", "postgres")
            .with_env_var("POSTGRES_DB", "postgres");

        let container = image.start().await?;
        let port = container.get_host_port_ipv4(5432).await?;
        let host = "127.0.0.1".to_string();
        let conn_str =
            format!("host={host} port={port} user=postgres password=postgres dbname=postgres");

        // Establish connection to verify readiness and run initial baseline extension setup
        let mut client_opt = None;
        for _ in 0..15 {
            match tokio_postgres::connect(&conn_str, tokio_postgres::NoTls).await {
                Ok((client, conn)) => {
                    tokio::spawn(async move {
                        if let Err(e) = conn.await {
                            eprintln!("Postgres connection error: {e}");
                        }
                    });
                    client_opt = Some(client);
                    break;
                }
                Err(_) => tokio::time::sleep(tokio::time::Duration::from_millis(300)).await,
            }
        }

        let client = client_opt.ok_or("Failed to connect to ephemeral Postgres container")?;
        setup_postgres(&client).await?;

        Ok(Self {
            _container: container,
            conn_str,
            host,
            port,
        })
    }

    /// Connects a new isolated `tokio_postgres::Client` to the harness.
    pub async fn connect(&self) -> Result<tokio_postgres::Client, tokio_postgres::Error> {
        let (client, conn) = tokio_postgres::connect(&self.conn_str, tokio_postgres::NoTls).await?;
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                eprintln!("Postgres client error: {e}");
            }
        });
        Ok(client)
    }
}

/// Reusable ephemeral Turso / libSQL test harness.
///
/// Supports in-memory (`:memory:`) or isolated temp-file databases, providing
/// both synchronous and native asynchronous connections.
#[cfg(feature = "turso")]
pub struct TursoHarness {
    pub mode: crate::migrations::TursoConnectionMode,
    _temp_dir: Option<tempfile::TempDir>,
}

#[cfg(feature = "turso")]
impl TursoHarness {
    /// Creates an in-memory Turso test harness.
    pub fn in_memory() -> Self {
        Self {
            mode: crate::migrations::TursoConnectionMode::InMemory,
            _temp_dir: None,
        }
    }

    /// Creates an ephemeral file-backed Turso test harness in an isolated temporary directory.
    pub fn ephemeral_file() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let temp_dir = tempfile::tempdir()?;
        let db_path = temp_dir.path().join("turso_test.db");
        Ok(Self {
            mode: crate::migrations::TursoConnectionMode::LocalFile(db_path),
            _temp_dir: Some(temp_dir),
        })
    }

    /// Connects a synchronous `LibSqlConnection` to the harness.
    pub fn connect_sync(
        &self,
    ) -> Result<diesel_libsql::LibSqlConnection, crate::migrations::MigrationError> {
        crate::migrations::establish_turso_sync(&self.mode)
    }

    /// Connects an asynchronous `AsyncLibSqlConnection` to the harness via `diesel-async`.
    #[cfg(feature = "turso-async")]
    pub async fn connect_async(
        &self,
    ) -> Result<diesel_libsql::AsyncLibSqlConnection, crate::migrations::MigrationError> {
        crate::migrations::establish_turso_async(&self.mode).await
    }
}
