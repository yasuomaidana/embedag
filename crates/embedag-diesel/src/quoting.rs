//! Escaping for the few places SQL text is built by hand, where a value cannot be bound as a
//! parameter: the seed row of a recursive CTE and the name of a savepoint.

/// Escapes a value for a single-quoted SQL string literal.
pub(crate) fn sql_literal(value: &str) -> String {
    value.replace('\'', "''")
}

/// Quotes a SQL identifier (a savepoint name cannot be bound as a parameter).
#[cfg(any(feature = "postgres-async", feature = "turso-async"))]
pub(crate) fn quoted_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

/// Quotes a table or column name taken from configuration, so a reserved word, a space or a quote
/// in it cannot break the statement or add to it. A schema-qualified name (`schema.table`) is
/// quoted part by part.
///
/// A plain name (`[A-Za-z_][A-Za-z0-9_]*`) is quoted in lower case: PostgreSQL folds an unquoted
/// name to lower case and SQLite ignores case, so `Orders` and `"orders"` name the same table as
/// before. Any other name is quoted exactly as written.
#[cfg_attr(
    not(any(
        feature = "sqlite",
        all(feature = "postgres-async", feature = "diesel-async")
    )),
    allow(dead_code)
)]
pub(crate) fn identifier_path(value: &str) -> String {
    value
        .split('.')
        .map(|part| {
            let plain = part
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            let text = if plain {
                part.to_ascii_lowercase()
            } else {
                part.to_string()
            };
            format!("\"{}\"", text.replace('"', "\"\""))
        })
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::literal_plain(sql_literal, "root", "root")]
    #[case::literal_quote(sql_literal, "o'brien", "o''brien")]
    #[case::literal_injection(sql_literal, "x' OR '1'='1", "x'' OR ''1''=''1")]
    #[cfg_attr(
        any(feature = "postgres-async", feature = "turso-async"),
        case::identifier_plain(quoted_identifier, "sp_1", "\"sp_1\"")
    )]
    #[cfg_attr(
        any(feature = "postgres-async", feature = "turso-async"),
        case::identifier_quote(quoted_identifier, "a\"b", "\"a\"\"b\"")
    )]
    #[case::path_plain(identifier_path, "semantic_nodes", "\"semantic_nodes\"")]
    #[case::path_folds_case(identifier_path, "Orders", "\"orders\"")]
    #[case::path_reserved_word(identifier_path, "order", "\"order\"")]
    #[case::path_schema(identifier_path, "public.Nodes", "\"public\".\"nodes\"")]
    #[case::path_space_kept_exact(identifier_path, "My Table", "\"My Table\"")]
    #[case::path_injection(
        identifier_path,
        "t\"; DROP TABLE x; --",
        "\"t\"\"; DROP TABLE x; --\""
    )]
    fn escapes(#[case] f: fn(&str) -> String, #[case] input: &str, #[case] want: &str) {
        assert_eq!(f(input), want);
    }
}
