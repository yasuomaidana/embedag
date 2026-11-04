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
    fn escapes(#[case] f: fn(&str) -> String, #[case] input: &str, #[case] want: &str) {
        assert_eq!(f(input), want);
    }
}
