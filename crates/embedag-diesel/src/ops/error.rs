use thiserror::Error;

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[cfg(feature = "postgres-async")]
    #[error("PostgreSQL database error: {0}")]
    Postgres(#[from] tokio_postgres::Error),

    #[error("Diesel database error: {0}")]
    Diesel(#[from] diesel::result::Error),

    #[error("Diesel connection error: {0}")]
    Connection(#[from] diesel::ConnectionError),

    #[error("Savepoint rollback executed: {0}")]
    SavepointRolledBack(String),

    #[error("Constraint violation: {0}")]
    ConstraintViolation(String),

    #[error("Cycle detected in persistent graph topology: {0}")]
    CycleDetected(String),

    #[error("Node not found: {0}")]
    NodeNotFound(String),
}
