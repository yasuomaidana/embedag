//! Relational upsert descriptors, mutation lifecycle execution, and savepoint operations.
//!
//! Subdivided into cohesive modules:
//! - [`error`]: The `PersistenceError` error types.
//! - [`descriptor`]: Generic upsert descriptors and processor.
//! - [`records`]: Mutation directive records and upsert records.
//! - [`postgres`]: PostgreSQL-specific async upsert and mutation lifecycle operations.
//! - [`turso`]: Turso / libSQL-specific async upsert and mutation lifecycle operations.

pub mod descriptor;
pub mod error;
pub mod records;

#[cfg(feature = "postgres-async")]
pub mod postgres;

#[cfg(feature = "turso-async")]
pub mod turso;

pub use descriptor::*;
pub use error::*;
pub use records::*;

#[cfg(feature = "postgres-async")]
pub use postgres::*;

#[cfg(feature = "turso-async")]
pub use turso::*;
