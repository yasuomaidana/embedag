//! Relational persistence adapter for PostgreSQL with pgvector and SQLite.

pub mod cte;
pub mod harness;
pub mod migrations;
pub mod ops;
pub mod traversal;

pub use cte::{
    with_cte, with_recursive, with_recursive_not_all, CteHierarchyBuilder, HierarchicalModel,
    HierarchicalNode, HierarchicalTableConfig, HierarchyItem, RecursiveCTEExt, RecursiveParts,
};
pub use harness::PostgresHarness;
pub use migrations::{
    run_postgres_migration, run_sqlite_migration, setup_postgres, setup_sqlite, MigrationError,
    POSTGRES_SETUP, SQLITE_SETUP,
};
pub use ops::{
    ConflictStrategy, EdgeUpsertRecord, MutationDirectiveRecord, NodeUpsertRecord,
    PersistenceError, UpsertDescriptor, UpsertProcessor,
};
#[cfg(feature = "postgres-async")]
pub use ops::{
    apply_mutation_lifecycle, batch_upsert_edges, batch_upsert_nodes, execute_with_savepoint,
};
pub use traversal::{AncestorRow, DescendantRow, RecursiveTraversalBuilder};
