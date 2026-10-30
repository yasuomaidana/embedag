//! Type-safe recursive CTE traversal builder with cycle guardrails for PostgreSQL and SQLite.
//!
//! Aligned with [`leynos/diesel-cte-ext`](https://github.com/leynos/diesel-cte-ext) architecture,
//! providing `RecursiveCTEExt` and `RecursiveParts` traversal contracts with SQL-level visited array path
//! cycle detection and deduplication.

use crate::ops::PersistenceError;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A node of a lineage (ancestors or descendants) retrieved in a single roundtrip via recursive CTE.
#[derive(Debug, Clone, PartialEq)]
pub struct LineageRow {
    pub node_id: Uuid,
    pub depth: i32,
    pub business_key: String,
    pub name: String,
    pub entity_level: String,
}

/// Ancestor lineage entry retrieved in a single roundtrip via recursive CTE.
pub type AncestorRow = LineageRow;

/// Descendant entry retrieved in a single roundtrip via recursive CTE.
pub type DescendantRow = LineageRow;

impl LineageRow {
    /// Reads the five traversal columns of a PostgreSQL row.
    fn from_pg(row: tokio_postgres::Row) -> Self {
        Self {
            node_id: row.get(0),
            depth: row.get(1),
            business_key: row.get(2),
            name: row.get(3),
            entity_level: row.get(4),
        }
    }

    /// Reads a Turso / libSQL row, whose ids come back as text.
    #[cfg(feature = "turso-async")]
    fn from_turso(row: TursoTraversalRow) -> Self {
        Self {
            node_id: Uuid::parse_str(&row.node_id).unwrap_or_default(),
            depth: row.depth,
            business_key: row.business_key,
            name: row.name,
            entity_level: row.entity_level,
        }
    }
}

/// The default semantic table, with at most `max_breadth` children per node when given.
#[cfg(any(
    feature = "sqlite",
    all(feature = "postgres-async", feature = "diesel-async")
))]
fn semantic_config(max_breadth: Option<i32>) -> crate::cte::HierarchicalTableConfig {
    let config = crate::cte::HierarchicalTableConfig::default_semantic();
    match max_breadth {
        Some(breadth) => config.with_max_breadth_per_node(breadth),
        None => config,
    }
}

/// Defines the traversals over the default semantic table: each runs the `CteHierarchyBuilder`
/// function `$inner`, limiting the children per node when a `max_breadth` parameter is declared;
/// `async` entries are awaited.
macro_rules! semantic_traversals {
    ($(
        $(#[$meta:meta])*
        $name:ident($conn:ty, $id:ident $(, $breadth:ident)?) => $inner:ident $($async:ident)?;
    )*) => {$(
        $(#[$meta])*
        pub $($async)? fn $name(
            conn: &mut $conn,
            $id: &str,
            max_depth: i32 $(, $breadth: i32)?
        ) -> Result<Vec<crate::cte::HierarchyItem>, diesel::result::Error> {
            let config = semantic_config(None $(.or(Some($breadth)))?);
            let items = crate::cte::CteHierarchyBuilder::$inner(conn, &config, $id, max_depth);
            semantic_traversals!(@wait items $($async)?)
        }
    )*};
    (@wait $items:ident async) => { $items.await };
    (@wait $items:ident) => { $items };
}

/// Defines the PostgreSQL traversals that take a node id and a depth and run one of the
/// `RecursiveTraversalBuilder` queries.
macro_rules! pg_lineages {
    ($(
        $(#[$meta:meta])*
        $name:ident => $query:ident, $row:ident;
    )*) => {$(
        $(#[$meta])*
        pub async fn $name(
            client: &tokio_postgres::Client,
            target_id: Uuid,
            max_depth: i32,
        ) -> Result<Vec<$row>, PersistenceError> {
            Self::lineage(client, Self::$query(), &[&target_id, &max_depth]).await
        }
    )*};
}

/// Defines the Turso / libSQL traversals that take a node id and a depth and run one of the
/// `RecursiveTraversalBuilder` statements.
#[cfg(feature = "turso-async")]
macro_rules! turso_lineages {
    ($(
        $(#[$meta:meta])*
        $name:ident($id:ident) => $query:ident, $row:ident;
    )*) => {$(
        $(#[$meta])*
        pub async fn $name(
            conn: &mut crate::migrations::AsyncLibSqlConnection,
            $id: Uuid,
            max_depth: i32,
        ) -> Result<Vec<$row>, PersistenceError> {
            Self::lineage_turso(conn, Self::$query($id, max_depth)).await
        }
    )*};
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

    /// Runs a lineage query on PostgreSQL and reads its rows.
    async fn lineage(
        client: &tokio_postgres::Client,
        query: &str,
        params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<Vec<LineageRow>, PersistenceError> {
        let rows = client.query(query, params).await?;
        Ok(rows.into_iter().map(LineageRow::from_pg).collect())
    }

    /// Executes historical point-in-time multi-hop ancestor traversal on PostgreSQL in a single round-trip.
    pub async fn fetch_ancestors_as_of(
        client: &tokio_postgres::Client,
        target_id: Uuid,
        max_depth: i32,
        as_of: DateTime<Utc>,
    ) -> Result<Vec<AncestorRow>, PersistenceError> {
        let query = Self::postgres_ancestor_as_of_query();
        Self::lineage(client, query, &[&target_id, &max_depth, &as_of]).await
    }

    pg_lineages! {
        /// Executes multi-hop ancestor traversal on PostgreSQL in a single round-trip.
        fetch_ancestors => postgres_ancestor_query, AncestorRow;
        /// Executes multi-hop descendant traversal on PostgreSQL in a single round-trip.
        fetch_descendants => postgres_descendant_query, DescendantRow;
    }

    semantic_traversals! {
        /// Executes multi-hop ancestor traversal on an SQLite connection using `diesel-cte-ext`.
        #[cfg(feature = "sqlite")]
        fetch_ancestors_sqlite(diesel::SqliteConnection, target_id) => fetch_ancestors_sqlite;
        /// Executes multi-hop descendant traversal on an SQLite connection using `diesel-cte-ext`.
        #[cfg(feature = "sqlite")]
        fetch_descendants_sqlite(diesel::SqliteConnection, root_id) => fetch_descendants_sqlite;
        /// Executes multi-hop descendant traversal on an SQLite connection with breadth limits per level.
        #[cfg(feature = "sqlite")]
        fetch_descendants_bounded_sqlite(diesel::SqliteConnection, root_id, max_breadth) => fetch_descendants_sqlite;
        /// Executes multi-hop ancestor traversal on an SQLite connection with breadth limits per level.
        #[cfg(feature = "sqlite")]
        fetch_ancestors_bounded_sqlite(diesel::SqliteConnection, target_id, max_breadth) => fetch_ancestors_sqlite;
        /// Executes multi-hop ancestor traversal asynchronously on PostgreSQL using `diesel-cte-ext`.
        #[cfg(all(feature = "postgres-async", feature = "diesel-async"))]
        fetch_ancestors_async(diesel_async::AsyncPgConnection, target_id) => fetch_ancestors_pg_async async;
        /// Executes multi-hop descendant traversal asynchronously on PostgreSQL using `diesel-cte-ext`.
        #[cfg(all(feature = "postgres-async", feature = "diesel-async"))]
        fetch_descendants_async(diesel_async::AsyncPgConnection, root_id) => fetch_descendants_pg_async async;
    }

    /// Constructs Turso / libSQL recursive CTE SQL query for multi-hop ancestors with cycle guard.
    pub fn turso_ancestor_query(target_id: Uuid, max_depth: i32) -> String {
        format!(
            "WITH RECURSIVE ancestor_tree(node_id, depth, visited_path) AS (
                SELECT
                    parent_id,
                    1 AS depth,
                    ',' || '{target_id}' || ',' || parent_id || ',' AS visited_path
                FROM semantic_edges
                WHERE child_id = '{target_id}'
                UNION ALL
                SELECT
                    e.parent_id,
                    t.depth + 1,
                    t.visited_path || e.parent_id || ','
                FROM semantic_edges e
                JOIN ancestor_tree t ON e.child_id = t.node_id
                WHERE t.depth < {max_depth}
                  AND INSTR(t.visited_path, ',' || e.parent_id || ',') = 0
            )
            SELECT
                t.node_id,
                t.depth,
                n.business_key,
                n.name,
                n.entity_level
            FROM ancestor_tree t
            JOIN semantic_nodes n ON n.id = t.node_id
            ORDER BY t.depth ASC;"
        )
    }

    /// Constructs Turso / libSQL recursive CTE SQL query for multi-hop descendants with cycle guard.
    pub fn turso_descendant_query(root_id: Uuid, max_depth: i32) -> String {
        format!(
            "WITH RECURSIVE descendant_tree(node_id, depth, visited_path) AS (
                SELECT
                    child_id,
                    1 AS depth,
                    ',' || '{root_id}' || ',' || child_id || ',' AS visited_path
                FROM semantic_edges
                WHERE parent_id = '{root_id}'
                UNION ALL
                SELECT
                    e.child_id,
                    t.depth + 1,
                    t.visited_path || e.child_id || ','
                FROM semantic_edges e
                JOIN descendant_tree t ON e.parent_id = t.node_id
                WHERE t.depth < {max_depth}
                  AND INSTR(t.visited_path, ',' || e.child_id || ',') = 0
            )
            SELECT
                t.node_id,
                t.depth,
                n.business_key,
                n.name,
                n.entity_level
            FROM descendant_tree t
            JOIN semantic_nodes n ON n.id = t.node_id
            ORDER BY t.depth ASC;"
        )
    }

    /// Runs a lineage statement on Turso / libSQL and reads its rows.
    #[cfg(feature = "turso-async")]
    async fn lineage_turso(
        conn: &mut crate::migrations::AsyncLibSqlConnection,
        sql: String,
    ) -> Result<Vec<LineageRow>, PersistenceError> {
        let rows: Vec<TursoTraversalRow> =
            diesel_async::RunQueryDsl::load(diesel::sql_query(sql), conn)
                .await
                .map_err(PersistenceError::Diesel)?;
        Ok(rows.into_iter().map(LineageRow::from_turso).collect())
    }

    #[cfg(feature = "turso-async")]
    turso_lineages! {
        /// Executes multi-hop ancestor traversal asynchronously on Turso / libSQL using .
        fetch_ancestors_turso_async(target_id) => turso_ancestor_query, AncestorRow;
        /// Executes multi-hop descendant traversal asynchronously on Turso / libSQL using .
        fetch_descendants_turso_async(root_id) => turso_descendant_query, DescendantRow;
    }
}

/// Raw query row mapping for Turso traversal queries.
#[cfg(feature = "turso-async")]
#[derive(Debug, Clone, diesel::QueryableByName)]
pub struct TursoTraversalRow {
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub node_id: String,
    #[diesel(sql_type = diesel::sql_types::Integer)]
    pub depth: i32,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub business_key: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub name: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub entity_level: String,
}
