#!/usr/bin/env bash
# Runs every local check. The pre-push hook and CI run all of it, and the pre-commit
# hook runs only formatters and linters with --fast. Otherwise it takes --base <ref>,
# the commit to compare with, which defaults to origin/main.
set -euo pipefail
cd "$(dirname "$0")/.."

fast=false
if [[ "${1:-}" == "--fast" ]]; then
  fast=true
  shift
fi

# A full run cleans up on the way out, however it ends, even at its first step: it removes what nothing uses, such
# as a killed test's server and files, cargo-mutants' copies, superseded build artifacts, and old build cache (#124).
# It drops every test database too, pass or fail, since tests drop their own but a killed test cannot (#43); only
# once a test Postgres has answered, since before then there is none to reach.
work=""
postgres=false
finish() {
  local status=$?
  if [[ -n "$work" ]]; then
    rm -rf "$work"
  fi
  python3 scripts/clean.py --routine
  if [[ "$postgres" == true ]] && ! cargo run -q -p tetsu-postgres --example drop_test_databases; then
    echo "error: test databases were not dropped; drop them or rerun the checks" >&2
    status=1
  fi
  exit "$status"
}
if [[ "$fast" == false ]]; then
  trap finish EXIT
fi

# On a maintainer's machine a check keeps to a budget of cores, half of them unless TETSU_CHECK_CORES says
# otherwise, and runs at low priority, so the machine stays usable while it runs. A CI runner does nothing else, so
# it uses every core. Each stage keeps to the budget: Cargo's build jobs and test threads, the script tests, Vitest,
# StrykerJS, and the mutant jobs, which share it.
all_cores="$(getconf _NPROCESSORS_ONLN)"
if [[ -n "${CI:-}" ]]; then
  cores="${TETSU_CHECK_CORES:-$all_cores}"
else
  cores="${TETSU_CHECK_CORES:-$((all_cores > 1 ? all_cores / 2 : 1))}"
  renice -n 10 -p $$ >/dev/null 2>&1 || true
  if command -v ionice >/dev/null; then
    ionice -c 2 -n 7 -p $$ >/dev/null 2>&1 || true
  fi
fi
if ! [[ "$cores" =~ ^[1-9][0-9]*$ ]]; then
  echo "error: TETSU_CHECK_CORES must be a whole number of cores, at least 1" >&2
  exit 1
fi
export TETSU_CHECK_CORES="$cores" CARGO_BUILD_JOBS="$cores" RUST_TEST_THREADS="$cores"

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
# mise, as Omarchy ships it, installs the release .nvmrc names once, and its folder goes first.
if ! right_node && command -v mise >/dev/null; then
  mise install --quiet "node@$wanted" >/dev/null 2>&1 &&
    mise_node="$(mise where "node@$wanted" 2>/dev/null)" &&
    PATH="$mise_node/bin:$PATH"
fi
if ! right_node; then
  echo "error: Node $wanted is required. Install it, for example: nvm install $wanted" >&2
  echo "or, with mise: mise install node@$wanted" >&2
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

for tool in cargo-llvm-cov cargo-mutants .tools/bin/betterleaks .tools/bin/mailpit .tools/bin/lizard .tools/bin/ruff \
  .tools/bin/shellcheck .tools/bin/shfmt .tools/bin/squawk .tools/bin/sqlfluff; do
  if ! command -v "$tool" >/dev/null; then
    echo "error: $tool is missing. Run scripts/install-tools.sh" >&2
    exit 1
  fi
done

# Checked before any slow stage, even the linters, so a clone set up before the nightly was pinned, or since its
# date moved, learns at once. Otherwise rustup could also install it alone, without the coverage tools.
nightly="$(<.config/nightly-toolchain)"
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
  cargo clippy --package tetsu-format-templates --all-targets -- -D warnings
  .tools/bin/ruff check
  .tools/bin/ruff format --check
  local shell_files=(scripts/*.sh benchmarks/run.sh .githooks/*)
  .tools/bin/shellcheck "${shell_files[@]}"
  .tools/bin/shfmt -d "${shell_files[@]}"
  local migrations=()
  while IFS= read -r file; do migrations+=("$file"); done < <(find crates -path '*/migrations/*.sql' | sort)
  if ((${#migrations[@]} > 0)); then
    .tools/bin/squawk --config .config/squawk.toml "${migrations[@]}"
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
  # And each is formatted (ADR-0083).
  cargo run --quiet --package tetsu-format-templates -- --check
}

lint
if [[ "$fast" == true ]]; then
  # The pre-commit hook runs this mode, so staged secrets stop here (ADR-0027).
  scripts/scan-secrets.sh --staged
  exit 0
fi

no_postgres() {
  # No password, so the port is for this machine only. The data is throwaway, so it stays in memory. TLS uses
  # the image's self-signed certificate, which the TLS tests must refuse. CI starts the same container.
  # The image is at least the floor in crates/postgres/src/migrate.rs (ADR-0060).
  echo "error: no test Postgres answers at $1. If Docker is stopped, start it, and the container below starts" >&2
  echo "with it. Otherwise start one, or set TEST_DATABASE_URL to a Postgres the tests may create databases on:" >&2
  echo "  docker run -d --name tetsu-test-pg --restart unless-stopped -p 127.0.0.1:54329:5432 \\" >&2
  echo "    -e POSTGRES_HOST_AUTH_METHOD=trust --tmpfs /var/lib/postgresql postgres:18.7 \\" >&2
  echo "    -c fsync=off -c synchronous_commit=off -c full_page_writes=off -c ssl=on \\" >&2
  echo "    -c ssl_cert_file=/etc/ssl/certs/ssl-cert-snakeoil.pem -c ssl_key_file=/etc/ssl/private/ssl-cert-snakeoil.key" >&2
  exit 1
}

# The tests' Postgres must answer before any test runs, or a stopped Docker shows up as failing tests. Unset or
# empty, the tests use the default in .cargo/config.toml, which Cargo applies only to a variable that is not set at
# all. An explicit address is used as given, and tried when it names a host and port; only those are printed, since
# it may hold a password.
authority='^[a-z]+://([^/?]*@)?([^:/?@]+):([0-9]+)(/|$)'
if [[ -z "${TEST_DATABASE_URL:-}" ]]; then
  unset TEST_DATABASE_URL
  url="$(sed -n 's/^TEST_DATABASE_URL *= *"\(.*\)"$/\1/p' .cargo/config.toml 2>/dev/null || true)"
  [[ "$url" =~ $authority ]] || no_postgres "the default in .cargo/config.toml"
else
  url="$TEST_DATABASE_URL"
fi
if [[ "$url" =~ $authority ]] && ! (echo >"/dev/tcp/${BASH_REMATCH[2]}/${BASH_REMATCH[3]}") 2>/dev/null; then
  no_postgres "${BASH_REMATCH[2]}:${BASH_REMATCH[3]}"
fi
postgres=true

base="origin/main"
if [[ "${1:-}" == "--base" ]]; then
  base="$2"
fi
# cargo-mutants reads only plain text diffs with a/ and b/ paths and every changed line, so override each
# setting that changes them. It mutates the files on disk, so the diff compares them with the base.
plain=(--text --no-ext-diff --no-textconv --no-color --no-renames --src-prefix=a/ --dst-prefix=b/)
merge="$(git merge-base "$base" HEAD)"
# In CI, whether anything in the given paths changed since the base, which a successful run already checked: a
# group of checks whose inputs are unchanged is skipped, and the log says so (ADR-0064). Outside CI every group runs.
changed() {
  [[ -z "${CI:-}" ]] || ! git diff --quiet "$merge" -- "$@"
}

python3 scripts/floor_guard.py "$@"
scripts/scan-secrets.sh --history
# Each script test in its own process, one per core, so the slowest one sets the time. They read scripts/ and the
# repository's settings, workflows, and docs, never the Rust or the front end.
if changed . ':(exclude)crates' ':(exclude)web'; then
  python3 scripts/parallel_unittest.py scripts/tests .
else
  echo "Nothing outside crates/ and web/ changed since $base, so the script tests are skipped" >&2
fi
scripts/check-adrs.sh
python3 scripts/check_doc_sizes.py
python3 scripts/check_secrets.py

# Dependencies (ADR-0028). The version is checked, not only that the tool exists. Advisories need the network:
# when the fetch fails, the check runs offline on a cache .config/deny.toml lets be at most 7 days old.
if [[ "$(cargo deny --version 2>/dev/null)" != "cargo-deny 0.20.2" ]]; then
  echo "error: cargo-deny 0.20.2 is needed. Run scripts/install-tools.sh" >&2
  exit 1
fi
offline=()
if ! cargo deny --config .config/deny.toml fetch >/dev/null 2>&1; then
  echo "warning: could not fetch the advisory database; checking against the cached copy" >&2
  offline=(--offline)
fi
cargo deny --config .config/deny.toml ${offline[@]+"${offline[@]}"} check --hide-inclusion-graph \
  -D advisory-not-detected
python3 scripts/check_dated_entries.py .config/deny.toml advisories ignore
python3 scripts/check_dated_entries.py .config/npm-checks.toml advisories ignore
python3 scripts/check_dated_entries.py .config/npm-checks.toml overrides entries

# Limits are in ADR-0015 and ADR-0016. Cargo skips the benchmark harness, except for formatting.
work="$(mktemp -d)"

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
# Coverage and mutation testing give the same result for the same Rust, so a pass is recorded against a
# fingerprint of every input they read: the Rust and its build settings, and the checks themselves. A later run
# outside CI that finds the same fingerprint skips both and says so. The base is left out: a push moves it, but the
# mutants drawn against the new base are changes the last pass already tested, so a docs-only push after it skips.
rust_inputs=(crates tools Cargo.toml Cargo.lock rust-toolchain.toml .config/nightly-toolchain .cargo scripts/check.sh
  scripts/check_coverage.py)
fingerprint="$({
  { find "${rust_inputs[@]}" -type f 2>/dev/null || true; } | LC_ALL=C sort | while IFS= read -r file; do
    sha256sum "$file"
  done
} | sha256sum | cut -d ' ' -f 1)"
pass_stamp=target/check-rust-pass
reuse=false
if [[ -z "${CI:-}" && "$(cat "$pass_stamp" 2>/dev/null)" == "$fingerprint" ]]; then
  reuse=true
  echo "Rust is unchanged since the last full pass, so coverage and mutation testing are skipped" >&2
elif ! changed "${rust_inputs[@]}"; then
  reuse=true
  echo "Rust is unchanged since $base, so coverage, the doc tests, and mutation testing are skipped" >&2
fi
if [[ "$reuse" == false ]]; then
  cargo llvm-cov --json --summary-only --output-path "$work/coverage.json"
  # Coverage runs no doc tests, and some prove what must not compile, such as a module's port being out of reach
  # (ADR-0021). Stable proves the examples on the toolchain we ship; only nightly's rustdoc checks the error code
  # a compile_fail test names, so a test cannot pass for an unrelated error.
  cargo test --doc
  # The development tools are outside the default members, so they are covered on their own, at Tier 2 and 3's
  # limits (ADR-0016).
  cargo llvm-cov --package tetsu-format-templates --summary-only --fail-under-regions 90 --fail-under-lines 90
  CARGO_TARGET_DIR=target/nightly cargo +"$nightly" test --doc
  cargo metadata --no-deps --format-version 1 >"$work/metadata.json"
  python3 scripts/check_coverage.py "$work/metadata.json" "$work/coverage.json"
  # Branches need a nightly compiler (ADR-0020), pinned in .config/nightly-toolchain, in a build folder of its own. The
  # stable run above stays the one that tests the code we ship.
  CARGO_TARGET_DIR=target/nightly cargo +"$nightly" llvm-cov --branch --json --summary-only \
    --output-path "$work/branches.json"
  python3 scripts/check_coverage.py --branches "$work/metadata.json" "$work/branches.json"
fi
# TypeScript in web/ (ADR-0056): unit tests under coverage limits, then mutation testing, where a surviving mutant
# fails.
web_inputs=(web package.json package-lock.json tsconfig.json .nvmrc .config/stryker.config.json scripts/check.sh)
if changed "${web_inputs[@]}"; then
  npx --no-install vitest run --coverage --config web/vitest.config.ts --maxWorkers="$cores"
  npx --no-install stryker run .config/stryker.config.json --concurrency "$cores"
else
  echo "The front end is unchanged since $base, so its tests and mutation testing are skipped" >&2
fi
.tools/bin/lizard -l rust -C 10 -L 50 -a 5 -w crates tools
.tools/bin/lizard -l typescript -C 10 -L 50 -a 5 -w web
npx --no-install jscpd crates tools web --threshold 0 --ignore "**/tests/**,**/assets/build/**" --reporters console
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
if [[ "$reuse" == false ]]; then
  git diff "${plain[@]}" "$merge" -- crates >"$work/rust.diff"
  if [[ -s "$work/rust.diff" ]]; then
    # Each job builds its own copy of the workspace, about five seconds, then tests a share of the mutants at
    # about five seconds each, so a job pays off from about 15 mutants: one per 15, rounded up. Never more than
    # one per four cores of the budget, where builds start to compete, and never fewer than one. Six jobs took 90
    # mutants from 146 to 104 seconds on 24 cores (2026-11-11). CARGO_MUTANTS_JOBS overrides it. The jobs share
    # the budget's build jobs and test threads, rather than each taking all of them.
    count="$(cargo mutants --in-diff "$work/rust.diff" --list | wc -l)"
    most=$((cores / 4))
    jobs=$(((count + 14) / 15))
    if ((jobs > most)); then jobs=$most; fi
    if ((jobs < 1)); then jobs=1; fi
    jobs="${CARGO_MUTANTS_JOBS:-$jobs}"
    if ! [[ "$jobs" =~ ^[1-9][0-9]*$ ]]; then
      echo "error: CARGO_MUTANTS_JOBS must be a whole number of jobs, at least 1" >&2
      exit 1
    fi
    share=$((cores / jobs > 0 ? cores / jobs : 1))
    # The coverage runs above just passed these tests, so cargo-mutants skips its own unmutated run, which also
    # sized its timeout, so that is set here: about six times the test suite's time.
    mutants=(cargo mutants --in-diff "$work/rust.diff" --baseline skip --timeout 60)
    if [[ -n "${CI:-}" && "$jobs" == 1 ]]; then
      # A runner runs one job, so it mutates its checkout in place, which it throws away after, and reuses
      # target/ rather than build every dependency again in a copy, which leaves target/ out.
      "${mutants[@]}" --in-place
    else
      # Its copies of the workspace go under target/, on disk, since a memory-backed /tmp may hold too few. Here
      # the checkout is the maintainer's, which a killed run in place would leave mutated.
      mkdir -p target/mutants-tmp
      CARGO_BUILD_JOBS="$share" RUST_TEST_THREADS="$share" TMPDIR="$PWD/target/mutants-tmp" \
        "${mutants[@]}" --jobs "$jobs"
    fi
  elif ! git diff --quiet "${plain[@]}" "$merge" HEAD -- crates; then
    echo "error: commits since $base change crates/, but the files on disk undo that, so there is nothing to" >&2
    echo "mutate. Commit or restore those files first." >&2
    exit 1
  fi
fi

if [[ "$reuse" == false ]]; then
  # cargo-mutants mutates only the default members, so the development tools are named, in a copy under target/.
  git diff "${plain[@]}" "$merge" -- tools >"$work/tools.diff"
  if [[ -s "$work/tools.diff" ]]; then
    mkdir -p target/mutants-tmp
    TMPDIR="$PWD/target/mutants-tmp" cargo mutants --package tetsu-format-templates --in-diff "$work/tools.diff" \
      --timeout 60
  fi
fi

mkdir -p target
echo "$fingerprint" >"$pass_stamp"
