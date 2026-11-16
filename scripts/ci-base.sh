#!/usr/bin/env bash
# The commit a CI run checks every change since, and whether that is the run's own commit. A pull request checks
# against its base; a manual run against the base it is given, or the commit the last successful manual run on
# main checked (ADR-0023). Only manual runs count, since a pull request run checks its own diff, not main's batch.
# With neither, it fails rather than check too little. A base that is the run's own commit has been checked by a
# successful run already, so the run has nothing to check (ADR-0064). It writes sha= and same= to GITHUB_OUTPUT.
set -euo pipefail

base="${PR_BASE:-${INPUT_BASE:-}}"
if [[ -z "$base" ]]; then
  base="$(gh run list --repo "$GITHUB_REPOSITORY" --workflow ci.yml --branch main \
    --event workflow_dispatch --status success --limit 1 --json headSha --jq '.[0].headSha // empty')"
fi
if [[ -z "$base" ]]; then
  echo "::error::No successful manual run on main to check changes since. Run again with the base input set."
  exit 1
fi
# A full commit SHA, so the value can never reach git as an option.
if [[ ! "$base" =~ ^[0-9a-f]{40}$ ]]; then
  echo "::error::The base must be a full commit SHA."
  exit 1
fi
same=false
if [[ "$base" == "$GITHUB_SHA" ]]; then
  same=true
  echo "::notice::A successful run already checked $base, this run's own commit, so there is nothing to check."
fi
{
  echo "sha=$base"
  echo "same=$same"
} >>"$GITHUB_OUTPUT"
