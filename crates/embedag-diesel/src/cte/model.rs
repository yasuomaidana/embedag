#[cfg(feature = "sqlite")]
use diesel::sqlite::SqliteConnection;

use super::builder::CteHierarchyBuilder;
use super::config::{HierarchicalTableConfig, HierarchyItem};

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
