//! The INSERT, DELETE and UPDATE statements of a model, per SQL dialect.

/// Placeholder style of a backend.
#[derive(Clone, Copy)]
pub enum Dialect {
    Sqlite,
    Postgres,
}

impl Dialect {
    /// The `n`-th (1-based) placeholder.
    fn placeholder(self, n: usize) -> String {
        match self {
            Self::Sqlite => "?".to_string(),
            Self::Postgres => format!("${n}"),
        }
    }
}

/// The three statements one backend gets.
pub struct Statements {
    pub insert: String,
    pub delete: String,
    pub update: String,
}

/// Positions of the columns that are not the id.
pub fn non_id_indices(names: &[String], id_col: &str) -> Vec<usize> {
    (0..names.len()).filter(|&i| names[i] != id_col).collect()
}

pub fn statements(dialect: Dialect, table: &str, names: &[String], id_col: &str) -> Statements {
    let cols = names.join(", ");
    let marks = joined((1..=names.len()).map(|i| dialect.placeholder(i)));
    let non_id = non_id_indices(names, id_col);
    let sets = joined(
        non_id
            .iter()
            .enumerate()
            .map(|(seq, &i)| format!("{} = {}", names[i], dialect.placeholder(seq + 1))),
    );
    Statements {
        insert: format!("INSERT INTO {table} ({cols}) VALUES ({marks});"),
        delete: format!(
            "DELETE FROM {table} WHERE {id_col} = {};",
            dialect.placeholder(1)
        ),
        update: format!(
            "UPDATE {table} SET {sets} WHERE {id_col} = {};",
            dialect.placeholder(non_id.len() + 1)
        ),
    }
}

fn joined(parts: impl Iterator<Item = String>) -> String {
    parts.collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::{fixture, rstest};

    #[fixture]
    fn names() -> Vec<String> {
        ["id", "name", "parent_id"].map(String::from).to_vec()
    }

    #[rstest]
    #[case::sqlite(
        Dialect::Sqlite,
        "INSERT INTO t (id, name, parent_id) VALUES (?, ?, ?);",
        "DELETE FROM t WHERE id = ?;",
        "UPDATE t SET name = ?, parent_id = ? WHERE id = ?;"
    )]
    #[case::postgres(
        Dialect::Postgres,
        "INSERT INTO t (id, name, parent_id) VALUES ($1, $2, $3);",
        "DELETE FROM t WHERE id = $1;",
        "UPDATE t SET name = $1, parent_id = $2 WHERE id = $3;"
    )]
    fn statements_per_dialect(
        names: Vec<String>,
        #[case] dialect: Dialect,
        #[case] insert: &str,
        #[case] delete: &str,
        #[case] update: &str,
    ) {
        let s = statements(dialect, "t", &names, "id");
        assert_eq!(
            (s.insert.as_str(), s.delete.as_str(), s.update.as_str()),
            (insert, delete, update)
        );
    }
}
