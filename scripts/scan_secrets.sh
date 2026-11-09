#!/usr/bin/env bash
# Scans for secrets with Betterleaks (ADR-0027). Exits 1 on a finding or an error, 2 on bad usage.
#
#   --staged          the staged changes, from the pre-commit hook
#   --push <remote>   each commit and tag being pushed to <remote>, and their messages, read from the pre-push
#                     hook's stdin
#   --history         all history reachable from HEAD, and every commit and tag message, from check.sh
set -euo pipefail
cd "$(dirname "$0")/.."
root="$PWD"

scanner="$root/.tools/bin/betterleaks"
# Findings print redacted, inline allow comments do nothing, and nothing is validated against a live service.
flags=(-c "$root/.betterleaks.toml" -i "$root/.betterleaksignore" --redact --ignore-gitleaks-allow --no-banner
  --log-level warn)

# The floor guard watches these two files, so the scan must not read rules or ignores from anywhere else. A
# missing ignore file would let Betterleaks fall back to an unchecked .gitleaksignore.
for input in .betterleaks.toml .betterleaksignore; do
  if [[ -L "$input" || ! -f "$input" ]]; then
    echo "error: $input must be a regular file; the scan reads only that file" >&2
    exit 1
  fi
done
# Any "path" key, quoted, dotted, or inline, could point [extend] at another file, and so could an escaped
# one. No rule here needs either.
if grep -Eq '(^|[{,.[:space:]])["'"'"']?path["'"'"']?[[:space:]]*=|\\[uU][0-9a-fA-F]{4}' .betterleaks.toml; then
  echo "error: .betterleaks.toml names a path; keep every rule in it, not in another file" >&2
  exit 1
fi
# An entry without a commit or tag id would skip that file and line in every commit, forever.
entries="$(grep -Ev '^[[:space:]]*(#|$)' .betterleaksignore || true)"
if [[ -n "$entries" ]] && grep -Evq '^[0-9a-f]{40}:' <<<"$entries"; then
  echo "error: every .betterleaksignore entry must start with the commit or tag id it applies to" >&2
  exit 1
fi

# Temporary files go on exit, including an interrupted one.
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# Runs the scanner with the remaining arguments, then names each finding by where it is and its rule, never by
# its content, so finding one needs no rerun that could print it. The first argument says what is scanned:
# "staged", "commits", or "messages", whose files are named by object id.
report() {
  local kind="$1" status=0
  shift
  "$scanner" "$@" -f json -r "$work/report.json" || status=$?
  if ((status != 0)) && [[ -s "$work/report.json" ]]; then
    python3 - "$kind" "$work/report.json" <<'EOF'
import json
import sys

kind, path = sys.argv[1:]
for finding in json.load(open(path)):
    where = {"staged": "staged", "commits": finding["Commit"], "messages": "message of"}[kind]
    print(f"secret found: {where} {finding['File']}:{finding['StartLine']} ({finding['RuleID']})", file=sys.stderr)
EOF
  fi
  rm -f "$work/report.json"
  return "$status"
}

# Scans the commits git log selects with these arguments, including changes made in merges.
commits() {
  report commits git "${flags[@]}" --log-opts "-m $*" .
}

# Scans the messages of the commit and tag objects named as arguments, one file each, named by its object id. A
# finding's fingerprint is then "<object>:<rule>:<line>", which stays put as history grows, so a rotated secret in
# a message can be listed in .betterleaksignore like any other.
messages() {
  (($#)) || return 0
  local dir="$work/messages" object status=0
  mkdir -p "$dir"
  for object in "$@"; do
    git cat-file -p "$object" | sed '1,/^$/d' >"$dir/$object" || status=1
  done
  if ((status == 0)); then
    (cd "$dir" && report messages dir "${flags[@]}" .) || status=$?
  fi
  rm -rf "$dir"
  return "$status"
}

# Prints the object ids in these lines, one per line, as an array assignment can read without word splitting.
ids() {
  local line
  while read -r line; do
    if [[ -n "$line" ]]; then
      echo "$line"
    fi
  done <<<"$1"
}

# Each scan runs even when an earlier one finds something, so one run reports every finding.
found=0
case "${1:-}" in
  --staged)
    # Git shows no lines for a file whose attributes turn diffs off, so the staged scan would miss it. History
    # and push scans read every line regardless.
    attributes=(git ls-files -z -co --exclude-standard -- ':(glob)**/.gitattributes')
    "${attributes[@]}" >/dev/null
    while IFS= read -r -d '' file; do
      if grep -Eq '(^|[[:space:]])(-diff|!diff|binary|diff=)' "$file"; then
        echo "error: $file turns diffs off, which would hide files from the secret scan" >&2
        exit 1
      fi
    done < <("${attributes[@]}")
    report staged git --staged "${flags[@]}" . || found=1
    ;;
  --history)
    commits HEAD || found=1
    # Captured first, so a failing git command fails the scan.
    revs="$(git rev-list HEAD)"
    tags="$(git for-each-ref --format='%(objecttype) %(objectname)' refs/tags)"
    objects=()
    while read -r id; do objects+=("$id"); done < <(ids "$revs"$'\n'"$(sed -n 's/^tag //p' <<<"$tags")")
    messages ${objects[@]+"${objects[@]}"} || found=1
    ;;
  --push)
    remote="${2:?usage: scripts/scan_secrets.sh --push <remote>}"
    # Each line is "<local ref> <local object> <remote ref> <remote object>". What the remote's own branches
    # lack is what this push sends, which covers new branches and a remote holding commits this clone lacks.
    while read -r _ local_object _ _; do
      if [[ "$local_object" =~ ^0+$ ]]; then
        continue
      fi
      revs="$(git rev-list "$local_object" --not --remotes="$remote")"
      kind="$(git cat-file -t "$local_object")"
      objects=()
      while read -r id; do objects+=("$id"); done < <(ids "$revs")
      if [[ "$kind" == tag ]]; then
        objects+=("$local_object")
      fi
      if ((${#objects[@]} > 0)); then
        commits "$local_object" --not --remotes="$remote" || found=1
        messages "${objects[@]}" || found=1
      fi
    done
    ;;
  *)
    echo "usage: scripts/scan_secrets.sh --staged | --push <remote> | --history" >&2
    exit 2
    ;;
esac
exit "$found"
