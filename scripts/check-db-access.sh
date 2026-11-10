#!/bin/sh
# SQL is reached only through Diesel (diesel, diesel-async, diesel-libsql, diesel-cte-ext).
#
# Fails when:
#   1. a Cargo.toml declares sqlx, rusqlite, sea-orm, mysql, mysql_async, or the bare postgres,
#      tokio-postgres or libsql crates without declaring diesel in the same manifest;
#   2. a Rust source uses rusqlite::, sqlx::, sea_orm::, tokio_postgres:: or libsql:: paths;
#   3. a Rust source calls sql_query outside a module named `raw` (a `raw` directory or raw.rs).
#
# Exceptions live in .github/db-access-allow.txt, one per line: `<path prefix> <reason>`. A reason is
# required, so every exception explains itself and names the ticket that removes it.
#
# Usage: sh scripts/check-db-access.sh [root]   (root defaults to the current directory)
set -eu

root="${1:-.}"
allow_file="$root/.github/db-access-allow.txt"
status=0
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

# Allowed path prefixes (first field of each non-comment line); a line without a reason is an error.
: > "$tmp/allow"
if [ -f "$allow_file" ]; then
  while IFS= read -r line; do
    case "$line" in ''|'#'*) continue ;; esac
    prefix=${line%% *}
    reason=${line#"$prefix"}
    reason=$(printf '%s' "$reason" | tr -d '[:space:]')
    if [ -z "$reason" ]; then
      echo "db-access: $allow_file: '$prefix' has no reason" >&2
      status=1
      continue
    fi
    printf '%s\n' "$prefix" >> "$tmp/allow"
  done < "$allow_file"
fi

allowed() { # $1 = path relative to root
  while IFS= read -r prefix; do
    case "$1" in "$prefix"*) return 0 ;; esac
  done < "$tmp/allow"
  return 1
}

fail() {
  echo "db-access: $1" >&2
  status=1
}

find "$root" \( -name target -o -name .git -o -name node_modules \) -prune -o -type f \
  \( -name Cargo.toml -o -name '*.rs' \) -print | sort > "$tmp/files"

while IFS= read -r file; do
  rel=${file#"$root"/}
  case "$file" in
    */Cargo.toml)
      # Dependency names declared in [dependencies], [dev-dependencies], [build-dependencies],
      # [workspace.dependencies], [target.*.dependencies] and [dependencies.<name>] tables.
      awk '
        /^[[:space:]]*\[/ {
          sect = $0
          gsub(/^[[:space:]]*\[|\][[:space:]]*$/, "", sect)
          in_deps = (sect ~ /(^|\.)(dev-|build-)?dependencies(\.|$)/)
          if (in_deps && sect ~ /dependencies\.[A-Za-z0-9_-]+$/) {
            name = sect; sub(/.*dependencies\./, "", name); print name
          }
          next
        }
        in_deps && /^[[:space:]]*[A-Za-z0-9_-]+[[:space:]]*(=|\.)/ {
          name = $0; sub(/^[[:space:]]*/, "", name); sub(/[[:space:]]*(=|\.).*/, "", name); print name
        }
      ' "$file" | sort -u > "$tmp/deps"
      has_diesel=no
      grep -qx 'diesel' "$tmp/deps" && has_diesel=yes
      for banned in sqlx rusqlite sea-orm mysql mysql_async postgres tokio-postgres libsql; do
        if grep -qx "$banned" "$tmp/deps" && [ "$has_diesel" = no ] && ! allowed "$rel"; then
          fail "$rel declares '$banned' without diesel: use Diesel for database access"
        fi
      done
      if grep -q '^sqlx-' "$tmp/deps" && [ "$has_diesel" = no ] && ! allowed "$rel"; then
        fail "$rel declares an sqlx crate without diesel: use Diesel for database access"
      fi
      ;;
    *.rs)
      allowed "$rel" && continue
      # Comment lines are ignored; a path such as diesel_libsql:: is not libsql::.
      grep -nE '(^|[^A-Za-z0-9_])(rusqlite|sqlx|sea_orm|tokio_postgres|libsql)::' "$file" 2>/dev/null \
        | grep -vE '^[0-9]+:[[:space:]]*//' > "$tmp/hits" || true
      if [ -s "$tmp/hits" ]; then
        while IFS= read -r hit; do fail "$rel:$hit: raw database driver outside Diesel"; done < "$tmp/hits"
      fi
      case "$rel" in
        */raw/*|*/raw.rs|raw/*|raw.rs) ;;
        *)
          grep -nE '(^|[^A-Za-z0-9_])sql_query[[:space:]]*\(' "$file" 2>/dev/null \
            | grep -vE '^[0-9]+:[[:space:]]*//' > "$tmp/hits" || true
          if [ -s "$tmp/hits" ]; then
            while IFS= read -r hit; do fail "$rel:$hit: sql_query outside a module named raw"; done < "$tmp/hits"
          fi
          ;;
      esac
      ;;
  esac
done < "$tmp/files"

if [ "$status" -eq 0 ]; then
  echo "db-access: database access goes through Diesel only"
fi
exit "$status"
