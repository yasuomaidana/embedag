use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[cfg(feature = "postgres-async")]
    #[error("PostgreSQL database error: {0}")]
    Postgres(#[from] tokio_postgres::Error),

    #[error("Diesel database error: {0}")]
    Diesel(#[from] diesel::result::Error),

    #[error("Savepoint rollback executed: {0}")]
    SavepointRolledBack(String),

    #[error("Constraint violation: {0}")]
    ConstraintViolation(String),

    #[error("Cycle detected in persistent graph topology: {0}")]
    CycleDetected(String),

    #[error("Node not found: {0}")]
    NodeNotFound(String),
}

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
            ConflictStrategy::CustomClause(
                "DO UPDATE SET confidence = EXCLUDED.confidence".into(),
            ),
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
                format!("ON CONFLICT ({conflict_joined}) DO UPDATE SET {}", updates.join(", "))
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
                format!("ON CONFLICT ({conflict_joined}) DO UPDATE SET {}", updates.join(", "))
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

/// Node record for batch relational upserts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeUpsertRecord {
    pub id: Uuid,
    pub business_key: String,
    pub entity_level: String,
    pub name: String,
    pub attributes: serde_json::Value,
    pub embedding: Option<Vec<f32>>,
}

/// Directed edge record for batch relational upserts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeUpsertRecord {
    pub parent_id: Uuid,
    pub child_id: Uuid,
    pub relationship_type: String,
    pub confidence: f32,
    pub provenance: String,
}

/// Dynamic mutation directive record to be applied against the relational store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MutationDirectiveRecord {
    InsertNode(NodeUpsertRecord),
    AttachParent {
        parent_id: Uuid,
        child_id: Uuid,
        relationship_type: String,
        confidence: f32,
        provenance: String,
    },
    UpdateNodePayload {
        id: Uuid,
        updated_attributes: serde_json::Value,
    },
    RetargetEdge {
        child_id: Uuid,
        old_parent_id: Uuid,
        new_parent_id: Uuid,
        relationship_type: String,
        confidence: f32,
    },
}

/// Executes an operation wrapped in an isolated SQL `SAVEPOINT`.
/// Automatically rolls back to the savepoint on error without aborting the enclosing outer transaction.
#[cfg(feature = "postgres-async")]
pub async fn execute_with_savepoint<F, Fut, T>(
    tx: &tokio_postgres::Transaction<'_>,
    savepoint_name: &str,
    operation: F,
) -> Result<T, PersistenceError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, PersistenceError>>,
{
    tx.batch_execute(&format!("SAVEPOINT {savepoint_name};"))
        .await?;

    match operation().await {
        Ok(val) => {
            tx.batch_execute(&format!("RELEASE SAVEPOINT {savepoint_name};"))
                .await?;
            Ok(val)
        }
        Err(e) => {
            tx.batch_execute(&format!("ROLLBACK TO SAVEPOINT {savepoint_name};"))
                .await?;
            Err(e)
        }
    }
}

/// Performs a batch upsert of node records generated via [`UpsertDescriptor`].
#[cfg(feature = "postgres-async")]
pub async fn batch_upsert_nodes(
    client: &(impl tokio_postgres::GenericClient + ?Sized),
    nodes: &[NodeUpsertRecord],
) -> Result<u64, PersistenceError> {
    if nodes.is_empty() {
        return Ok(0);
    }

    let descriptor = UpsertDescriptor::semantic_nodes();
    let stmt = descriptor.build_postgres_sql();

    let mut count = 0;
    for node in nodes {
        client
            .execute(
                &stmt,
                &[
                    &node.id,
                    &node.business_key,
                    &node.entity_level,
                    &node.name,
                    &node.attributes,
                    &chrono::Utc::now(),
                ],
            )
            .await?;
        count += 1;
    }

    Ok(count)
}

/// Performs a batch upsert of edge records generated via [`UpsertDescriptor`].
#[cfg(feature = "postgres-async")]
pub async fn batch_upsert_edges(
    client: &(impl tokio_postgres::GenericClient + ?Sized),
    edges: &[EdgeUpsertRecord],
) -> Result<u64, PersistenceError> {
    if edges.is_empty() {
        return Ok(0);
    }

    let descriptor = UpsertDescriptor::semantic_edges();
    let stmt = descriptor.build_postgres_sql();

    let mut count = 0;
    for edge in edges {
        client
            .execute(
                &stmt,
                &[
                    &edge.parent_id,
                    &edge.child_id,
                    &edge.relationship_type,
                    &edge.confidence,
                    &edge.provenance,
                    &chrono::Utc::now(),
                ],
            )
            .await?;
        count += 1;
    }

    Ok(count)
}

/// Executes a dynamic graph mutation lifecycle batch atomically within a transaction.
#[cfg(feature = "postgres-async")]
pub async fn apply_mutation_lifecycle(
    tx: &tokio_postgres::Transaction<'_>,
    mutations: &[MutationDirectiveRecord],
) -> Result<(), PersistenceError> {
    for mutation in mutations {
        match mutation {
            MutationDirectiveRecord::InsertNode(node) => {
                batch_upsert_nodes(tx, std::slice::from_ref(node)).await?;
            }
            MutationDirectiveRecord::AttachParent {
                parent_id,
                child_id,
                relationship_type,
                confidence,
                provenance,
            } => {
                let edge = EdgeUpsertRecord {
                    parent_id: *parent_id,
                    child_id: *child_id,
                    relationship_type: relationship_type.clone(),
                    confidence: *confidence,
                    provenance: provenance.clone(),
                };
                batch_upsert_edges(tx, &[edge]).await?;
            }
            MutationDirectiveRecord::UpdateNodePayload {
                id,
                updated_attributes,
            } => {
                let stmt = "
                    UPDATE semantic_nodes
                    SET attributes = attributes || $1, updated_at = NOW()
                    WHERE id = $2;
                ";
                tx.execute(stmt, &[updated_attributes, id]).await?;
            }
            MutationDirectiveRecord::RetargetEdge {
                child_id,
                old_parent_id,
                new_parent_id,
                relationship_type,
                confidence,
            } => {
                let del_stmt = "
                    DELETE FROM semantic_edges
                    WHERE parent_id = $1 AND child_id = $2 AND relationship_type = $3;
                ";
                tx.execute(del_stmt, &[old_parent_id, child_id, relationship_type])
                    .await?;

                let edge = EdgeUpsertRecord {
                    parent_id: *new_parent_id,
                    child_id: *child_id,
                    relationship_type: relationship_type.clone(),
                    confidence: *confidence,
                    provenance: "retargeted".into(),
                };
                batch_upsert_edges(tx, &[edge]).await?;
            }
        }
    }

    Ok(())
}
