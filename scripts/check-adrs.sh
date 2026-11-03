#!/usr/bin/env bash
# Checks that every ADR follows the template and appears in the index.
set -euo pipefail

dir="$(cd "$(dirname "$0")/.." && pwd)/docs/adr"
index="$dir/README.md"
sections=("## Context" "## Decision" "## Alternatives considered" "## Consequences" "## Sources")
errors=0
expected=1

fail() {
  echo "error: $1" >&2
  errors=$((errors + 1))
}

for file in "$dir"/[0-9][0-9][0-9][0-9]-*.md; do
  name="$(basename "$file")"
  number="${name:0:4}"

  if ((10#$number != expected)); then
    fail "$name: expected ADR number $(printf '%04d' "$expected")"
  fi
  expected=$((10#$number + 1))

  if ! head -n 1 "$file" | grep -Eq "^# $number\. .+"; then
    fail "$name: first line must be '# $number. Title'"
  fi
  if ! grep -Eq '^- Status: (Proposed|Accepted|Deprecated|Superseded by [0-9]{4})$' "$file"; then
    fail "$name: missing or invalid '- Status:' line"
  fi
  if ! grep -Eq '^- Date: [0-9]{4}-[0-9]{2}-[0-9]{2}$' "$file"; then
    fail "$name: missing or invalid '- Date: YYYY-MM-DD' line"
  fi
  for section in "${sections[@]}"; do
    grep -qx "$section" "$file" || fail "$name: missing section '$section'"
  done
  grep -q "($name)" "$index" || fail "$name: not listed in docs/adr/README.md"
done

if ((errors > 0)); then
  echo "$errors ADR problem(s) found." >&2
  exit 1
fi
echo "ADRs OK."
