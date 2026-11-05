//! Turso / libSQL asynchronous batch operations and savepoint execution.

use super::error::PersistenceError;
use super::records::*;
use crate::quoting::quoted_identifier;

/// Executes an operation wrapped in an isolated SQL `SAVEPOINT` on Turso / libSQL via `diesel-async`.
/// Automatically rolls back to the savepoint on error without aborting the enclosing outer transaction.
/// The operation's own error is always the one returned: a failure of the rollback itself (a dropped
/// connection, say) is ignored, because the connection is already unusable and that error would only
/// hide the cause.
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
    let savepoint = begin_savepoint(conn, savepoint_name).await?;
    let result = operation(conn).await;
    end_savepoint(conn, &savepoint, result).await
}

/// Opens the savepoint `name` and returns its quoted form.
#[cfg(feature = "turso-async")]
async fn begin_savepoint(
    conn: &mut crate::migrations::AsyncLibSqlConnection,
    name: &str,
) -> Result<String, PersistenceError> {
    use diesel_async::SimpleAsyncConnection;
    let savepoint = quoted_identifier(name);
    conn.batch_execute(&format!("SAVEPOINT {savepoint};"))
        .await
        .map_err(PersistenceError::Diesel)?;
    Ok(savepoint)
}

/// Releases the savepoint when `result` is `Ok`; otherwise rolls back to it and releases it, and
/// returns the error of `result` whatever happens to the rollback.
#[cfg(feature = "turso-async")]
async fn end_savepoint<T>(
    conn: &mut crate::migrations::AsyncLibSqlConnection,
    savepoint: &str,
    result: Result<T, PersistenceError>,
) -> Result<T, PersistenceError> {
    use diesel_async::SimpleAsyncConnection;
    match result {
        Ok(val) => {
            conn.batch_execute(&format!("RELEASE SAVEPOINT {savepoint};"))
                .await
                .map_err(PersistenceError::Diesel)?;
            Ok(val)
        }
        Err(e) => {
            // `ROLLBACK TO` keeps the savepoint on the stack, so release it as well.
            let _ = conn
                .batch_execute(&format!(
                    "ROLLBACK TO SAVEPOINT {savepoint}; RELEASE SAVEPOINT {savepoint};"
                ))
                .await;
            Err(e)
        }
    }
}

/// The SQL of a JSON object holding the members of `existing` overwritten by the members of
/// `incoming` (both JSON text expressions): the SQLite counterpart of PostgreSQL's `jsonb || jsonb`,
/// a top-level merge in which a member of `incoming` replaces the whole member of `existing`.
fn merged_attributes(existing: &str, incoming: &str) -> String {
    let member = |source: &str, filter: &str| {
        format!(
            "SELECT key AS k, CASE type WHEN 'text' THEN json_quote(value) \
             WHEN 'true' THEN 'true' WHEN 'false' THEN 'false' WHEN 'null' THEN 'null' \
             ELSE value END AS v FROM json_each({source}){filter}"
        )
    };
    let kept = member(
        existing,
        &format!(" WHERE key NOT IN (SELECT key FROM json_each({incoming}))"),
    );
    let replaced = member(incoming, "");
    format!("(SELECT json_group_object(k, json(v)) FROM ({kept} UNION ALL {replaced}))")
}

/// Performs a batch upsert of node records to Turso / libSQL via `diesel-async`.
///
/// A node whose business key exists keeps its entity level and gets the new name; its attributes
/// are merged with the new ones at the top level, as on PostgreSQL. The `embedding` of the record
/// is not written (see the crate README).
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
             name = excluded.name, attributes = {merged}, updated_at = excluded.updated_at;",
            merged = merged_attributes("semantic_nodes.attributes", "excluded.attributes"),
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
///
/// The batch runs inside one `SAVEPOINT`: if any directive fails, none of the batch is kept (an
/// edge retarget never loses the old edge without adding the new one), and the error of the failing
/// directive is returned. Inside an open transaction the batch is rolled back on its own, leaving
/// the transaction usable.
#[cfg(feature = "turso-async")]
pub async fn apply_mutation_lifecycle_turso(
    conn: &mut crate::migrations::AsyncLibSqlConnection,
    mutations: &[MutationDirectiveRecord],
) -> Result<(), PersistenceError> {
    let savepoint = begin_savepoint(conn, "apply_mutation_lifecycle").await?;
    let result = apply_directives_turso(conn, mutations).await;
    end_savepoint(conn, &savepoint, result).await
}

/// Runs the directives one after the other; the caller provides the atomicity.
#[cfg(feature = "turso-async")]
async fn apply_directives_turso(
    conn: &mut crate::migrations::AsyncLibSqlConnection,
    mutations: &[MutationDirectiveRecord],
) -> Result<(), PersistenceError> {
    use diesel_async::SimpleAsyncConnection;
    for mutation in mutations {
        match mutation {
            MutationDirectiveRecord::InsertNode(node) => {
                batch_upsert_nodes_turso(conn, std::slice::from_ref(node)).await?;
            }
            MutationDirectiveRecord::AttachParent { .. } => {
                if let Some(edge) = mutation.attached_edge() {
                    batch_upsert_edges_turso(conn, &[edge]).await?;
                }
            }
            MutationDirectiveRecord::UpdateNodePayload {
                id,
                updated_attributes,
            } => {
                let id_str = id.to_string();
                let attr_str = updated_attributes.to_string().replace('\'', "''");
                let now_str = chrono::Utc::now().to_rfc3339();
                let merged =
                    merged_attributes("semantic_nodes.attributes", &format!("'{attr_str}'"));
                let sql = format!(
                    "UPDATE semantic_nodes SET attributes = {merged}, updated_at = '{now_str}' WHERE id = '{id_str}';"
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
