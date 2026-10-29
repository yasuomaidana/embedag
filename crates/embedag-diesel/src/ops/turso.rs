//! Turso / libSQL asynchronous batch operations and savepoint execution.

use super::error::PersistenceError;
use super::records::*;

/// Executes an operation wrapped in an isolated SQL `SAVEPOINT` on Turso / libSQL via `diesel-async`.
/// Automatically rolls back to the savepoint on error without aborting the enclosing outer transaction.
#[cfg(feature = "turso-async")]
pub async fn execute_with_savepoint_turso<F, T>(
    conn: &mut crate::migrations::AsyncLibSqlConnection,
    savepoint_name: &str,
    operation: F,
) -> Result<T, PersistenceError>
where
    F: for<'a> FnOnce(
            &'a mut crate::migrations::AsyncLibSqlConnection,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<T, PersistenceError>> + Send + 'a>,
        > + Send,
    T: Send,
{
    use diesel_async::SimpleAsyncConnection;
    conn.batch_execute(&format!("SAVEPOINT {savepoint_name};"))
        .await
        .map_err(PersistenceError::Diesel)?;

    match operation(conn).await {
        Ok(val) => {
            conn.batch_execute(&format!("RELEASE SAVEPOINT {savepoint_name};"))
                .await
                .map_err(PersistenceError::Diesel)?;
            Ok(val)
        }
        Err(e) => {
            conn.batch_execute(&format!("ROLLBACK TO SAVEPOINT {savepoint_name};"))
                .await
                .map_err(PersistenceError::Diesel)?;
            Err(e)
        }
    }
}

/// Performs a batch upsert of node records to Turso / libSQL via `diesel-async`.
#[cfg(feature = "turso-async")]
pub async fn batch_upsert_nodes_turso(
    conn: &mut crate::migrations::AsyncLibSqlConnection,
    nodes: &[NodeUpsertRecord],
) -> Result<u64, PersistenceError> {
    use diesel_async::SimpleAsyncConnection;
    if nodes.is_empty() {
        return Ok(0);
    }

    let mut count = 0;
    for node in nodes {
        let id_str = node.id.to_string();
        let attr_str = node.attributes.to_string().replace('\'', "''");
        let name_str = node.name.replace('\'', "''");
        let bkey_str = node.business_key.replace('\'', "''");
        let level_str = node.entity_level.replace('\'', "''");
        let now_str = chrono::Utc::now().to_rfc3339();

        let sql = format!(
            "INSERT INTO semantic_nodes (id, business_key, entity_level, name, attributes, updated_at) \
             VALUES ('{id_str}', '{bkey_str}', '{level_str}', '{name_str}', '{attr_str}', '{now_str}') \
             ON CONFLICT (business_key) DO UPDATE SET \
             name = excluded.name, entity_level = excluded.entity_level, attributes = excluded.attributes, updated_at = excluded.updated_at;"
        );
        conn.batch_execute(&sql)
            .await
            .map_err(PersistenceError::Diesel)?;
        count += 1;
    }

    Ok(count)
}

/// Performs a batch upsert of edge records to Turso / libSQL via `diesel-async`.
#[cfg(feature = "turso-async")]
pub async fn batch_upsert_edges_turso(
    conn: &mut crate::migrations::AsyncLibSqlConnection,
    edges: &[EdgeUpsertRecord],
) -> Result<u64, PersistenceError> {
    use diesel_async::SimpleAsyncConnection;
    if edges.is_empty() {
        return Ok(0);
    }

    let mut count = 0;
    for edge in edges {
        let parent_str = edge.parent_id.to_string();
        let child_str = edge.child_id.to_string();
        let rel_str = edge.relationship_type.replace('\'', "''");
        let conf = edge.confidence;
        let prov_str = edge.provenance.replace('\'', "''");
        let now_str = chrono::Utc::now().to_rfc3339();

        let sql = format!(
            "INSERT INTO semantic_edges (parent_id, child_id, relationship_type, confidence, provenance, created_at) \
             VALUES ('{parent_str}', '{child_str}', '{rel_str}', {conf}, '{prov_str}', '{now_str}') \
             ON CONFLICT (parent_id, child_id, relationship_type) DO UPDATE SET \
             confidence = excluded.confidence, provenance = excluded.provenance;"
        );
        conn.batch_execute(&sql)
            .await
            .map_err(PersistenceError::Diesel)?;
        count += 1;
    }

    Ok(count)
}

/// Executes a dynamic graph mutation lifecycle batch atomically on Turso / libSQL via `diesel-async`.
#[cfg(feature = "turso-async")]
pub async fn apply_mutation_lifecycle_turso(
    conn: &mut crate::migrations::AsyncLibSqlConnection,
    mutations: &[MutationDirectiveRecord],
) -> Result<(), PersistenceError> {
    use diesel_async::SimpleAsyncConnection;
    for mutation in mutations {
        match mutation {
            MutationDirectiveRecord::InsertNode(node) => {
                batch_upsert_nodes_turso(conn, std::slice::from_ref(node)).await?;
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
                batch_upsert_edges_turso(conn, &[edge]).await?;
            }
            MutationDirectiveRecord::UpdateNodePayload {
                id,
                updated_attributes,
            } => {
                let id_str = id.to_string();
                let attr_str = updated_attributes.to_string().replace('\'', "''");
                let now_str = chrono::Utc::now().to_rfc3339();
                let sql = format!(
                    "UPDATE semantic_nodes SET attributes = '{attr_str}', updated_at = '{now_str}' WHERE id = '{id_str}';"
                );
                conn.batch_execute(&sql)
                    .await
                    .map_err(PersistenceError::Diesel)?;
            }
            MutationDirectiveRecord::RetargetEdge {
                child_id,
                old_parent_id,
                new_parent_id,
                relationship_type,
                confidence,
            } => {
                let old_p = old_parent_id.to_string();
                let ch = child_id.to_string();
                let rel = relationship_type.replace('\'', "''");
                let del_sql = format!(
                    "DELETE FROM semantic_edges WHERE parent_id = '{old_p}' AND child_id = '{ch}' AND relationship_type = '{rel}';"
                );
                conn.batch_execute(&del_sql)
                    .await
                    .map_err(PersistenceError::Diesel)?;

                let edge = EdgeUpsertRecord {
                    parent_id: *new_parent_id,
                    child_id: *child_id,
                    relationship_type: relationship_type.clone(),
                    confidence: *confidence,
                    provenance: "mutation_lifecycle_retarget".into(),
                };
                batch_upsert_edges_turso(conn, &[edge]).await?;
            }
        }
    }

    Ok(())
}
