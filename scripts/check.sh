#!/usr/bin/env bash
# Runs every local check. The pre-push hook and CI run all of it, and the pre-commit
# hook runs only formatters and linters with --fast. Otherwise it takes --base <ref>,
# the commit to compare with, which defaults to origin/main.
set -euo pipefail
cd "$(dirname "$0")/.."

wanted="$(cat .nvmrc)"
right_node() { [[ "$(node --version 2>/dev/null)" == "v$wanted."* ]]; }
nvm_script="${NVM_DIR:-$HOME/.nvm}/nvm.sh"

if ! right_node && [[ -s "$nvm_script" ]]; then
  # nvm's own script does not run under these shell options.
  set +eu
  # shellcheck source=/dev/null # nvm lives outside the repository.
  . "$nvm_script"
  nvm use "$wanted" >/dev/null
  set -eu
fi
if ! right_node; then
  echo "error: Node $wanted is required. Install it, for example: nvm install $wanted" >&2
  exit 1
fi

fast=false
if [[ "${1:-}" == "--fast" ]]; then
  fast=true
  shift
fi

for tool in cargo-llvm-cov cargo-mutants .tools/bin/lizard .tools/bin/ruff .tools/bin/shellcheck \
  .tools/bin/shfmt .tools/bin/squawk .tools/bin/sqlfluff; do
  if ! command -v "$tool" >/dev/null; then
    echo "error: $tool is missing. Run scripts/install-tools.sh" >&2
    exit 1
  fi
done

# Formatters and linters, fast enough to run on every commit (ADR-0017).
lint() {
  cargo fmt --all --check
  cargo clippy --all-targets -- -D warnings
  .tools/bin/ruff check
  .tools/bin/ruff format --check
  local shell_files=(scripts/*.sh benchmarks/run.sh .githooks/*)
  .tools/bin/shellcheck "${shell_files[@]}"
  .tools/bin/shfmt -d "${shell_files[@]}"
  local migrations=()
  while IFS= read -r file; do migrations+=("$file"); done < <(find crates -path '*/migrations/*.sql' | sort)
  if ((${#migrations[@]} > 0)); then
    .tools/bin/squawk "${migrations[@]}"
  fi
  local sql=()
  while IFS= read -r file; do sql+=("$file"); done < <(find crates benchmarks -name '*.sql' 2>/dev/null | sort)
  if ((${#sql[@]} > 0)); then
    .tools/bin/sqlfluff lint "${sql[@]}"
  fi
  npx --yes markdownlint-cli2@0.24.0 "**/*.md"
}

lint
if [[ "$fast" == true ]]; then
  exit 0
fi

if [[ -z "${TEST_DATABASE_URL:-}" ]]; then
  # No password, so the port is for this machine only. The data is throwaway, so it stays in memory.
  echo "error: set TEST_DATABASE_URL to a Postgres the tests may create databases on, for example:" >&2
  echo "  docker run -d --name tetsu-test-pg --restart unless-stopped -p 127.0.0.1:54329:5432 \\" >&2
  echo "    -e POSTGRES_HOST_AUTH_METHOD=trust --tmpfs /var/lib/postgresql postgres:18.6 \\" >&2
  echo "    -c fsync=off -c synchronous_commit=off -c full_page_writes=off" >&2
  echo "  export TEST_DATABASE_URL=postgres://postgres@localhost:54329/postgres" >&2
  exit 1
fi

python3 scripts/floor_guard.py "$@"
python3 -m unittest discover -b -s scripts/tests -t .
scripts/check-adrs.sh
python3 scripts/check_doc_sizes.py

# Limits are in ADR-0015 and ADR-0016. Cargo skips the benchmark harness, except for formatting.
work="$(mktemp -d)"
# Drop every test database on the way out, pass or fail. Tests drop their own, but a killed test cannot (#43).
finish() {
  rm -rf "$work"
  cargo run -q -p tetsu-postgres --example drop_test_databases ||
    echo "warning: test databases were not dropped; the next test run sweeps them" >&2
}
trap finish EXIT
cargo llvm-cov --json --summary-only --output-path "$work/coverage.json"
cargo metadata --no-deps --format-version 1 >"$work/metadata.json"
python3 scripts/check_coverage.py "$work/metadata.json" "$work/coverage.json"
.tools/bin/lizard -l rust -C 10 -L 50 -a 5 -w crates
npx --yes jscpd@5.4.2 crates --threshold 0 --ignore "**/tests/**" --reporters console

base="origin/main"
if [[ "${1:-}" == "--base" ]]; then
  base="$2"
fi
# cargo-mutants reads only plain text diffs with a/ and b/ paths and every changed line, so override each
# setting that changes them. It mutates the files on disk, so the diff compares them with the base.
plain=(--text --no-ext-diff --no-textconv --no-color --no-renames --src-prefix=a/ --dst-prefix=b/)
merge="$(git merge-base "$base" HEAD)"
git diff "${plain[@]}" "$merge" -- crates >"$work/rust.diff"
if [[ -s "$work/rust.diff" ]]; then
  # Each job builds its own copy of the workspace, about five seconds, so jobs pay off only with many mutants:
  # about the square root of their count over eight, so one for under 18, two up to 49, three up to 97. Never
  # more than one per four cores, where builds start to compete. CARGO_MUTANTS_JOBS overrides it.
  count="$(cargo mutants --in-diff "$work/rust.diff" --list | wc -l)"
  most=$(($(getconf _NPROCESSORS_ONLN) / 4))
  jobs=1
  while ((jobs < most && count >= 2 * (2 * jobs + 1) ** 2)); do jobs=$((jobs + 1)); done
  cargo mutants --in-diff "$work/rust.diff" --jobs "${CARGO_MUTANTS_JOBS:-$jobs}"
elif ! git diff --quiet "${plain[@]}" "$merge" HEAD -- crates; then
  echo "error: commits since $base change crates/, but the files on disk undo that, so there is nothing to" >&2
  echo "mutate. Commit or restore those files first." >&2
  exit 1
fi
