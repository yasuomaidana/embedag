//! Generic Hierarchical Recursive CTE Traversal Engine using `diesel-cte-ext`.
//!
//! Provides compile-time safe, dialect-aware recursive Common Table Expressions
//! for both SQLite and PostgreSQL. Supports custom user-defined tables,
//! self-referencing adjacency lists (e.g. `categories(id, parent_id, name)`),
//! and dual-table node/edge schemas. Completely agnostic of any domain model.

use diesel::RunQueryDsl;
use diesel::dsl::sql;
use diesel::sql_types::{Integer, Text};
#[cfg(feature = "sqlite")]
use diesel::sqlite::SqliteConnection;
pub use diesel_cte_ext::{
    CteParts, RecursiveCTEExt, RecursiveParts, with_cte, with_recursive, with_recursive_not_all,
};
pub use embedag_derive::HierarchicalModel;
use serde::{Deserialize, Serialize};

/// Configuration describing an arbitrary hierarchical database table.
///
/// Enables developers with existing database migrations and domain models
/// to leverage `embedag`'s recursive CTE capabilities without forcing rigid schema adoption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HierarchicalTableConfig {
    /// Name of the relational table (e.g., "categories", "taxonomies", "org_units").
    pub table_name: String,
    /// Column name representing the node identifier (e.g., "id" or "category_id").
    pub id_column: String,
    /// Column name representing the optional parent reference (e.g., "parent_id" or "parent_category_id").
    pub parent_column: String,
    /// Column name representing the human-readable entity name (e.g., "name" or "label").
    pub name_column: String,
    /// Optional semantic description or context instructions about the table to feed LLM prompts.
    pub description: Option<String>,
    /// Optional limit on the number of child branches traversed per node at each depth level.
    pub max_breadth_per_node: Option<i32>,
}

impl HierarchicalTableConfig {
    /// Creates a configuration for an adjacency list hierarchy table.
    pub fn new(
        table_name: impl Into<String>,
        id_column: impl Into<String>,
        parent_column: impl Into<String>,
        name_column: impl Into<String>,
    ) -> Self {
        Self {
            table_name: table_name.into(),
            id_column: id_column.into(),
            parent_column: parent_column.into(),
            name_column: name_column.into(),
            description: None,
            max_breadth_per_node: None,
        }
    }

    /// Attaches an optional semantic description of this table to feed LLM extraction prompts.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Sets an upper bound on child branches traversed per node at each depth.
    pub fn with_max_breadth_per_node(mut self, breadth: i32) -> Self {
        self.max_breadth_per_node = Some(breadth);
        self
    }

    /// Configuration for embedag's default semantic nodes/edges table.
    pub fn default_semantic() -> Self {
        Self::new("semantic_nodes", "id", "parent_id", "name")
    }

    /// Formats this table configuration into structured context suitable for feeding LLM prompts.
    pub fn table_context(&self) -> String {
        let mut ctx = format!(
            "Table: {}\n- Key Column: {}\n- Parent Column: {}\n- Label Column: {}",
            self.table_name, self.id_column, self.parent_column, self.name_column
        );
        if let Some(desc) = &self.description {
            ctx.push_str(&format!("\n- Description: {desc}"));
        }
        ctx
    }
}

/// Represents an item in a traversed hierarchy lineage with its relative depth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HierarchyItem {
    /// Unique identifier of the node.
    pub id: String,
    /// Relative traversal depth (1 = immediate parent/child, 2 = grandparent, etc.).
    pub depth: i32,
    /// Name or label of the node.
    pub name: String,
}

/// Trait implemented by domain models that represent hierarchical graph nodes.
pub trait HierarchicalNode {
    /// Type of the node identifier.
    type Id: Clone + ToString + std::fmt::Display;

    /// Unique identifier of the current node.
    fn node_id(&self) -> Self::Id;

    /// Parent node identifier, or `None` if this is a root node.
    fn parent_id(&self) -> Option<Self::Id>;

    /// Human-readable label or category name.
    fn node_name(&self) -> &str;
}

/// Trait implemented by models representing hierarchical tables (derived via `#[derive(HierarchicalModel)]`).
///
/// Automatically generates type-safe Diesel CTE queries, column mappings, and step queries.
pub trait HierarchicalModel {
    /// Name of the relational table.
    fn table_name() -> &'static str;
    /// Identifier column name.
    fn id_column() -> &'static str;
    /// Parent identifier column name.
    fn parent_column() -> &'static str;
    /// Name/label column name.
    fn name_column() -> &'static str;
    /// Optional semantic description of the table to provide context to the LLM.
    fn table_description() -> Option<&'static str> {
        None
    }

    /// Produces a [`HierarchicalTableConfig`] for this model including attached description.
    fn table_config() -> HierarchicalTableConfig {
        let mut config = HierarchicalTableConfig::new(
            Self::table_name(),
            Self::id_column(),
            Self::parent_column(),
            Self::name_column(),
        );
        if let Some(desc) = Self::table_description() {
            config = config.with_description(desc);
        }
        config
    }

    /// Formats the model's table context for feeding LLM extraction prompts.
    fn table_context() -> String {
        Self::table_config().table_context()
    }

    /// Traverses ancestors of `target_id` up to `max_depth` on an SQLite connection.
    #[cfg(feature = "sqlite")]
    fn fetch_ancestors_sqlite(
        conn: &mut SqliteConnection,
        target_id: &str,
        max_depth: i32,
    ) -> diesel::QueryResult<Vec<HierarchyItem>>
    where
        Self: Sized,
    {
        CteHierarchyBuilder::fetch_ancestors_sqlite(
            conn,
            &Self::table_config(),
            target_id,
            max_depth,
        )
    }

    /// Traverses descendants of `root_id` down to `max_depth` on an SQLite connection.
    #[cfg(feature = "sqlite")]
    fn fetch_descendants_sqlite(
        conn: &mut SqliteConnection,
        root_id: &str,
        max_depth: i32,
    ) -> diesel::QueryResult<Vec<HierarchyItem>>
    where
        Self: Sized,
    {
        CteHierarchyBuilder::fetch_descendants_sqlite(
            conn,
            &Self::table_config(),
            root_id,
            max_depth,
        )
    }

    /// Traverses ancestors with deduplication on an SQLite connection.
    #[cfg(feature = "sqlite")]
    fn fetch_ancestors_dedup_sqlite(
        conn: &mut SqliteConnection,
        target_id: &str,
        max_depth: i32,
    ) -> diesel::QueryResult<Vec<HierarchyItem>>
    where
        Self: Sized,
    {
        CteHierarchyBuilder::fetch_ancestors_dedup_sqlite(
            conn,
            &Self::table_config(),
            target_id,
            max_depth,
        )
    }

    /// Traverses ancestors of `target_id` up to `max_depth` asynchronously on PostgreSQL via `diesel-async`.
    #[cfg(all(feature = "postgres-async", feature = "diesel-async"))]
    fn fetch_ancestors_pg_async<'a>(
        conn: &'a mut diesel_async::AsyncPgConnection,
        target_id: &'a str,
        max_depth: i32,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = diesel::QueryResult<Vec<HierarchyItem>>> + Send + 'a>,
    >
    where
        Self: Sized + 'a,
    {
        let config = Self::table_config();
        Box::pin(async move {
            CteHierarchyBuilder::fetch_ancestors_pg_async(conn, &config, target_id, max_depth).await
        })
    }

    /// Traverses descendants of `root_id` down to `max_depth` asynchronously on PostgreSQL via `diesel-async`.
    #[cfg(all(feature = "postgres-async", feature = "diesel-async"))]
    fn fetch_descendants_pg_async<'a>(
        conn: &'a mut diesel_async::AsyncPgConnection,
        root_id: &'a str,
        max_depth: i32,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = diesel::QueryResult<Vec<HierarchyItem>>> + Send + 'a>,
    >
    where
        Self: Sized + 'a,
    {
        let config = Self::table_config();
        Box::pin(async move {
            CteHierarchyBuilder::fetch_descendants_pg_async(conn, &config, root_id, max_depth).await
        })
    }

    /// Generates the recursive CTE step query for ancestor traversal.
    fn ancestor_step_sql(max_depth: i32) -> String {
        format!(
            "SELECT t.{}, t.{}, a.depth + 1, t.{} \
             FROM {} t \
             INNER JOIN ancestors_cte a ON t.{} = a.parent_ref \
             WHERE a.depth < {}",
            Self::id_column(),
            Self::parent_column(),
            Self::name_column(),
            Self::table_name(),
            Self::id_column(),
            max_depth
        )
    }

    /// Generates the recursive CTE seed query for ancestor traversal.
    fn ancestor_seed_sql(target_id: &str) -> String {
        format!(
            "SELECT {} AS curr_id, {} AS parent_ref, 0 AS depth, {} AS curr_name \
             FROM {} \
             WHERE {} = '{}'",
            Self::id_column(),
            Self::parent_column(),
            Self::name_column(),
            Self::table_name(),
            Self::id_column(),
            target_id
        )
    }

    /// Generates the recursive CTE step query for descendant traversal.
    fn descendant_step_sql(max_depth: i32) -> String {
        format!(
            "SELECT t.{}, d.depth + 1, t.{} \
             FROM {} t \
             INNER JOIN descendants_cte d ON t.{} = d.curr_id \
             WHERE d.depth < {}",
            Self::id_column(),
            Self::name_column(),
            Self::table_name(),
            Self::parent_column(),
            max_depth
        )
    }

    /// Generates the recursive CTE seed query for descendant traversal.
    fn descendant_seed_sql(root_id: &str) -> String {
        format!(
            "SELECT {} AS curr_id, 0 AS depth, {} AS curr_name \
             FROM {} \
             WHERE {} = '{}'",
            Self::id_column(),
            Self::name_column(),
            Self::table_name(),
            Self::id_column(),
            root_id
        )
    }
}

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

        let rows: Vec<TursoHierarchyRow> = diesel_async::RunQueryDsl::load(diesel::sql_query(sql), conn).await?;
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

        let rows: Vec<TursoHierarchyRow> = diesel_async::RunQueryDsl::load(diesel::sql_query(sql), conn).await?;
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
