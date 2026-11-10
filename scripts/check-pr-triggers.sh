#!/bin/sh
# Every pull request into main runs every workflow: fails when a workflow has no pull_request trigger, when
# its pull_request trigger filters on branches, branches-ignore, paths or paths-ignore, or when a job has a
# job-level `if:` that can skip it. Step-level `if:` (cache hits, OS-specific steps) is fine.
#
# Usage: sh scripts/check-pr-triggers.sh [workflow-dir]   (default .github/workflows)
set -eu

dir="${1:-.github/workflows}"
status=0

for file in "$dir"/*.yml "$dir"/*.yaml; do
  [ -f "$file" ] || continue
  problems=$(awk '
    /^on:/ { in_on = 1; next }
    /^[A-Za-z_-]+:/ { in_on = 0; in_jobs = ($1 == "jobs:") }
    in_on && /^  pull_request:/ { has_pr = 1; in_pr = 1; next }
    in_on && /^  [A-Za-z_-]+:/ { in_pr = 0 }
    in_on && in_pr && /^    (branches|branches-ignore|paths|paths-ignore):/ {
      sub(/^ +/, ""); sub(/:.*/, ""); print "pull_request filters on " $0
    }
    in_jobs && /^    if:/ { print "a job has a job-level if: " $0 }
    END { if (!has_pr) print "no pull_request trigger" }
  ' "$file")
  if [ -n "$problems" ]; then
    printf '%s\n' "$problems" | while IFS= read -r p; do echo "pr-triggers: $file: $p" >&2; done
    status=1
  fi
done

[ "$status" -eq 0 ] && echo "pr-triggers: every workflow runs on every pull request"
exit "$status"
