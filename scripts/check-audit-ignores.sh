#!/usr/bin/env bash
# Fails when an advisory ignored in .cargo/audit.toml is no longer reported for Cargo.lock, so the ignore list can
# only shrink: drop the entry it names. Issue #2 explains why the remaining ones are ignored.
set -euo pipefail

config=".cargo/audit.toml"
ignored=$(grep -oE 'RUSTSEC-[0-9]{4}-[0-9]{4}' "$config" | sort -u || true)
if [ -z "$ignored" ]; then
  echo "No ignored advisories configured; audit check passed."
  exit 0
fi

# Audit the lockfile without the ignore list (no .cargo/audit.toml next to the copy).
workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT
cp Cargo.lock "$workdir/Cargo.lock"
reported=$(cd "$workdir" && { cargo audit --json 2>/dev/null || true; } | grep -oE 'RUSTSEC-[0-9]{4}-[0-9]{4}' | sort -u || true)

stale=$(comm -23 <(printf '%s\n' "$ignored") <(printf '%s\n' "$reported") || true)
if [ -n "$stale" ]; then
  echo "Ignored in $config but no longer reported for Cargo.lock; remove them:" >&2
  printf '  %s\n' $stale >&2
  exit 1
fi
echo "All $(printf '%s\n' "$ignored" | wc -l | tr -d ' ') ignored advisories still apply."
