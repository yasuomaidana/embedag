//! Recursive CTE query builder for compile-time safe dialect-aware graph traversals.

use super::config::{HierarchicalTableConfig, HierarchyItem};
#[cfg(any(
    feature = "sqlite",
    all(feature = "postgres-async", feature = "diesel-async"),
    feature = "turso-async"
))]
use super::model::HierarchicalModel;
use super::sql as cte_sql;
#[cfg(any(
    feature = "sqlite",
    all(feature = "postgres-async", feature = "diesel-async"),
    feature = "turso-async"
))]
use super::sql::Dialect;
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

#[cfg(any(
    feature = "sqlite",
    all(feature = "postgres-async", feature = "diesel-async")
))]
type Parts<Seed> = RecursiveParts<
    diesel::expression::SqlLiteral<Seed>,
    diesel::expression::SqlLiteral<Seed>,
    diesel::expression::SqlLiteral<(Text, Integer, Text)>,
>;

/// The seed, step and body of `cte` as typed SQL; `Seed` is the column set of the seed and step.
#[cfg(any(
    feature = "sqlite",
    all(feature = "postgres-async", feature = "diesel-async")
))]
fn parts<Seed: diesel::expression::TypedExpressionType>(cte: &cte_sql::CteSql) -> Parts<Seed> {
    RecursiveParts::new(sql(&cte.seed), sql(&cte.step), sql(&cte.body))
}

/// The rows a traversal returns as hierarchy items.
#[cfg(any(
    feature = "sqlite",
    all(feature = "postgres-async", feature = "diesel-async")
))]
fn items(rows: Vec<(String, i32, String)>) -> Vec<HierarchyItem> {
    rows.into_iter()
        .map(|(id, depth, name)| HierarchyItem { id, depth, name })
        .collect()
}

/// Defines, for each entry, the type-safe `fetch_model_*` wrapper that runs the builder function
/// `$inner` with the model's table configuration; `async` entries are awaited.
macro_rules! model_traversals {
    ($(
        $(#[$meta:meta])*
        $name:ident($conn:ty, $id:ident) => $inner:ident $($async:ident)?;
    )*) => {$(
        $(#[$meta])*
        pub $($async)? fn $name<M: HierarchicalModel>(
            conn: &mut $conn,
            $id: &str,
            max_depth: i32,
        ) -> diesel::QueryResult<Vec<HierarchyItem>> {
            let config = M::table_config();
            let items = Self::$inner(conn, &config, $id, max_depth);
            model_traversals!(@wait items $($async)?)
        }
    )*};
    (@wait $items:ident async) => { $items.await };
    (@wait $items:ident) => { $items };
}

/// Defines the Turso / libSQL traversals: each builds the CTE with `cte_sql::$cte` and runs it as
/// one `WITH RECURSIVE` statement.
#[cfg(feature = "turso-async")]
macro_rules! turso_traversals {
    ($(
        $(#[$meta:meta])*
        $name:ident($id:ident) => $cte:ident, $cte_name:ident, $columns:ident;
    )*) => {$(
        $(#[$meta])*
        pub async fn $name(
            conn: &mut crate::migrations::AsyncLibSqlConnection,
            config: &HierarchicalTableConfig,
            $id: &str,
            max_depth: i32,
        ) -> diesel::QueryResult<Vec<HierarchyItem>> {
            let cte = cte_sql::$cte(Dialect::Plain, config, $id, max_depth);
            let statement = cte.statement(cte_sql::$cte_name, &cte_sql::$columns);
            Self::load_turso(conn, statement).await
        }
    )*};
}

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
        let cte = cte_sql::ancestors(Dialect::Plain, config, target_id, max_depth);
        Self::load_sqlite::<(Text, Text, Integer, Text)>(
            conn,
            &cte,
            cte_sql::ANCESTORS,
            &cte_sql::ANCESTOR_COLUMNS,
        )
    }

    /// Traverses ancestors with automatic set deduplication (UNION) via `diesel_cte_ext::with_recursive_not_all`.
    #[cfg(feature = "sqlite")]
    pub fn fetch_ancestors_dedup_sqlite(
        conn: &mut SqliteConnection,
        config: &HierarchicalTableConfig,
        target_id: &str,
        max_depth: i32,
    ) -> diesel::QueryResult<Vec<HierarchyItem>> {
        let cte = cte_sql::ancestors(Dialect::Plain, config, target_id, max_depth);
        let rows = conn
            .with_recursive_not_all(
                cte_sql::ANCESTORS,
                &cte_sql::ANCESTOR_COLUMNS,
                parts::<(Text, Text, Integer, Text)>(&cte),
            )
            .load::<(String, i32, String)>(conn)?;
        Ok(items(rows))
    }

    /// Traverses descendants of `root_id` down to `max_depth` on an SQLite connection.
    #[cfg(feature = "sqlite")]
    pub fn fetch_descendants_sqlite(
        conn: &mut SqliteConnection,
        config: &HierarchicalTableConfig,
        root_id: &str,
        max_depth: i32,
    ) -> diesel::QueryResult<Vec<HierarchyItem>> {
        let cte = cte_sql::descendants(Dialect::Plain, config, root_id, max_depth);
        Self::load_sqlite::<(Text, Integer, Text)>(
            conn,
            &cte,
            cte_sql::DESCENDANTS,
            &cte_sql::DESCENDANT_COLUMNS,
        )
    }

    /// Runs the recursive CTE `cte` named `name` over `columns` on SQLite and reads its rows.
    #[cfg(feature = "sqlite")]
    fn load_sqlite<Seed: diesel::expression::TypedExpressionType + 'static>(
        conn: &mut SqliteConnection,
        cte: &cte_sql::CteSql,
        name: &'static str,
        columns: &'static [&'static str],
    ) -> diesel::QueryResult<Vec<HierarchyItem>> {
        let rows = conn
            .with_recursive(name, columns, parts::<Seed>(cte))
            .load::<(String, i32, String)>(conn)?;
        Ok(items(rows))
    }

    /// Traverses ancestors of `target_id` up to `max_depth` asynchronously on PostgreSQL via `diesel-async`.
    #[cfg(all(feature = "postgres-async", feature = "diesel-async"))]
    pub async fn fetch_ancestors_pg_async(
        conn: &mut diesel_async::AsyncPgConnection,
        config: &HierarchicalTableConfig,
        target_id: &str,
        max_depth: i32,
    ) -> diesel::QueryResult<Vec<HierarchyItem>> {
        let cte = cte_sql::ancestors(Dialect::TextCast, config, target_id, max_depth);
        Self::load_pg::<(Text, Text, Integer, Text)>(
            conn,
            &cte,
            cte_sql::ANCESTORS,
            &cte_sql::ANCESTOR_COLUMNS,
        )
        .await
    }

    /// Traverses descendants of `root_id` down to `max_depth` asynchronously on PostgreSQL via `diesel-async`.
    #[cfg(all(feature = "postgres-async", feature = "diesel-async"))]
    pub async fn fetch_descendants_pg_async(
        conn: &mut diesel_async::AsyncPgConnection,
        config: &HierarchicalTableConfig,
        root_id: &str,
        max_depth: i32,
    ) -> diesel::QueryResult<Vec<HierarchyItem>> {
        let cte = cte_sql::descendants(Dialect::TextCast, config, root_id, max_depth);
        Self::load_pg::<(Text, Integer, Text)>(
            conn,
            &cte,
            cte_sql::DESCENDANTS,
            &cte_sql::DESCENDANT_COLUMNS,
        )
        .await
    }

    /// Runs the recursive CTE `cte` named `name` over `columns` on PostgreSQL and reads its rows.
    #[cfg(all(feature = "postgres-async", feature = "diesel-async"))]
    async fn load_pg<Seed: diesel::expression::TypedExpressionType + Send + Sync + 'static>(
        conn: &mut diesel_async::AsyncPgConnection,
        cte: &cte_sql::CteSql,
        name: &'static str,
        columns: &'static [&'static str],
    ) -> diesel::QueryResult<Vec<HierarchyItem>> {
        let query = conn.with_recursive(name, columns, parts::<Seed>(cte));
        let rows: Vec<(String, i32, String)> = diesel_async::RunQueryDsl::load(query, conn).await?;
        Ok(items(rows))
    }

    #[cfg(feature = "turso-async")]
    turso_traversals! {
        /// Traverses ancestors of `target_id` up to `max_depth` asynchronously on Turso / libSQL via `diesel-async`.
        fetch_ancestors_turso_async(target_id) => ancestors, ANCESTORS, ANCESTOR_COLUMNS;
        /// Traverses descendants of `root_id` down to `max_depth` asynchronously on Turso / libSQL via `diesel-async`.
        fetch_descendants_turso_async(root_id) => descendants, DESCENDANTS, DESCENDANT_COLUMNS;
    }

    model_traversals! {
        /// Type-safe model ancestor traversal for SQLite.
        #[cfg(feature = "sqlite")]
        fetch_model_ancestors_sqlite(SqliteConnection, target_id) => fetch_ancestors_sqlite;
        /// Type-safe model descendant traversal for SQLite.
        #[cfg(feature = "sqlite")]
        fetch_model_descendants_sqlite(SqliteConnection, root_id) => fetch_descendants_sqlite;
        /// Type-safe model ancestor deduplication traversal for SQLite.
        #[cfg(feature = "sqlite")]
        fetch_model_ancestors_dedup_sqlite(SqliteConnection, target_id) => fetch_ancestors_dedup_sqlite;
        /// Type-safe model ancestor traversal for PostgreSQL async.
        #[cfg(all(feature = "postgres-async", feature = "diesel-async"))]
        fetch_model_ancestors_pg_async(diesel_async::AsyncPgConnection, target_id) => fetch_ancestors_pg_async async;
        /// Type-safe model descendant traversal for PostgreSQL async.
        #[cfg(all(feature = "postgres-async", feature = "diesel-async"))]
        fetch_model_descendants_pg_async(diesel_async::AsyncPgConnection, root_id) => fetch_descendants_pg_async async;
        /// Type-safe model ancestor traversal for Turso / libSQL async.
        #[cfg(feature = "turso-async")]
        fetch_model_ancestors_turso_async(crate::migrations::AsyncLibSqlConnection, target_id) => fetch_ancestors_turso_async async;
        /// Type-safe model descendant traversal for Turso / libSQL async.
        #[cfg(feature = "turso-async")]
        fetch_model_descendants_turso_async(crate::migrations::AsyncLibSqlConnection, root_id) => fetch_descendants_turso_async async;
    }

    /// Runs a complete CTE statement on Turso / libSQL and maps its rows to hierarchy items.
    #[cfg(feature = "turso-async")]
    async fn load_turso(
        conn: &mut crate::migrations::AsyncLibSqlConnection,
        statement: String,
    ) -> diesel::QueryResult<Vec<HierarchyItem>> {
        let rows: Vec<TursoHierarchyRow> =
            diesel_async::RunQueryDsl::load(diesel::sql_query(statement), conn).await?;
        Ok(rows
            .into_iter()
            .map(|r| HierarchyItem {
                id: r.curr_id,
                depth: r.depth,
                name: r.curr_name,
            })
            .collect())
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
