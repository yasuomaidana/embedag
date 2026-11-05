//! PostgreSQL asynchronous batch operations and savepoint execution.

use super::descriptor::*;
use super::error::PersistenceError;
use super::records::*;
use crate::quoting::quoted_identifier;

/// Executes an operation wrapped in an isolated SQL `SAVEPOINT`.
/// Automatically rolls back to the savepoint on error without aborting the enclosing outer transaction.
/// The operation's own error is always the one returned: a failure of the rollback itself (a dropped
/// connection, say) is ignored, because the connection is already unusable and that error would only
/// hide the cause.
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
    let savepoint = quoted_identifier(savepoint_name);
    tx.batch_execute(&format!("SAVEPOINT {savepoint};")).await?;

    match operation().await {
        Ok(val) => {
            tx.batch_execute(&format!("RELEASE SAVEPOINT {savepoint};"))
                .await?;
            Ok(val)
        }
        Err(e) => {
            // `ROLLBACK TO` keeps the savepoint on the stack, so release it as well.
            let _ = tx
                .batch_execute(&format!(
                    "ROLLBACK TO SAVEPOINT {savepoint}; RELEASE SAVEPOINT {savepoint};"
                ))
                .await;
            Err(e)
        }
    }
}

/// The query parameters of one record: the record's own columns, then the write time.
#[cfg(feature = "postgres-async")]
type Params<'a> = Vec<&'a (dyn tokio_postgres::types::ToSql + Sync)>;

/// A record `batch_upsert` writes: the upsert statement of its table and the parameters of one row.
#[cfg(feature = "postgres-async")]
trait PgUpsert {
    fn statement() -> String;
    fn params<'a>(&'a self, now: &'a chrono::DateTime<chrono::Utc>) -> Params<'a>;
}

/// Implements [`PgUpsert`] for a record: the descriptor of its table and the fields that fill the
/// statement's placeholders, in order.
#[cfg(feature = "postgres-async")]
macro_rules! pg_upsert {
    ($record:ty, $descriptor:ident, [$($field:ident),+ $(,)?]) => {
        impl PgUpsert for $record {
            fn statement() -> String {
                UpsertDescriptor::$descriptor().build_postgres_sql()
            }

            fn params<'a>(&'a self, now: &'a chrono::DateTime<chrono::Utc>) -> Params<'a> {
                vec![$(&self.$field,)+ now]
            }
        }
    };
}

#[cfg(feature = "postgres-async")]
pg_upsert!(
    NodeUpsertRecord,
    semantic_nodes,
    [id, business_key, entity_level, name, attributes]
);

#[cfg(feature = "postgres-async")]
pg_upsert!(
    EdgeUpsertRecord,
    semantic_edges,
    [
        parent_id,
        child_id,
        relationship_type,
        confidence,
        provenance
    ]
);

/// Executes the upsert of `T` once per item and counts the executions. The statement is prepared once,
/// so each row costs one round trip that only binds its parameters.
#[cfg(feature = "postgres-async")]
async fn batch_upsert<T: PgUpsert>(
    client: &(impl tokio_postgres::GenericClient + ?Sized),
    items: &[T],
) -> Result<u64, PersistenceError> {
    if items.is_empty() {
        return Ok(0);
    }
    let stmt = client.prepare(&T::statement()).await?;
    let mut count = 0;
    for item in items {
        let now = chrono::Utc::now();
        client.execute(&stmt, &item.params(&now)).await?;
        count += 1;
    }
    Ok(count)
}

/// Performs a batch upsert of node records generated via [`UpsertDescriptor`].
#[cfg(feature = "postgres-async")]
pub async fn batch_upsert_nodes(
    client: &(impl tokio_postgres::GenericClient + ?Sized),
    nodes: &[NodeUpsertRecord],
) -> Result<u64, PersistenceError> {
    batch_upsert(client, nodes).await
}

/// Performs a batch upsert of edge records generated via [`UpsertDescriptor`].
#[cfg(feature = "postgres-async")]
pub async fn batch_upsert_edges(
    client: &(impl tokio_postgres::GenericClient + ?Sized),
    edges: &[EdgeUpsertRecord],
) -> Result<u64, PersistenceError> {
    batch_upsert(client, edges).await
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
            MutationDirectiveRecord::AttachParent { .. } => {
                if let Some(edge) = mutation.attached_edge() {
                    batch_upsert_edges(tx, &[edge]).await?;
                }
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
