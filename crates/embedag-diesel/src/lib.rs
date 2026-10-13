//! Relational persistence adapter for PostgreSQL with pgvector, SQLite, and Turso / libSQL.

pub mod cte;
pub mod harness;
pub mod migrations;
pub mod ops;
pub mod traversal;

pub use cte::{
    CteHierarchyBuilder, HierarchicalModel, HierarchicalNode, HierarchicalTableConfig,
    HierarchyItem, RecursiveCTEExt, RecursiveParts, with_cte, with_recursive,
    with_recursive_not_all,
};
#[cfg(all(feature = "postgres-async", feature = "test-harness"))]
pub use harness::PostgresHarness;
#[cfg(feature = "turso")]
pub use harness::TursoHarness;

pub use migrations::{
    MigrationError, POSTGRES_SETUP, SQLITE_SETUP, TURSO_SETUP, TursoConnectionMode,
};
#[cfg(feature = "turso-async")]
pub use migrations::{establish_turso_async, run_turso_migration_async, setup_turso_async, AsyncLibSqlConnection};
#[cfg(feature = "turso")]
pub use migrations::{establish_turso_sync, run_turso_migration_sync, setup_turso_sync};
#[cfg(feature = "postgres-async")]
pub use migrations::{run_postgres_migration, setup_postgres};
#[cfg(feature = "sqlite")]
pub use migrations::{run_sqlite_migration, setup_sqlite};

pub use ops::{
    ConflictStrategy, EdgeUpsertRecord, MutationDirectiveRecord, NodeUpsertRecord,
    PersistenceError, UpsertDescriptor, UpsertProcessor,
};
#[cfg(feature = "postgres-async")]
pub use ops::{
    apply_mutation_lifecycle, batch_upsert_edges, batch_upsert_nodes, execute_with_savepoint,
};
#[cfg(feature = "turso-async")]
pub use ops::{
    apply_mutation_lifecycle_turso, batch_upsert_edges_turso, batch_upsert_nodes_turso,
    execute_with_savepoint_turso,
};

pub use traversal::{AncestorRow, DescendantRow, RecursiveTraversalBuilder};
