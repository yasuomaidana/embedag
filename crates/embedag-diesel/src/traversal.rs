//! Type-safe recursive CTE traversal builder with cycle guardrails for PostgreSQL and SQLite.
//!
//! Aligned with [`leynos/diesel-cte-ext`](https://github.com/leynos/diesel-cte-ext) architecture,
//! providing `RecursiveCTEExt` and `RecursiveParts` traversal contracts with SQL-level visited array path
//! cycle detection and deduplication.

use crate::ops::PersistenceError;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Ancestor lineage entry retrieved in a single roundtrip via recursive CTE.
#[derive(Debug, Clone, PartialEq)]
pub struct AncestorRow {
    pub node_id: Uuid,
    pub depth: i32,
    pub business_key: String,
    pub name: String,
    pub entity_level: String,
}

/// Descendant entry retrieved in a single roundtrip via recursive CTE.
#[derive(Debug, Clone, PartialEq)]
pub struct DescendantRow {
    pub node_id: Uuid,
    pub depth: i32,
    pub business_key: String,
    pub name: String,
    pub entity_level: String,
}

/// Type-safe recursive CTE traversal builder with cycle guardrails for PostgreSQL and SQLite.
///
/// Implements recursive Common Table Expression queries modeled after `leynos/diesel-cte-ext`
/// (`RecursiveCTEExt`, `RecursiveParts::new(seed, step, body)`).
pub struct RecursiveTraversalBuilder;

impl RecursiveTraversalBuilder {
    /// Constructs PostgreSQL recursive CTE SQL query for multi-hop ancestors with array path cycle guard.
    pub fn postgres_ancestor_query() -> &'static str {
        "
        WITH RECURSIVE ancestor_tree(node_id, depth, visited, cycle) AS (\n            -- Base Anchor Member\n            SELECT\n                parent_id,\n                1 AS depth,\n                ARRAY[child_id, parent_id] AS visited,\n                FALSE AS cycle\n            FROM semantic_edges\n            WHERE child_id = $1\n\n            UNION ALL\n\n            -- Recursive Member with Cycle Guard\n            SELECT\n                e.parent_id,\n                t.depth + 1,\n                t.visited || e.parent_id,\n                e.parent_id = ANY(t.visited) AS cycle\n            FROM semantic_edges e\n            JOIN ancestor_tree t ON e.child_id = t.node_id\n            WHERE t.depth < $2\n              AND NOT t.cycle\n              AND NOT (e.parent_id = ANY(t.visited))\n        )\n        SELECT\n            t.node_id,\n            t.depth,\n            n.business_key,\n            n.name,\n            n.entity_level\n        FROM ancestor_tree t\n        JOIN semantic_nodes n ON n.id = t.node_id\n        ORDER BY t.depth ASC;\n        "
    }

    /// Constructs PostgreSQL recursive CTE SQL query for historical point-in-time multi-hop ancestors.
    pub fn postgres_ancestor_as_of_query() -> &'static str {
        "
        WITH RECURSIVE ancestor_tree(node_id, depth, visited, cycle) AS (\n            -- Base Anchor Member\n            SELECT\n                parent_id,\n                1 AS depth,\n                ARRAY[child_id, parent_id] AS visited,\n                FALSE AS cycle\n            FROM semantic_edges\n            WHERE child_id = $1\n              AND valid_from <= $3 AND (valid_to IS NULL OR valid_to > $3)\n\n            UNION ALL\n\n            -- Recursive Member with Cycle Guard & Point-in-time check\n            SELECT\n                e.parent_id,\n                t.depth + 1,\n                t.visited || e.parent_id,\n                e.parent_id = ANY(t.visited) AS cycle\n            FROM semantic_edges e\n            JOIN ancestor_tree t ON e.child_id = t.node_id\n            WHERE t.depth < $2\n              AND NOT t.cycle\n              AND NOT (e.parent_id = ANY(t.visited))\n              AND e.valid_from <= $3 AND (e.valid_to IS NULL OR e.valid_to > $3)\n        )\n        SELECT\n            t.node_id,\n            t.depth,\n            n.business_key,\n            n.name,\n            n.entity_level\n        FROM ancestor_tree t\n        JOIN semantic_nodes n ON n.id = t.node_id\n        ORDER BY t.depth ASC;\n        "
    }

    /// Constructs PostgreSQL recursive CTE SQL query for multi-hop descendants with cycle guard.
    pub fn postgres_descendant_query() -> &'static str {
        "
        WITH RECURSIVE descendant_tree(node_id, depth, visited, cycle) AS (\n            SELECT\n                child_id,\n                1 AS depth,\n                ARRAY[parent_id, child_id] AS visited,\n                FALSE AS cycle\n            FROM semantic_edges\n            WHERE parent_id = $1\n\n            UNION ALL\n\n            SELECT\n                e.child_id,\n                t.depth + 1,\n                t.visited || e.child_id,\n                e.child_id = ANY(t.visited) AS cycle\n            FROM semantic_edges e\n            JOIN descendant_tree t ON e.parent_id = t.node_id\n            WHERE t.depth < $2\n              AND NOT t.cycle\n              AND NOT (e.child_id = ANY(t.visited))\n        )\n        SELECT\n            t.node_id,\n            t.depth,\n            n.business_key,\n            n.name,\n            n.entity_level\n        FROM descendant_tree t\n        JOIN semantic_nodes n ON n.id = t.node_id\n        ORDER BY t.depth ASC;\n        "
    }

    /// Executes multi-hop ancestor traversal on PostgreSQL in a single round-trip.
    pub async fn fetch_ancestors(
        client: &tokio_postgres::Client,
        target_id: Uuid,
        max_depth: i32,
    ) -> Result<Vec<AncestorRow>, PersistenceError> {
        let rows = client
            .query(Self::postgres_ancestor_query(), &[&target_id, &max_depth])
            .await?;

        let ancestors = rows
            .into_iter()
            .map(|row| AncestorRow {
                node_id: row.get(0),
                depth: row.get(1),
                business_key: row.get(2),
                name: row.get(3),
                entity_level: row.get(4),
            })
            .collect();

        Ok(ancestors)
    }

    /// Executes historical point-in-time multi-hop ancestor traversal on PostgreSQL in a single round-trip.
    pub async fn fetch_ancestors_as_of(
        client: &tokio_postgres::Client,
        target_id: Uuid,
        max_depth: i32,
        as_of: DateTime<Utc>,
    ) -> Result<Vec<AncestorRow>, PersistenceError> {
        let rows = client
            .query(
                Self::postgres_ancestor_as_of_query(),
                &[&target_id, &max_depth, &as_of],
            )
            .await?;

        let ancestors = rows
            .into_iter()
            .map(|row| AncestorRow {
                node_id: row.get(0),
                depth: row.get(1),
                business_key: row.get(2),
                name: row.get(3),
                entity_level: row.get(4),
            })
            .collect();

        Ok(ancestors)
    }

    /// Executes multi-hop descendant traversal on PostgreSQL in a single round-trip.
    pub async fn fetch_descendants(
        client: &tokio_postgres::Client,
        target_id: Uuid,
        max_depth: i32,
    ) -> Result<Vec<DescendantRow>, PersistenceError> {
        let rows = client
            .query(Self::postgres_descendant_query(), &[&target_id, &max_depth])
            .await?;

        let descendants = rows
            .into_iter()
            .map(|row| DescendantRow {
                node_id: row.get(0),
                depth: row.get(1),
                business_key: row.get(2),
                name: row.get(3),
                entity_level: row.get(4),
            })
            .collect();

        Ok(descendants)
    }

    /// Executes multi-hop ancestor traversal on an SQLite connection using `diesel-cte-ext`.
    #[cfg(feature = "sqlite")]
    pub fn fetch_ancestors_sqlite(
        conn: &mut diesel::SqliteConnection,
        target_id: &str,
        max_depth: i32,
    ) -> Result<Vec<crate::cte::HierarchyItem>, diesel::result::Error> {
        let config = crate::cte::HierarchicalTableConfig::default_semantic();
        crate::cte::CteHierarchyBuilder::fetch_ancestors_sqlite(conn, &config, target_id, max_depth)
    }

    /// Executes multi-hop descendant traversal on an SQLite connection using `diesel-cte-ext`.
    #[cfg(feature = "sqlite")]
    pub fn fetch_descendants_sqlite(
        conn: &mut diesel::SqliteConnection,
        root_id: &str,
        max_depth: i32,
    ) -> Result<Vec<crate::cte::HierarchyItem>, diesel::result::Error> {
        let config = crate::cte::HierarchicalTableConfig::default_semantic();
        crate::cte::CteHierarchyBuilder::fetch_descendants_sqlite(conn, &config, root_id, max_depth)
    }

    /// Executes multi-hop ancestor traversal asynchronously on PostgreSQL using `diesel-cte-ext`.
    #[cfg(all(feature = "postgres-async", feature = "diesel-async"))]
    pub async fn fetch_ancestors_async(
        conn: &mut diesel_async::AsyncPgConnection,
        target_id: &str,
        max_depth: i32,
    ) -> Result<Vec<crate::cte::HierarchyItem>, diesel::result::Error> {
        let config = crate::cte::HierarchicalTableConfig::default_semantic();
        crate::cte::CteHierarchyBuilder::fetch_ancestors_pg_async(conn, &config, target_id, max_depth).await
    }

    /// Executes multi-hop descendant traversal asynchronously on PostgreSQL using `diesel-cte-ext`.
    #[cfg(all(feature = "postgres-async", feature = "diesel-async"))]
    pub async fn fetch_descendants_async(
        conn: &mut diesel_async::AsyncPgConnection,
        root_id: &str,
        max_depth: i32,
    ) -> Result<Vec<crate::cte::HierarchyItem>, diesel::result::Error> {
        let config = crate::cte::HierarchicalTableConfig::default_semantic();
        crate::cte::CteHierarchyBuilder::fetch_descendants_pg_async(conn, &config, root_id, max_depth).await
    }
}
