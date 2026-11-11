#!/bin/sh
# Tests for scripts/check-pr-triggers.sh.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
guard="$here/check-pr-triggers.sh"
base=$(mktemp -d)
trap 'rm -rf "$base"' EXIT HUP INT TERM
failures=0

expect() { # <name> <pass|fail> <workflow text>
  mkdir -p "$base/case"; rm -f "$base/case"/*.yml
  printf '%s\n' "$3" > "$base/case/w.yml"
  if sh "$guard" "$base/case" >/dev/null 2>&1; then got=pass; else got=fail; fi
  if [ "$got" = "$2" ]; then echo "ok   $1"; else echo "FAIL $1 (expected $2, got $got)"; failures=$((failures + 1)); fi
}

expect "unfiltered pull_request passes" pass 'name: ok
on:
  push:
    branches: [ main ]
  pull_request:
jobs:
  a:
    runs-on: ubuntu-latest
    steps:
      - if: runner.os == '"'"'macOS'"'"'
        run: true'
expect "branch filter fails" fail 'name: x
on:
  pull_request:
    branches: [ main ]
jobs:
  a:
    runs-on: ubuntu-latest'
expect "path filter fails" fail 'name: x
on:
  pull_request:
    paths: [ "Cargo.lock" ]
jobs:
  a:
    runs-on: ubuntu-latest'
expect "push-only workflow fails" fail 'name: x
on:
  push:
    branches: [ main ]
jobs:
  a:
    runs-on: ubuntu-latest'
expect "job-level if fails" fail 'name: x
on:
  pull_request:
jobs:
  a:
    if: github.actor != '"'"'bot'"'"'
    runs-on: ubuntu-latest'
expect "a path filter on push only passes" pass 'name: x
on:
  push:
    paths: [ "Cargo.lock" ]
  pull_request:
jobs:
  a:
    runs-on: ubuntu-latest'

[ "$failures" -eq 0 ] && echo "all pr-trigger cases passed" || { echo "$failures case(s) failed"; exit 1; }
