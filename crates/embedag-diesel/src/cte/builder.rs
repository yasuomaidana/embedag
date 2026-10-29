//! Recursive CTE query builder for compile-time safe dialect-aware graph traversals.

use super::config::{HierarchicalTableConfig, HierarchyItem};
#[cfg(any(
    feature = "sqlite",
    all(feature = "postgres-async", feature = "diesel-async"),
    feature = "turso-async"
))]
use super::model::HierarchicalModel;
#[cfg(feature = "sqlite")]
use diesel::RunQueryDsl;
#[cfg(any(
    feature = "sqlite",
    all(feature = "postgres-async", feature = "diesel-async")
))]
use diesel::dsl::sql;
#[cfg(any(
    feature = "sqlite",
    all(feature = "postgres-async", feature = "diesel-async")
))]
use diesel::sql_types::{Integer, Text};
#[cfg(feature = "sqlite")]
use diesel::sqlite::SqliteConnection;
#[cfg(any(
    feature = "sqlite",
    all(feature = "postgres-async", feature = "diesel-async")
))]
use diesel_cte_ext::{RecursiveCTEExt, RecursiveParts};

/// Query builder for compiling and executing recursive CTE traversals using `diesel-cte-ext`.
pub struct CteHierarchyBuilder;

impl CteHierarchyBuilder {
    /// Traverses ancestors of `target_id` up to `max_depth` on an SQLite connection.
    ///
    /// Single-roundtrip SQL query assembled via `diesel_cte_ext::with_recursive`.
    #[cfg(feature = "sqlite")]
    pub fn fetch_ancestors_sqlite(
        conn: &mut SqliteConnection,
        config: &HierarchicalTableConfig,
        target_id: &str,
        max_depth: i32,
    ) -> diesel::QueryResult<Vec<HierarchyItem>> {
        let table = &config.table_name;
        let id_col = &config.id_column;
        let parent_col = &config.parent_column;
        let name_col = &config.name_column;

        // Base Anchor (Seed Query): seed with target node at depth 0
        let seed_sql = format!(
            "SELECT {id_col} AS curr_id, {parent_col} AS parent_ref, 0 AS depth, {name_col} AS curr_name \
             FROM {table} \
             WHERE {id_col} = '{target_id}'"
        );

        // Recursive Member (Step Query): recursively join upwards on parent_ref
        let step_sql = format!(
            "SELECT t.{id_col}, t.{parent_col}, a.depth + 1, t.{name_col} \
             FROM {table} t \
             INNER JOIN ancestors_cte a ON t.{id_col} = a.parent_ref \
             WHERE a.depth < {max_depth}"
        );

        // Body Query: select ancestors strictly above target (depth > 0)
        let body_sql =
            "SELECT curr_id, depth, curr_name FROM ancestors_cte WHERE depth > 0 ORDER BY depth ASC".to_string();

        let parts = RecursiveParts::new(
            sql::<(Text, Text, Integer, Text)>(&seed_sql),
            sql::<(Text, Text, Integer, Text)>(&step_sql),
            sql::<(Text, Integer, Text)>(&body_sql),
        );

        let rows: Vec<(String, i32, String)> = conn
            .with_recursive(
                "ancestors_cte",
                &["curr_id", "parent_ref", "depth", "curr_name"],
                parts,
            )
            .load::<(String, i32, String)>(conn)?;

        Ok(rows
            .into_iter()
            .map(|(id, depth, name)| HierarchyItem { id, depth, name })
            .collect())
    }

    /// Traverses ancestors with automatic set deduplication (UNION) via `diesel_cte_ext::with_recursive_not_all`.
    #[cfg(feature = "sqlite")]
    pub fn fetch_ancestors_dedup_sqlite(
        conn: &mut SqliteConnection,
        config: &HierarchicalTableConfig,
        target_id: &str,
        max_depth: i32,
    ) -> diesel::QueryResult<Vec<HierarchyItem>> {
        let table = &config.table_name;
        let id_col = &config.id_column;
        let parent_col = &config.parent_column;
        let name_col = &config.name_column;

        let seed_sql = format!(
            "SELECT {id_col} AS curr_id, {parent_col} AS parent_ref, 0 AS depth, {name_col} AS curr_name \
             FROM {table} \
             WHERE {id_col} = '{target_id}'"
        );

        let step_sql = format!(
            "SELECT t.{id_col}, t.{parent_col}, a.depth + 1, t.{name_col} \
             FROM {table} t \
             INNER JOIN ancestors_cte a ON t.{id_col} = a.parent_ref \
             WHERE a.depth < {max_depth}"
        );

        let body_sql =
            "SELECT curr_id, depth, curr_name FROM ancestors_cte WHERE depth > 0 ORDER BY depth ASC".to_string();

        let parts = RecursiveParts::new(
            sql::<(Text, Text, Integer, Text)>(&seed_sql),
            sql::<(Text, Text, Integer, Text)>(&step_sql),
            sql::<(Text, Integer, Text)>(&body_sql),
        );

        let rows: Vec<(String, i32, String)> = conn
            .with_recursive_not_all(
                "ancestors_cte",
                &["curr_id", "parent_ref", "depth", "curr_name"],
                parts,
            )
            .load::<(String, i32, String)>(conn)?;

        Ok(rows
            .into_iter()
            .map(|(id, depth, name)| HierarchyItem { id, depth, name })
            .collect())
    }

    /// Traverses descendants of `root_id` down to `max_depth` on an SQLite connection.
    #[cfg(feature = "sqlite")]
    pub fn fetch_descendants_sqlite(
        conn: &mut SqliteConnection,
        config: &HierarchicalTableConfig,
        root_id: &str,
        max_depth: i32,
    ) -> diesel::QueryResult<Vec<HierarchyItem>> {
        let table = &config.table_name;
        let id_col = &config.id_column;
        let parent_col = &config.parent_column;
        let name_col = &config.name_column;

        let seed_sql = format!(
            "SELECT {id_col} AS curr_id, 0 AS depth, {name_col} AS curr_name \
             FROM {table} \
             WHERE {id_col} = '{root_id}'"
        );

        let step_sql = if let Some(breadth) = config.max_breadth_per_node {
            format!(
                "SELECT t.{id_col}, d.depth + 1, t.{name_col} \
                 FROM ( \
                     SELECT {id_col}, {parent_col}, {name_col}, \
                            ROW_NUMBER() OVER (PARTITION BY {parent_col} ORDER BY {name_col}) as rn \
                     FROM {table} \
                 ) t \
                 INNER JOIN descendants_cte d ON t.{parent_col} = d.curr_id \
                 WHERE d.depth < {max_depth} AND t.rn <= {breadth}"
            )
        } else {
            format!(
                "SELECT t.{id_col}, d.depth + 1, t.{name_col} \
                 FROM {table} t \
                 INNER JOIN descendants_cte d ON t.{parent_col} = d.curr_id \
                 WHERE d.depth < {max_depth}"
            )
        };

        let body_sql =
            "SELECT curr_id, depth, curr_name FROM descendants_cte WHERE depth > 0 ORDER BY depth ASC".to_string();

        let parts = RecursiveParts::new(
            sql::<(Text, Integer, Text)>(&seed_sql),
            sql::<(Text, Integer, Text)>(&step_sql),
            sql::<(Text, Integer, Text)>(&body_sql),
        );

        let rows: Vec<(String, i32, String)> = conn
            .with_recursive("descendants_cte", &["curr_id", "depth", "curr_name"], parts)
            .load::<(String, i32, String)>(conn)?;

        Ok(rows
            .into_iter()
            .map(|(id, depth, name)| HierarchyItem { id, depth, name })
            .collect())
    }

    /// Type-safe model ancestor traversal for SQLite.
    #[cfg(feature = "sqlite")]
    pub fn fetch_model_ancestors_sqlite<M: HierarchicalModel>(
        conn: &mut SqliteConnection,
        target_id: &str,
        max_depth: i32,
    ) -> diesel::QueryResult<Vec<HierarchyItem>> {
        Self::fetch_ancestors_sqlite(conn, &M::table_config(), target_id, max_depth)
    }

    /// Type-safe model descendant traversal for SQLite.
    #[cfg(feature = "sqlite")]
    pub fn fetch_model_descendants_sqlite<M: HierarchicalModel>(
        conn: &mut SqliteConnection,
        root_id: &str,
        max_depth: i32,
    ) -> diesel::QueryResult<Vec<HierarchyItem>> {
        Self::fetch_descendants_sqlite(conn, &M::table_config(), root_id, max_depth)
    }

    /// Type-safe model ancestor deduplication traversal for SQLite.
    #[cfg(feature = "sqlite")]
    pub fn fetch_model_ancestors_dedup_sqlite<M: HierarchicalModel>(
        conn: &mut SqliteConnection,
        target_id: &str,
        max_depth: i32,
    ) -> diesel::QueryResult<Vec<HierarchyItem>> {
        Self::fetch_ancestors_dedup_sqlite(conn, &M::table_config(), target_id, max_depth)
    }

    /// Traverses ancestors of `target_id` up to `max_depth` asynchronously on PostgreSQL via `diesel-async`.
    #[cfg(all(feature = "postgres-async", feature = "diesel-async"))]
    pub async fn fetch_ancestors_pg_async(
        conn: &mut diesel_async::AsyncPgConnection,
        config: &HierarchicalTableConfig,
        target_id: &str,
        max_depth: i32,
    ) -> diesel::QueryResult<Vec<HierarchyItem>> {
        let table = &config.table_name;
        let id_col = &config.id_column;
        let parent_col = &config.parent_column;
        let name_col = &config.name_column;

        let seed_sql = format!(
            "SELECT {id_col}::text AS curr_id, {parent_col}::text AS parent_ref, 0 AS depth, {name_col}::text AS curr_name \
             FROM {table} \
             WHERE {id_col}::text = '{target_id}'"
        );

        let step_sql = format!(
            "SELECT t.{id_col}::text, t.{parent_col}::text, a.depth + 1, t.{name_col}::text \
             FROM {table} t \
             INNER JOIN ancestors_cte a ON t.{id_col}::text = a.parent_ref \
             WHERE a.depth < {max_depth}"
        );

        let body_sql =
            "SELECT curr_id, depth, curr_name FROM ancestors_cte WHERE depth > 0 ORDER BY depth ASC".to_string();

        let parts = RecursiveParts::new(
            sql::<(Text, Text, Integer, Text)>(&seed_sql),
            sql::<(Text, Text, Integer, Text)>(&step_sql),
            sql::<(Text, Integer, Text)>(&body_sql),
        );

        let query = conn.with_recursive(
            "ancestors_cte",
            &["curr_id", "parent_ref", "depth", "curr_name"],
            parts,
        );
        let rows: Vec<(String, i32, String)> = diesel_async::RunQueryDsl::load(query, conn).await?;

        Ok(rows
            .into_iter()
            .map(|(id, depth, name)| HierarchyItem { id, depth, name })
            .collect())
    }

    /// Traverses descendants of `root_id` down to `max_depth` asynchronously on PostgreSQL via `diesel-async`.
    #[cfg(all(feature = "postgres-async", feature = "diesel-async"))]
    pub async fn fetch_descendants_pg_async(
        conn: &mut diesel_async::AsyncPgConnection,
        config: &HierarchicalTableConfig,
        root_id: &str,
        max_depth: i32,
    ) -> diesel::QueryResult<Vec<HierarchyItem>> {
        let table = &config.table_name;
        let id_col = &config.id_column;
        let parent_col = &config.parent_column;
        let name_col = &config.name_column;

        let seed_sql = format!(
            "SELECT {id_col}::text AS curr_id, 0 AS depth, {name_col}::text AS curr_name \
             FROM {table} \
             WHERE {id_col}::text = '{root_id}'"
        );

        let step_sql = if let Some(breadth) = config.max_breadth_per_node {
            format!(
                "SELECT t.{id_col}::text, d.depth + 1, t.{name_col}::text \
                 FROM ( \
                     SELECT {id_col}, {parent_col}, {name_col}, \
                            ROW_NUMBER() OVER (PARTITION BY {parent_col} ORDER BY {name_col}) as rn \
                     FROM {table} \
                 ) t \
                 INNER JOIN descendants_cte d ON t.{parent_col}::text = d.curr_id \
                 WHERE d.depth < {max_depth} AND t.rn <= {breadth}"
            )
        } else {
            format!(
                "SELECT t.{id_col}::text, d.depth + 1, t.{name_col}::text \
                 FROM {table} t \
                 INNER JOIN descendants_cte d ON t.{parent_col}::text = d.curr_id \
                 WHERE d.depth < {max_depth}"
            )
        };

        let body_sql =
            "SELECT curr_id, depth, curr_name FROM descendants_cte WHERE depth > 0 ORDER BY depth ASC".to_string();

        let parts = RecursiveParts::new(
            sql::<(Text, Integer, Text)>(&seed_sql),
            sql::<(Text, Integer, Text)>(&step_sql),
            sql::<(Text, Integer, Text)>(&body_sql),
        );

        let query =
            conn.with_recursive("descendants_cte", &["curr_id", "depth", "curr_name"], parts);
        let rows: Vec<(String, i32, String)> = diesel_async::RunQueryDsl::load(query, conn).await?;

        Ok(rows
            .into_iter()
            .map(|(id, depth, name)| HierarchyItem { id, depth, name })
            .collect())
    }

    /// Type-safe model ancestor traversal for PostgreSQL async.
    #[cfg(all(feature = "postgres-async", feature = "diesel-async"))]
    pub async fn fetch_model_ancestors_pg_async<M: HierarchicalModel>(
        conn: &mut diesel_async::AsyncPgConnection,
        target_id: &str,
        max_depth: i32,
    ) -> diesel::QueryResult<Vec<HierarchyItem>> {
        Self::fetch_ancestors_pg_async(conn, &M::table_config(), target_id, max_depth).await
    }

    /// Type-safe model descendant traversal for PostgreSQL async.
    #[cfg(all(feature = "postgres-async", feature = "diesel-async"))]
    pub async fn fetch_model_descendants_pg_async<M: HierarchicalModel>(
        conn: &mut diesel_async::AsyncPgConnection,
        root_id: &str,
        max_depth: i32,
    ) -> diesel::QueryResult<Vec<HierarchyItem>> {
        Self::fetch_descendants_pg_async(conn, &M::table_config(), root_id, max_depth).await
    }

    /// Traverses ancestors of `target_id` up to `max_depth` asynchronously on Turso / libSQL via `diesel-async`.
    #[cfg(feature = "turso-async")]
    pub async fn fetch_ancestors_turso_async(
        conn: &mut crate::migrations::AsyncLibSqlConnection,
        config: &HierarchicalTableConfig,
        target_id: &str,
        max_depth: i32,
    ) -> diesel::QueryResult<Vec<HierarchyItem>> {
        let table = &config.table_name;
        let id_col = &config.id_column;
        let parent_col = &config.parent_column;
        let name_col = &config.name_column;

        let sql = format!(
            "WITH RECURSIVE ancestors_cte(curr_id, parent_ref, depth, curr_name) AS (\
                SELECT {id_col}, {parent_col}, 0, {name_col} \
                FROM {table} \
                WHERE {id_col} = '{target_id}' \
                UNION ALL \
                SELECT t.{id_col}, t.{parent_col}, a.depth + 1, t.{name_col} \
                FROM {table} t \
                INNER JOIN ancestors_cte a ON t.{id_col} = a.parent_ref \
                WHERE a.depth < {max_depth} \
            ) \
            SELECT curr_id, depth, curr_name FROM ancestors_cte WHERE depth > 0 ORDER BY depth ASC;"
        );

        let rows: Vec<TursoHierarchyRow> =
            diesel_async::RunQueryDsl::load(diesel::sql_query(sql), conn).await?;
        Ok(rows
            .into_iter()
            .map(|r| HierarchyItem {
                id: r.curr_id,
                depth: r.depth,
                name: r.curr_name,
            })
            .collect())
    }

    /// Traverses descendants of `root_id` down to `max_depth` asynchronously on Turso / libSQL via `diesel-async`.
    #[cfg(feature = "turso-async")]
    pub async fn fetch_descendants_turso_async(
        conn: &mut crate::migrations::AsyncLibSqlConnection,
        config: &HierarchicalTableConfig,
        root_id: &str,
        max_depth: i32,
    ) -> diesel::QueryResult<Vec<HierarchyItem>> {
        let table = &config.table_name;
        let id_col = &config.id_column;
        let parent_col = &config.parent_column;
        let name_col = &config.name_column;

        let sql = if let Some(breadth) = config.max_breadth_per_node {
            format!(
                "WITH RECURSIVE descendants_cte(curr_id, depth, curr_name) AS (\
                    SELECT {id_col}, 0, {name_col} \
                    FROM {table} \
                    WHERE {id_col} = '{root_id}' \
                    UNION ALL \
                    SELECT t.{id_col}, d.depth + 1, t.{name_col} \
                    FROM ( \
                        SELECT {id_col}, {parent_col}, {name_col}, \
                               ROW_NUMBER() OVER (PARTITION BY {parent_col} ORDER BY {name_col}) as rn \
                        FROM {table} \
                    ) t \
                    INNER JOIN descendants_cte d ON t.{parent_col} = d.curr_id \
                    WHERE d.depth < {max_depth} AND t.rn <= {breadth} \
                ) \
                SELECT curr_id, depth, curr_name FROM descendants_cte WHERE depth > 0 ORDER BY depth ASC;"
            )
        } else {
            format!(
                "WITH RECURSIVE descendants_cte(curr_id, depth, curr_name) AS (\
                    SELECT {id_col}, 0, {name_col} \
                    FROM {table} \
                    WHERE {id_col} = '{root_id}' \
                    UNION ALL \
                    SELECT t.{id_col}, d.depth + 1, t.{name_col} \
                    FROM {table} t \
                    INNER JOIN descendants_cte d ON t.{parent_col} = d.curr_id \
                    WHERE d.depth < {max_depth} \
                ) \
                SELECT curr_id, depth, curr_name FROM descendants_cte WHERE depth > 0 ORDER BY depth ASC;"
            )
        };

        let rows: Vec<TursoHierarchyRow> =
            diesel_async::RunQueryDsl::load(diesel::sql_query(sql), conn).await?;
        Ok(rows
            .into_iter()
            .map(|r| HierarchyItem {
                id: r.curr_id,
                depth: r.depth,
                name: r.curr_name,
            })
            .collect())
    }

    /// Type-safe model ancestor traversal for Turso / libSQL async.
    #[cfg(feature = "turso-async")]
    pub async fn fetch_model_ancestors_turso_async<M: HierarchicalModel>(
        conn: &mut crate::migrations::AsyncLibSqlConnection,
        target_id: &str,
        max_depth: i32,
    ) -> diesel::QueryResult<Vec<HierarchyItem>> {
        Self::fetch_ancestors_turso_async(conn, &M::table_config(), target_id, max_depth).await
    }

    /// Type-safe model descendant traversal for Turso / libSQL async.
    #[cfg(feature = "turso-async")]
    pub async fn fetch_model_descendants_turso_async<M: HierarchicalModel>(
        conn: &mut crate::migrations::AsyncLibSqlConnection,
        root_id: &str,
        max_depth: i32,
    ) -> diesel::QueryResult<Vec<HierarchyItem>> {
        Self::fetch_descendants_turso_async(conn, &M::table_config(), root_id, max_depth).await
    }
}

/// Raw query row mapping for Turso CTE hierarchy queries.
#[cfg(feature = "turso-async")]
#[derive(Debug, Clone, diesel::QueryableByName)]
pub struct TursoHierarchyRow {
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub curr_id: String,
    #[diesel(sql_type = diesel::sql_types::Integer)]
    pub depth: i32,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub curr_name: String,
}
