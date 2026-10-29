//! Generic upsert descriptors, conflict strategies, and mutation directive records.

use serde::{Deserialize, Serialize};

/// Strategy for handling conflicts during relational upsert operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictStrategy {
    /// Ignore conflicts (`ON CONFLICT (...) DO NOTHING`).
    DoNothing,
    /// Update specified columns with incoming `EXCLUDED` values.
    DoUpdate {
        /// Columns to be updated from incoming excluded values.
        columns: Vec<String>,
    },
    /// Custom SQL conflict resolution clause supplied by the user.
    CustomClause(String),
}

/// Generic structure-agnostic descriptor defining an upsert operation for ANY relational table.
///
/// Eliminates the need for users to hand-write raw SQL string templates with `$1, $2, ...`
/// or hardcode specific table and column names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpsertDescriptor {
    /// Target relational table name.
    pub table_name: String,
    /// Column names to insert.
    pub columns: Vec<String>,
    /// Unique/primary key columns that trigger conflict resolution.
    pub conflict_columns: Vec<String>,
    /// Strategy applied on conflict.
    pub strategy: ConflictStrategy,
}

impl UpsertDescriptor {
    /// Creates a new generic upsert descriptor.
    pub fn new(
        table_name: impl Into<String>,
        columns: Vec<impl Into<String>>,
        conflict_columns: Vec<impl Into<String>>,
        strategy: ConflictStrategy,
    ) -> Self {
        Self {
            table_name: table_name.into(),
            columns: columns.into_iter().map(Into::into).collect(),
            conflict_columns: conflict_columns.into_iter().map(Into::into).collect(),
            strategy,
        }
    }

    /// Convenience builder with a do-nothing conflict strategy.
    pub fn on_conflict_do_nothing(
        table_name: impl Into<String>,
        columns: Vec<impl Into<String>>,
        conflict_columns: Vec<impl Into<String>>,
    ) -> Self {
        Self::new(
            table_name,
            columns,
            conflict_columns,
            ConflictStrategy::DoNothing,
        )
    }

    /// Convenience builder with a do-update conflict strategy updating specific columns.
    pub fn on_conflict_do_update(
        table_name: impl Into<String>,
        columns: Vec<impl Into<String>>,
        conflict_columns: Vec<impl Into<String>>,
        update_columns: Vec<impl Into<String>>,
    ) -> Self {
        Self::new(
            table_name,
            columns,
            conflict_columns,
            ConflictStrategy::DoUpdate {
                columns: update_columns.into_iter().map(Into::into).collect(),
            },
        )
    }

    /// Default descriptor for turnkey semantic nodes upsert.
    pub fn semantic_nodes() -> Self {
        Self::new(
            "semantic_nodes",
            vec![
                "id",
                "business_key",
                "entity_level",
                "name",
                "attributes",
                "updated_at",
            ],
            vec!["business_key"],
            ConflictStrategy::CustomClause(
                "DO UPDATE SET attributes = semantic_nodes.attributes || EXCLUDED.attributes, name = EXCLUDED.name, updated_at = NOW()".into(),
            ),
        )
    }

    /// Default descriptor for turnkey semantic edges upsert.
    pub fn semantic_edges() -> Self {
        Self::new(
            "semantic_edges",
            vec![
                "parent_id",
                "child_id",
                "relationship_type",
                "confidence",
                "provenance",
                "created_at",
            ],
            vec!["parent_id", "child_id", "relationship_type"],
            ConflictStrategy::CustomClause("DO UPDATE SET confidence = EXCLUDED.confidence".into()),
        )
    }

    /// Generates PostgreSQL-compatible parameterized SQL statement for this descriptor.
    pub fn build_postgres_sql(&self) -> String {
        let cols_joined = self.columns.join(", ");
        let placeholders: Vec<String> = (1..=self.columns.len()).map(|i| format!("${i}")).collect();
        let placeholders_joined = placeholders.join(", ");
        let conflict_joined = self.conflict_columns.join(", ");

        let conflict_part = match &self.strategy {
            ConflictStrategy::DoNothing => format!("ON CONFLICT ({conflict_joined}) DO NOTHING"),
            ConflictStrategy::DoUpdate { columns } => {
                let updates: Vec<String> = columns
                    .iter()
                    .map(|c| format!("{c} = EXCLUDED.{c}"))
                    .collect();
                format!(
                    "ON CONFLICT ({conflict_joined}) DO UPDATE SET {}",
                    updates.join(", ")
                )
            }
            ConflictStrategy::CustomClause(clause) => {
                format!("ON CONFLICT ({conflict_joined}) {clause}")
            }
        };

        format!(
            "INSERT INTO {} ({cols_joined}) VALUES ({placeholders_joined}) {conflict_part};",
            self.table_name
        )
    }

    /// Generates SQLite-compatible parameterized SQL statement for this descriptor.
    pub fn build_sqlite_sql(&self) -> String {
        let cols_joined = self.columns.join(", ");
        let placeholders: Vec<&str> = (0..self.columns.len()).map(|_| "?").collect();
        let placeholders_joined = placeholders.join(", ");
        let conflict_joined = self.conflict_columns.join(", ");

        let conflict_part = match &self.strategy {
            ConflictStrategy::DoNothing => format!("ON CONFLICT ({conflict_joined}) DO NOTHING"),
            ConflictStrategy::DoUpdate { columns } => {
                let updates: Vec<String> = columns
                    .iter()
                    .map(|c| format!("{c} = excluded.{c}"))
                    .collect();
                format!(
                    "ON CONFLICT ({conflict_joined}) DO UPDATE SET {}",
                    updates.join(", ")
                )
            }
            ConflictStrategy::CustomClause(clause) => {
                format!("ON CONFLICT ({conflict_joined}) {clause}")
            }
        };

        format!(
            "INSERT INTO {} ({cols_joined}) VALUES ({placeholders_joined}) {conflict_part};",
            self.table_name
        )
    }

    /// Generates Turso / libSQL-compatible parameterized SQL statement for this descriptor.
    pub fn build_turso_sql(&self) -> String {
        self.build_sqlite_sql()
    }
}

/// Structure-agnostic upsert processor supporting custom resolution functions and generic execution.
pub struct UpsertProcessor;

impl UpsertProcessor {
    /// Executes a custom upsert routine using a user-provided closure or function.
    pub fn execute_with_custom<Conn, Record, F, R>(
        conn: &mut Conn,
        record: &Record,
        custom_fn: F,
    ) -> R
    where
        F: FnOnce(&mut Conn, &Record) -> R,
    {
        custom_fn(conn, record)
    }
}
