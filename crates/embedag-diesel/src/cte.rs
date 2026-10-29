//! Generic Hierarchical Recursive CTE Traversal Engine using `diesel-cte-ext`.
//!
//! Provides compile-time safe, dialect-aware recursive Common Table Expressions
//! for both SQLite and PostgreSQL. Supports custom user-defined tables,
//! self-referencing adjacency lists (e.g. `categories(id, parent_id, name)`),
//! and dual-table node/edge schemas. Completely agnostic of any domain model.

pub mod builder;
pub mod config;
pub mod model;

pub use builder::*;
pub use config::*;
pub use model::*;

pub use diesel_cte_ext::{
    CteParts, RecursiveCTEExt, RecursiveParts, with_cte, with_recursive, with_recursive_not_all,
};
pub use embedag_derive::HierarchicalModel;
