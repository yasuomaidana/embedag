//! PostgreSQL asynchronous batch operations and savepoint execution.

use super::descriptor::*;
use super::error::PersistenceError;
use super::records::*;

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
