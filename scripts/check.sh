#!/usr/bin/env bash
# Runs every local check. The pre-push hook and CI run all of it, and the pre-commit
# hook runs only formatters and linters with --fast. Otherwise it takes --base <ref>,
# the commit to compare with, which defaults to origin/main.
set -euo pipefail
cd "$(dirname "$0")/.."

wanted="$(cat .nvmrc)"
# .nvmrc names an exact release, which fixes npm's version too, or a major version.
right_node() {
  local found
  found="$(node --version 2>/dev/null)"
  [[ "$found" == "v$wanted" || "$found" == "v$wanted."* ]]
}
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
# npm is checked like cargo-deny, against the version package.json names (ADR-0034).
wanted_npm="$(sed -n 's/.*"packageManager": *"npm@\([0-9.]*\)".*/\1/p' package.json)"
if [[ "$(npm --version 2>/dev/null)" != "$wanted_npm" ]]; then
  echo "error: npm $wanted_npm is required, the one Node $wanted bundles" >&2
  exit 1
fi
# The tree the lockfile names, installed without scripts whatever .npmrc or the environment say, and only when the
# lockfile has changed since the last install, so the hooks, CI, and a run by hand all use the same tree.
lock_sum="$( (sha256sum package-lock.json 2>/dev/null || shasum -a 256 package-lock.json) | cut -d' ' -f1)"
# Development packages are installed and audited whatever NODE_ENV or an omit setting says, since every one is.
if [[ "$(cat node_modules/.tetsu-lock 2>/dev/null)" != "$lock_sum" ]]; then
  if ! npm ci --ignore-scripts --include=dev --no-audit --no-fund --loglevel=error; then
    echo "error: could not install the Node tools, which needs the network once per lockfile change." >&2
    echo "Run: npm ci --ignore-scripts --include=dev" >&2
    exit 1
  fi
  echo "$lock_sum" >node_modules/.tetsu-lock
fi
# The front-end build the tetsu crate embeds is not in git, so it is made before anything compiles (ADR-0036). It is
# remade only when an input is newer, since rewriting it would make Cargo rebuild the crate on every run.
manifest=crates/tetsu/assets/build/.vite/manifest.json
if [[ ! -f "$manifest" ]] || [[ -n "$(find web crates/*/templates package-lock.json -type f -newer "$manifest" -print -quit)" ]]; then
  npx --no-install vite build --config web/vite.config.ts --logLevel error
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
  npx --no-install markdownlint-cli2 "**/*.md"
  npx --no-install biome check --error-on-warnings
  npx --no-install tsc -p .
  # Templates use only classes the build defines, and no inline styles (ADR-0032).
  local templates=()
  while IFS= read -r file; do templates+=("$file"); done < <(find crates web -path '*/templates/*.html' | sort)
  python3 scripts/check_templates.py crates/tetsu/assets/build/assets/tetsu-*.css ${templates[@]+"${templates[@]}"}
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
# Each script test in its own process, one per core, so the slowest one sets the time.
python3 scripts/parallel_unittest.py scripts/tests .
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
python3 scripts/check_dated_entries.py npm-checks.toml advisories ignore
python3 scripts/check_dated_entries.py npm-checks.toml overrides entries

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

# npm dependencies (ADR-0034). Advisories and deprecations need the network: when the audit or the registry cannot
# be reached, the last passing check of the same lockfile counts for 7 days, and everything else still runs.
audit_stamp=.tools/npm-audit-pass
# A node_modules changed by hand, such as with npm install --no-save, no longer matches the lockfile.
if ! npm ls --all --include=dev >/dev/null 2>&1; then
  echo "error: node_modules does not match package-lock.json. Run: npm ci --ignore-scripts --include=dev" >&2
  exit 1
fi
npm audit --json --include=dev >"$work/audit.json" 2>/dev/null || true
npm_status=0
python3 scripts/check_npm.py . --audit "$work/audit.json" || npm_status=$?
if ((npm_status == 0)); then
  echo "$lock_sum $(date -u +%s)" >"$audit_stamp"
elif ((npm_status == 3)); then
  read -r passed_sum passed_at <"$audit_stamp" 2>/dev/null || true
  if [[ "${passed_sum:-}" != "$lock_sum" ]] || (($(date -u +%s) - ${passed_at:-0} > 7 * 86400)); then
    echo "error: npm advisories could not be checked, and this lockfile has no passing check in 7 days" >&2
    exit 1
  fi
  echo "warning: npm advisories could not be checked; using the last passing check of this lockfile" >&2
  python3 scripts/check_npm.py .
else
  exit "$npm_status"
fi
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
.tools/bin/lizard -l typescript -C 10 -L 50 -a 5 -w web
npx --no-install jscpd crates web --threshold 0 --ignore "**/tests/**,**/assets/build/**" --reporters console
# The image's front-end stage, from the image's own build context, so a file Vite reads that the Dockerfile or
# .dockerignore leaves out fails here rather than in CI. CI builds the whole image in another job (ADR-0023), so it
# skips this, and so does a machine without a running Docker, with a warning.
if [[ -n "${CI:-}" ]]; then
  :
elif docker info >/dev/null 2>&1; then
  if ! docker build --progress=plain --target web --tag tetsu:web-check . >"$work/web-stage.log" 2>&1; then
    tail -n 60 "$work/web-stage.log" >&2
    exit 1
  fi
else
  echo "warning: Docker is not running, so the image's front-end stage was not built. CI still builds it." >&2
fi

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
  # The coverage runs above just passed these tests, so cargo-mutants skips its own unmutated run, which also
  # sized its timeout, so that is set here: about six times the test suite's time. Its copies of the workspace go
  # under target/, on disk, since a memory-backed /tmp may hold too few of them.
  mkdir -p target/mutants-tmp
  TMPDIR="$PWD/target/mutants-tmp" cargo mutants --in-diff "$work/rust.diff" --baseline skip --timeout 60 \
    --jobs "${CARGO_MUTANTS_JOBS:-$jobs}"
elif ! git diff --quiet "${plain[@]}" "$merge" HEAD -- crates; then
  echo "error: commits since $base change crates/, but the files on disk undo that, so there is nothing to" >&2
  echo "mutate. Commit or restore those files first." >&2
  exit 1
fi
