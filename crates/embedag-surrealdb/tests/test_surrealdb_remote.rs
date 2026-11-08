//! The persistence suite against a SurrealDB server reached over WebSocket with credentials.

use std::ops::Deref;
use std::time::Duration;

use embedag_surrealdb::{SurrealDbConfig, SurrealDbEngine};
use testcontainers::core::WaitFor;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

/// An engine connected to a container it owns, so the server lives as long as the engine.
pub struct RemoteEngine {
    engine: SurrealDbEngine,
    _container: ContainerAsync<GenericImage>,
}

impl Deref for RemoteEngine {
    type Target = SurrealDbEngine;

    fn deref(&self) -> &SurrealDbEngine {
        &self.engine
    }
}

/// Starts SurrealDB and connects as its root user, retrying while the server finishes booting.
pub async fn remote_engine() -> Result<RemoteEngine, Box<dyn std::error::Error + Send + Sync>> {
    let image = GenericImage::new("surrealdb/surrealdb", "latest")
        .with_exposed_port(8000.into())
        .with_wait_for(WaitFor::message_on_stdout("Started web server"))
        .with_cmd([
            "start",
            "--user",
            "root",
            "--pass",
            "root",
            "--bind",
            "0.0.0.0:8000",
            "memory",
        ]);
    let container = image.start().await?;
    let port = container.get_host_port_ipv4(8000).await?;
    let config =
        SurrealDbConfig::remote(format!("ws://127.0.0.1:{port}")).with_credentials("root", "root");
    let mut last_error = None;
    for _ in 0..30 {
        match SurrealDbEngine::open(config.clone()).await {
            Ok(engine) => {
                return Ok(RemoteEngine {
                    engine,
                    _container: container,
                });
            }
            Err(e) => last_error = Some(e),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err(last_error.map_or_else(|| "no connection attempt".into(), |e| e.to_string().into()))
}

embedag_core::persistence_suite!(
    remote_engine(),
    #[ignore = "starts a SurrealDB container: runs in the persistence-testcontainers and coverage jobs"]
);
