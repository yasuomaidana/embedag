#!/bin/sh
# Tests for scripts/check-db-access.sh: each case builds a tiny workspace and checks the exit status.
# Usage: sh scripts/test-check-db-access.sh
set -eu

here=$(cd "$(dirname "$0")" && pwd)
guard="$here/check-db-access.sh"
base=$(mktemp -d)
trap 'rm -rf "$base"' EXIT HUP INT TERM
failures=0

# new_case <name>: makes an empty case directory and prints its path.
new_case() { mkdir -p "$base/$1/src" "$base/$1/.github"; printf '%s' "$base/$1"; }

expect() { # <name> <pass|fail> <dir>
  if sh "$guard" "$3" >/dev/null 2>&1; then got=pass; else got=fail; fi
  if [ "$got" = "$2" ]; then echo "ok   $1"; else echo "FAIL $1 (expected $2, got $got)"; failures=$((failures + 1)); fi
}

d=$(new_case diesel_only)
printf '[dependencies]\ndiesel = "2"\ndiesel-async = { version = "0.9", features = ["postgres"] }\ndiesel-libsql = "0.1"\n' > "$d/Cargo.toml"
printf 'fn main() { let _ = diesel_libsql::LibSqlConnection::establish; }\n' > "$d/src/main.rs"
expect "diesel and its backends pass" pass "$d"

d=$(new_case sqlx_manifest)
printf '[dependencies]\nsqlx = "0.8"\n' > "$d/Cargo.toml"
expect "sqlx without diesel fails" fail "$d"

d=$(new_case rusqlite_table)
printf '[dependencies.rusqlite]\nversion = "0.32"\n' > "$d/Cargo.toml"
expect "rusqlite dependency table fails" fail "$d"

d=$(new_case tokio_pg_with_diesel)
printf '[dependencies]\ndiesel = "2"\ntokio-postgres = "0.7"\n' > "$d/Cargo.toml"
printf 'fn main() {}\n' > "$d/src/main.rs"
expect "tokio-postgres declared next to diesel passes the manifest rule" pass "$d"

d=$(new_case tokio_pg_source)
printf '[dependencies]\ndiesel = "2"\ntokio-postgres = "0.7"\n' > "$d/Cargo.toml"
printf 'fn f(c: &tokio_postgres::Client) {}\n' > "$d/src/lib.rs"
expect "tokio_postgres path in source fails" fail "$d"

d=$(new_case comment_only)
printf '[dependencies]\ndiesel = "2"\n' > "$d/Cargo.toml"
printf '// replaces tokio_postgres::Client\nfn main() {}\n' > "$d/src/main.rs"
expect "a comment mentioning a driver passes" pass "$d"

d=$(new_case sql_query_outside_raw)
printf '[dependencies]\ndiesel = "2"\n' > "$d/Cargo.toml"
printf 'fn f() { diesel::sql_query("select 1"); }\n' > "$d/src/lib.rs"
expect "sql_query outside raw fails" fail "$d"

d=$(new_case sql_query_in_raw)
mkdir -p "$d/src/raw"
printf '[dependencies]\ndiesel = "2"\n' > "$d/Cargo.toml"
printf 'pub fn f() { diesel::sql_query("select 1"); }\n' > "$d/src/raw/mod.rs"
expect "sql_query inside a raw module passes" pass "$d"

d=$(new_case allowed_with_reason)
printf '[dependencies]\ndiesel = "2"\n' > "$d/Cargo.toml"
printf 'fn f(c: &tokio_postgres::Client) {}\n' > "$d/src/legacy.rs"
printf 'src/legacy.rs EMBEDAG-1: port to diesel-async\n' > "$d/.github/db-access-allow.txt"
expect "an allow-listed file with a reason passes" pass "$d"

d=$(new_case allowed_without_reason)
printf '[dependencies]\ndiesel = "2"\n' > "$d/Cargo.toml"
printf 'fn f(c: &tokio_postgres::Client) {}\n' > "$d/src/legacy.rs"
printf 'src/legacy.rs\n' > "$d/.github/db-access-allow.txt"
expect "an allow-list entry without a reason fails" fail "$d"

d=$(new_case target_ignored)
mkdir -p "$d/target/debug"
printf '[dependencies]\ndiesel = "2"\n' > "$d/Cargo.toml"
printf 'fn f(c: &rusqlite::Connection) {}\n' > "$d/target/debug/gen.rs"
expect "build output under target/ is ignored" pass "$d"

[ "$failures" -eq 0 ] && echo "all db-access cases passed" || { echo "$failures db-access case(s) failed"; exit 1; }
