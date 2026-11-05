#!/usr/bin/env bash
# Fails when an advisory ignored in .cargo/audit.toml is no longer reported for Cargo.lock, so the ignore list can
# only shrink: drop the entry it names. Issue #2 explains why the remaining ones are ignored.
#
# It also fails, with a different message, when `cargo audit` itself cannot run (advisory database fetch, network):
# an audit that did not run says nothing about the ignores, so no ignore is called stale in that case.
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

# `cargo audit` exits 1 when it reports vulnerabilities, so the exit status alone does not tell a finding from a
# failure. A run that worked prints a JSON report with a `vulnerabilities` object; anything else did not run.
if ! report=$(cd "$workdir" && cargo audit --json 2>"$workdir/stderr"); then
  :
fi
if ! printf '%s' "$report" | jq -e '.vulnerabilities | type == "object"' >/dev/null 2>&1; then
  echo "cargo audit did not produce a report, so the ignored advisories could not be checked:" >&2
  cat "$workdir/stderr" >&2
  exit 2
fi

# The advisories reported as vulnerabilities or as warnings (unmaintained, unsound). Advisories that only appear
# under `related` of another one are not reported for this lockfile and do not count.
reported=$(printf '%s' "$report" | jq -r '
  [ (.vulnerabilities.list // [])[].advisory.id,
    ((.warnings // {}) | to_entries[] | .value[] | .advisory.id // empty) ]
  | unique | .[]')

stale=$(comm -23 <(printf '%s\n' "$ignored") <(printf '%s\n' "$reported") || true)
if [ -n "$stale" ]; then
  echo "Ignored in $config but no longer reported for Cargo.lock; remove them:" >&2
  printf '  %s\n' $stale >&2
  exit 1
fi
echo "All $(printf '%s\n' "$ignored" | wc -l | tr -d ' ') ignored advisories still apply."
