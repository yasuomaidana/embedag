//! The SQL text of the recursive CTE traversals, shared by every backend.

use super::config::HierarchicalTableConfig;

/// How a backend spells the keys it compares and returns: SQLite compares them as stored,
/// PostgreSQL casts every key column to `text` first.
#[derive(Clone, Copy)]
pub(super) enum Dialect {
    /// Keys are used as stored (SQLite, Turso / libSQL).
    Plain,
    /// Keys are cast with `::text` (PostgreSQL).
    TextCast,
}

impl Dialect {
    fn key(self, expr: &str) -> String {
        match self {
            Self::Plain => expr.to_string(),
            Self::TextCast => format!("{expr}::text"),
        }
    }
}

/// The three parts of a recursive CTE: the seed (anchor), the recursive step and the final query.
pub(super) struct CteSql {
    pub(super) seed: String,
    pub(super) step: String,
    pub(super) body: String,
}

impl CteSql {
    /// The complete `WITH RECURSIVE` statement, for backends that run raw SQL.
    #[cfg(feature = "turso-async")]
    pub(super) fn statement(&self, name: &str, columns: &[&str]) -> String {
        format!(
            "WITH RECURSIVE {name}({}) AS ({} UNION ALL {}) {};",
            columns.join(", "),
            self.seed,
            self.step,
            self.body
        )
    }
}

/// Name of the ancestors CTE and its columns.
pub(super) const ANCESTORS: &str = "ancestors_cte";
pub(super) const ANCESTOR_COLUMNS: [&str; 4] = ["curr_id", "parent_ref", "depth", "curr_name"];
/// Name of the descendants CTE and its columns.
pub(super) const DESCENDANTS: &str = "descendants_cte";
pub(super) const DESCENDANT_COLUMNS: [&str; 3] = ["curr_id", "depth", "curr_name"];

/// Walks up from `target_id` through the parent column, `max_depth` levels at most.
pub(super) fn ancestors(
    dialect: Dialect,
    config: &HierarchicalTableConfig,
    target_id: &str,
    max_depth: i32,
) -> CteSql {
    let table = &config.table_name;
    let id = dialect.key(&config.id_column);
    let parent = dialect.key(&config.parent_column);
    let name = dialect.key(&config.name_column);
    let (id_col, parent_col, name_col) = (&id, &parent, &name);
    let (t_id, t_parent, t_name) = (
        dialect.key(&format!("t.{}", config.id_column)),
        dialect.key(&format!("t.{}", config.parent_column)),
        dialect.key(&format!("t.{}", config.name_column)),
    );
    CteSql {
        seed: format!(
            "SELECT {id_col} AS curr_id, {parent_col} AS parent_ref, 0 AS depth, {name_col} AS curr_name \
             FROM {table} \
             WHERE {id_col} = '{target_id}'"
        ),
        step: format!(
            "SELECT {t_id}, {t_parent}, a.depth + 1, {t_name} \
             FROM {table} t \
             INNER JOIN {ANCESTORS} a ON {t_id} = a.parent_ref \
             WHERE a.depth < {max_depth}"
        ),
        body: format!(
            "SELECT curr_id, depth, curr_name FROM {ANCESTORS} WHERE depth > 0 ORDER BY depth ASC"
        ),
    }
}

/// Walks down from `root_id` through the parent column, `max_depth` levels at most and, when the
/// table sets one, with at most `max_breadth_per_node` children per node.
pub(super) fn descendants(
    dialect: Dialect,
    config: &HierarchicalTableConfig,
    root_id: &str,
    max_depth: i32,
) -> CteSql {
    let table = &config.table_name;
    let (raw_id, raw_parent, raw_name) = (
        &config.id_column,
        &config.parent_column,
        &config.name_column,
    );
    let id_col = dialect.key(raw_id);
    let name_col = dialect.key(raw_name);
    let t_id = dialect.key(&format!("t.{raw_id}"));
    let t_name = dialect.key(&format!("t.{raw_name}"));
    let t_parent = dialect.key(&format!("t.{raw_parent}"));
    let (source, limit) = match config.max_breadth_per_node {
        Some(breadth) => (
            format!(
                "( \
                     SELECT {raw_id}, {raw_parent}, {raw_name}, \
                            ROW_NUMBER() OVER (PARTITION BY {raw_parent} ORDER BY {raw_name}) as rn \
                     FROM {table} \
                 ) t"
            ),
            format!(" AND t.rn <= {breadth}"),
        ),
        None => (format!("{table} t"), String::new()),
    };
    CteSql {
        seed: format!(
            "SELECT {id_col} AS curr_id, 0 AS depth, {name_col} AS curr_name \
             FROM {table} \
             WHERE {id_col} = '{root_id}'"
        ),
        step: format!(
            "SELECT {t_id}, d.depth + 1, {t_name} \
             FROM {source} \
             INNER JOIN {DESCENDANTS} d ON {t_parent} = d.curr_id \
             WHERE d.depth < {max_depth}{limit}"
        ),
        body: format!(
            "SELECT curr_id, depth, curr_name FROM {DESCENDANTS} WHERE depth > 0 ORDER BY depth ASC"
        ),
    }
}
