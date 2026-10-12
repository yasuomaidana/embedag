use crate::migrations::setup_postgres;
use testcontainers::core::WaitFor;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

/// Reusable Testcontainers PostgreSQL + pgvector testing harness.
pub struct PostgresHarness {
    _container: ContainerAsync<GenericImage>,
    pub conn_str: String,
    pub host: String,
    pub port: u16,
}

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
