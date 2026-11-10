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

for tool in cargo-llvm-cov cargo-mutants .tools/bin/betterleaks .tools/bin/lizard .tools/bin/ruff \
  .tools/bin/shellcheck .tools/bin/shfmt .tools/bin/squawk .tools/bin/sqlfluff; do
  if ! command -v "$tool" >/dev/null; then
    echo "error: $tool is missing. Run scripts/install-tools.sh" >&2
    exit 1
  fi
done

# Checked before any slow stage, even the linters, so a clone set up before the nightly was pinned, or since its
# date moved, learns at once. Otherwise rustup could also install it alone, without the coverage tools.
nightly="$(<.nightly-toolchain)"
if [[ "$fast" == false ]]; then
  components="$(rustup component list --toolchain "$nightly" --installed 2>/dev/null || true)"
  if [[ $'\n'"$components" != *$'\n'llvm-tools* ]]; then
    echo "error: $nightly with llvm-tools-preview is missing. Run scripts/install-tools.sh" >&2
    exit 1
  fi
fi

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
  # The pre-commit hook runs this mode, so staged secrets stop here (ADR-0027).
  scripts/scan_secrets.sh --staged
  exit 0
fi

# Unset or empty, the tests use the default in .cargo/config.toml, which Cargo applies only to a variable that is
# not set at all. An explicit address is used as given.
if [[ -z "${TEST_DATABASE_URL:-}" ]]; then
  unset TEST_DATABASE_URL
  url="$(sed -n 's/^TEST_DATABASE_URL *= *"\(.*\)"$/\1/p' .cargo/config.toml 2>/dev/null || true)"
  if ! [[ "$url" =~ @([^:/]+):([0-9]+)/ ]] || ! (echo >"/dev/tcp/${BASH_REMATCH[1]}/${BASH_REMATCH[2]}") 2>/dev/null; then
    # No password, so the port is for this machine only. The data is throwaway, so it stays in memory. TLS uses
    # the image's self-signed certificate, which the TLS tests must refuse. CI starts the same container.
    echo "error: no test Postgres answers at ${url:-the default in .cargo/config.toml}. Start one, or set" >&2
    echo "TEST_DATABASE_URL to a Postgres the tests may create databases on:" >&2
    echo "  docker run -d --name tetsu-test-pg --restart unless-stopped -p 127.0.0.1:54329:5432 \\" >&2
    echo "    -e POSTGRES_HOST_AUTH_METHOD=trust --tmpfs /var/lib/postgresql postgres:18.6 \\" >&2
    echo "    -c fsync=off -c synchronous_commit=off -c full_page_writes=off -c ssl=on \\" >&2
    echo "    -c ssl_cert_file=/etc/ssl/certs/ssl-cert-snakeoil.pem -c ssl_key_file=/etc/ssl/private/ssl-cert-snakeoil.key" >&2
    exit 1
  fi
fi

python3 scripts/floor_guard.py "$@"
scripts/scan_secrets.sh --history
python3 -m unittest discover -b -s scripts/tests -t .
scripts/check-adrs.sh
python3 scripts/check_doc_sizes.py
python3 scripts/check_secrets.py

# Dependencies (ADR-0028). The version is checked, not only that the tool exists. Advisories need the network:
# when the fetch fails, the check runs offline on a cache deny.toml lets be at most 7 days old.
if [[ "$(cargo deny --version 2>/dev/null)" != "cargo-deny 0.20.2" ]]; then
  echo "error: cargo-deny 0.20.2 is needed. Run scripts/install-tools.sh" >&2
  exit 1
fi
offline=()
if ! cargo deny fetch >/dev/null 2>&1; then
  echo "warning: could not fetch the advisory database; checking against the cached copy" >&2
  offline=(--offline)
fi
cargo deny ${offline[@]+"${offline[@]}"} check --hide-inclusion-graph -D advisory-not-detected
python3 scripts/check_dated_entries.py deny.toml advisories ignore

# Limits are in ADR-0015 and ADR-0016. Cargo skips the benchmark harness, except for formatting.
work="$(mktemp -d)"
# Drop every test database on the way out, pass or fail. Tests drop their own, but a killed test cannot (#43).
finish() {
  local status=$?
  rm -rf "$work"
  if ! cargo run -q -p tetsu-postgres --example drop_test_databases; then
    echo "error: test databases were not dropped; drop them or rerun the checks" >&2
    status=1
  fi
  exit "$status"
}
trap finish EXIT
cargo llvm-cov --json --summary-only --output-path "$work/coverage.json"
# Coverage runs no doc tests, and some prove what must not compile, such as a module's port being out of reach
# (ADR-0021). Stable proves the examples on the toolchain we ship; only nightly's rustdoc checks the error code
# a compile_fail test names, so a test cannot pass for an unrelated error.
cargo test --doc
CARGO_TARGET_DIR=target/nightly cargo +"$nightly" test --doc
cargo metadata --no-deps --format-version 1 >"$work/metadata.json"
python3 scripts/check_coverage.py "$work/metadata.json" "$work/coverage.json"
# Branches need a nightly compiler (ADR-0020), pinned in .nightly-toolchain, in a build folder of its own. The
# stable run above stays the one that tests the code we ship.
CARGO_TARGET_DIR=target/nightly cargo +"$nightly" llvm-cov --branch --json --summary-only \
  --output-path "$work/branches.json"
python3 scripts/check_coverage.py --branches "$work/metadata.json" "$work/branches.json"
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
