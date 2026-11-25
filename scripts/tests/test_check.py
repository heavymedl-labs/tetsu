"""check.sh must run each stage and stop on the first failure, and the hook must call it."""

import os
import re
import shutil
import socket
import stat
import tempfile
import unittest
from pathlib import Path

from scripts.tests.support import ROOT, run, write

EXECUTABLE = stat.S_IRWXU
# Every cargo stand-in reports the cargo-deny version the environment names, and fails a fetch when asked to.
CARGO_DENY = (
    'if [ "$1 $2" = "deny --version" ]; then echo "cargo-deny ${DENY_VERSION-0.20.2}"; exit 0; fi\n'
    'if [ "$1 $2 $3 $4" = "deny --config .config/deny.toml fetch" ]; then exit "${DENY_FETCH-0}"; fi\n'
)


class CheckScript(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.log = self.root / "log"
        # Something answers at the test Postgres's address, as a running one would.
        self.postgres = socket.create_server(("127.0.0.1", 0))
        self.addCleanup(self.postgres.close)
        self.database_url = f"postgres://postgres@127.0.0.1:{self.postgres.getsockname()[1]}/postgres"
        (self.root / "scripts").mkdir()
        shutil.copy(ROOT / "scripts" / "check.sh", self.root / "scripts")
        write(self.root / ".config/nightly-toolchain", "nightly-2026-11-01\n")
        write(self.root / ".nvmrc", "26\n")
        self.bin = self.root / "bin"
        self.stub("node", "echo v26.1.0")
        # No mise unless a test gives one, so the machine's own never answers.
        self.stub("mise", "exit 1")
        for tool in ("npx", "cargo", "cargo-llvm-cov", "cargo-mutants"):
            self.stub(tool, f'echo "{tool} $*" >> "{self.log}"')
        write(self.root / "package.json", '{\n  "packageManager": "npm@11.20.0"\n}\n')
        write(self.root / "package-lock.json", "{}\n")
        # npm reports its version and an audit, and its install makes node_modules.
        self.stub(
            "npm",
            'case "$1" in --version) echo "${NPM_VERSION-11.20.0}";; '
            f'audit) echo "{{}}"; echo "npm $*" >> "{self.log}";; ls) exit "${{NPM_LS-0}}";; '
            f'ci) [ -n "${{NPM_CI_FAILS-}}" ] && exit 1; mkdir -p node_modules; '
            f'echo "npm $*" >> "{self.root / "installs"}";; esac',
        )
        # A build writes the stylesheet the tetsu crate embeds, which is not in git.
        self.stub(
            "npx",
            'if [ "$2" = vite ]; then mkdir -p crates/tetsu/assets/build/assets; '
            "echo built > crates/tetsu/assets/build/assets/tetsu-x.css; "
            "mkdir -p crates/tetsu/assets/build/.vite; "
            "echo {} > crates/tetsu/assets/build/.vite/manifest.json; "
            f'echo "npx $*" >> "{self.root / "builds"}"; exit 0; fi\n'
            f'echo "npx $*" >> "{self.log}"',
        )
        # Cargo also logs its build folder, which the nightly pass keeps apart.
        self.stub("cargo", f'echo "cargo $* dir=${{CARGO_TARGET_DIR-}}" >> "{self.log}"')
        self.stub("rustup", 'echo "llvm-tools-x86_64-unknown-linux-gnu"')
        # The npm check exits with the status the environment names, as when the registry cannot be reached.
        self.stub(
            "python3",
            f'echo "python $1 $2 $3" >> "{self.log}"\n'
            'if [ "$1 $3" = "scripts/check_npm.py --audit" ]; then exit "${NPM_CHECK-0}"; fi',
        )
        # git diff --quiet answers as the environment says: 0, unchanged since the base, unless a test says.
        self.stub(
            "git",
            'if [ "$1" = merge-base ]; then echo "${MERGE_BASE-abc}"; '
            'elif [ "$1 $2" = "diff --quiet" ]; then echo "$@" >> "'
            + str(self.log)
            + '"; exit "${DIFF_QUIET-0}"; else echo "$@" >> "'
            + str(self.log)
            + '"; echo changed; fi',
        )
        # Docker answers info as the environment says, and a build fails when asked to, with Vite's error.
        self.stub(
            "docker",
            f'echo "docker $*" >> "{self.log}"\n'
            'if [ "$1" = info ]; then exit "${DOCKER_INFO-0}"; fi\n'
            'if [ -n "${DOCKER_BUILD_FAILS-}" ]; then echo "Cannot resolve entry module"; exit 1; fi',
        )
        for tool in ("lizard", "ruff", "shellcheck", "shfmt", "squawk", "sqlfluff"):
            write(
                self.root / ".tools" / "bin" / tool,
                f'#!/bin/sh\necho "{tool} $*" >> "{self.log}"\n',
                EXECUTABLE,
            )
        write(self.root / "crates" / "a" / "migrations" / "0001.sql", "SELECT 1;\n")
        write(self.root / "scripts" / "check-adrs.sh", f'#!/bin/sh\necho adrs >> "{self.log}"\n', EXECUTABLE)
        scan = f'#!/bin/sh\necho "scan $1" >> "{self.log}"\n'
        write(self.root / "scripts" / "scan-secrets.sh", scan, EXECUTABLE)
        write(self.root / ".tools" / "bin" / "betterleaks", "#!/bin/sh\n", EXECUTABLE)
        write(self.root / ".tools" / "bin" / "mailpit", "#!/bin/sh\n", EXECUTABLE)

    no_database = False

    def stub(self, name, body):
        prefix = CARGO_DENY if name == "cargo" else ""
        write(self.bin / name, f"#!/bin/sh\n{prefix}{body}\n", EXECUTABLE)

    def check(self, *args, nvm_dir=None, **env):
        # CI is unset unless a test sets it, even when the tests themselves run in CI.
        env = {"CI": "", **env}
        return run(
            [self.root / "scripts" / "check.sh", *args],
            self.root,
            PATH=f"{self.bin}:/usr/bin:/bin",
            NVM_DIR=str(nvm_dir or self.root / "no-nvm"),
            TEST_DATABASE_URL="" if self.no_database else self.database_url,
            **env,
        )

    def test_runs_every_stage(self):
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        log = self.log.read_text()
        for stage in (
            "floor_guard.py",
            "scan --history",
            "unittest",
            "adrs",
            "check_doc_sizes.py",
            "cargo deny --config .config/deny.toml check --hide-inclusion-graph -D advisory-not-detected",
            "check_dated_entries.py .config/deny.toml advisories",
            "check_dated_entries.py .config/npm-checks.toml advisories",
            "check_dated_entries.py .config/npm-checks.toml overrides",
            "check_npm.py . --audit",
            "check_templates.py",
            "npm audit --json --include=dev",
            "biome check --error-on-warnings",
            "tsc -p .",
            "lizard -l typescript",
            "markdownlint-cli2",
            "cargo fmt --all --check",
            "cargo clippy",
            "cargo llvm-cov --json",
            "cargo test --doc",
            "cargo metadata",
            "check_coverage.py",
            "cargo +nightly-2026-11-01 llvm-cov --branch",
            "check_coverage.py --branches",
            "lizard",
            "jscpd",
            "ruff check",
            "ruff format --check",
            "shellcheck",
            "shfmt -d",
            "squawk",
            "sqlfluff lint",
            "cargo mutants",
        ):
            self.assertIn(stage, log)

    def test_builds_the_images_front_end_stage(self):
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("docker build --progress=plain --target web", self.log.read_text())

    def test_a_failing_front_end_stage_fails_and_shows_why(self):
        result = self.check(DOCKER_BUILD_FAILS="1")
        self.assertEqual(result.returncode, 1)
        self.assertIn("Cannot resolve entry module", result.stderr)

    def test_ci_leaves_the_front_end_stage_to_its_image_job(self):
        result = self.check(CI="true")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("docker", self.log.read_text())

    def test_without_a_running_docker_the_stage_is_skipped_with_a_warning(self):
        result = self.check(DOCKER_INFO="1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Docker is not running", result.stderr)
        self.assertNotIn("docker build", self.log.read_text())

    def test_cargo_deny_must_be_the_pinned_version(self):
        for version in ("0.20.1", ""):
            with self.subTest(version=version):
                result = self.check(DENY_VERSION=version)
                self.assertEqual(result.returncode, 1)
                self.assertIn("cargo-deny 0.20.2 is needed", result.stderr)
                self.assertNotIn("cargo llvm-cov", self.log.read_text())

    def test_without_the_network_dependencies_are_checked_against_the_cache(self):
        result = self.check(DENY_FETCH="1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("cached copy", result.stderr)
        self.assertIn("cargo deny --config .config/deny.toml --offline check", self.log.read_text())

    def test_npm_must_be_the_version_package_json_names(self):
        result = self.check(NPM_VERSION="11.19.0")
        self.assertEqual(result.returncode, 1)
        self.assertIn("npm 11.20.0 is required", result.stderr)

    def test_the_tree_is_installed_without_scripts_only_when_the_lockfile_changes(self):
        installs = self.root / "installs"
        self.assertEqual(self.check("--fast").returncode, 0)
        self.assertEqual(
            installs.read_text(),
            "npm ci --ignore-scripts --include=dev --no-audit --no-fund --loglevel=error\n",
        )
        self.check("--fast")
        self.assertEqual(len(installs.read_text().splitlines()), 1)
        write(self.root / "package-lock.json", '{"changed": true}\n')
        self.check("--fast")
        self.assertEqual(len(installs.read_text().splitlines()), 2)

    def test_a_failed_install_says_what_to_run(self):
        result = self.check("--fast", NPM_CI_FAILS="1")
        self.assertEqual(result.returncode, 1)
        self.assertIn("npm ci --ignore-scripts --include=dev", result.stderr)

    def test_a_node_modules_changed_by_hand_fails(self):
        result = self.check(NPM_LS="1")
        self.assertEqual(result.returncode, 1)
        self.assertIn("does not match package-lock.json", result.stderr)

    def test_a_pass_of_another_lockfile_or_older_than_7_days_does_not_count(self):
        self.assertEqual(self.check().returncode, 0)
        stamp = self.root / ".tools" / "npm-audit-pass"
        lock_sum, passed_at = stamp.read_text().split()
        for text in (f"{'0' * 64} {passed_at}", f"{lock_sum} {int(passed_at) - 7 * 86400 - 60}"):
            with self.subTest(stamp=text):
                stamp.write_text(text + "\n")
                result = self.check(NPM_CHECK="3")
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn("no passing check in 7 days", result.stderr)

    def test_without_the_registry_a_recent_pass_of_the_same_lockfile_counts(self):
        result = self.check(NPM_CHECK="3")
        self.assertEqual(result.returncode, 1)
        self.assertIn("no passing check in 7 days", result.stderr)
        self.assertEqual(self.check().returncode, 0)
        result = self.check(NPM_CHECK="3")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("using the last passing check", result.stderr)

    def test_a_failing_npm_check_fails(self):
        self.assertEqual(self.check(NPM_CHECK="1").returncode, 1)

    def test_the_front_end_is_built_before_anything_compiles(self):
        built = "crates/tetsu/assets/build/assets/tetsu-x.css"
        self.stub("cargo", f'[ -f {built} ] || exit 9\necho "cargo $*" >> "{self.log}"')
        result = self.check("--fast")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("vite build --config web/vite.config.ts", (self.root / "builds").read_text())

    def test_the_front_end_is_rebuilt_only_when_an_input_changed(self):
        builds = self.root / "builds"
        write(self.root / "web" / "src" / "tetsu.css", "@import 'tailwindcss';\n")
        self.assertEqual(self.check("--fast").returncode, 0)
        self.check("--fast")
        self.assertEqual(len(builds.read_text().splitlines()), 1)
        manifest = self.root / "crates" / "tetsu" / "assets" / "build" / ".vite" / "manifest.json"
        os.utime(manifest, (1, 1))
        self.check("--fast")
        self.assertEqual(len(builds.read_text().splitlines()), 2)

    def test_fast_mode_runs_only_formatters_and_linters(self):
        result = self.check("--fast")
        self.assertEqual(result.returncode, 0, result.stderr)
        log = self.log.read_text()
        for stage in (
            "cargo fmt --all --check",
            "cargo clippy",
            "ruff check",
            "ruff format --check",
            "shellcheck",
            "shfmt -d",
            "squawk",
            "sqlfluff lint",
            "markdownlint-cli2",
            "check_templates.py",
            "scan --staged",
        ):
            self.assertIn(stage, log)
        for slow in ("floor_guard.py", "scan --history", "unittest", "cargo llvm-cov", "cargo mutants"):
            self.assertNotIn(slow, log)

    def test_a_failing_secret_scan_fails_both_modes(self):
        write(self.root / "scripts" / "scan-secrets.sh", "#!/bin/sh\nexit 1\n", EXECUTABLE)
        for args in (("--fast",), ()):
            with self.subTest(args=args):
                self.assertNotEqual(self.check(*args).returncode, 0)

    def test_skips_squawk_when_there_are_no_migrations(self):
        shutil.rmtree(self.root / "crates")
        result = self.check("--fast")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("squawk", self.log.read_text())

    def test_fast_mode_needs_no_test_database(self):
        self.no_database = True
        result = self.check("--fast")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_passes_the_base_to_the_guard_and_the_mutation_diff(self):
        self.check("--base", "abc123")
        log = self.log.read_text()
        self.assertIn("floor_guard.py --base abc123", log)
        self.assertIn("--src-prefix=a/ --dst-prefix=b/ abc -- crates", log)

    def real_git_repo(self):
        """Makes the test root a repository with a base commit, then a commit that edits and copies Rust."""
        (self.bin / "git").unlink()
        self.stub("cargo", f'if [ "$1" = mutants ]; then cat "$3" >> "{self.log}"; fi')
        # Pin settings that would make the setup itself fail, such as signing or hooks.
        git = ["git", "-c", "user.email=t@example.com", "-c", "user.name=t"]
        git += ["-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null"]
        lib = self.root / "crates" / "a" / "src" / "lib.rs"
        write(lib, "pub fn a() {}\npub fn b() {}\n")
        for step in (["init", "-q"], ["add", "."], ["commit", "-q", "-m", "base"]):
            run([*git, *step], self.root, check=True)
        base = run(["git", "rev-parse", "HEAD"], self.root, check=True).stdout.strip()
        write(lib.with_name("copy.rs"), lib.read_text())
        write(lib, "pub fn a() {}\npub fn b() {}\npub fn c() {}\n")
        for step in (["add", "."], ["commit", "-q", "-m", "change"]):
            run([*git, *step], self.root, check=True)
        return base, lib

    def test_the_mutation_diff_ignores_personal_git_settings(self):
        # cargo-mutants reads only plain a/ and b/ diffs, so any other style tests nothing or too little.
        base, _ = self.real_git_repo()
        write(self.root / ".gitattributes", "*.rs diff=hostile\n")
        write(self.root / ".git" / "info" / "attributes", "copy.rs -diff\nlib.rs binary\n")
        for key, value in (
            ("diff.mnemonicPrefix", "true"),
            ("diff.noprefix", "true"),
            ("diff.srcPrefix", "SRC/"),
            ("diff.dstPrefix", "DST/"),
            ("diff.external", "true"),
            ("diff.hostile.textconv", "false"),
            ("diff.renames", "copies"),
            ("color.ui", "always"),
            ("color.diff", "always"),
        ):
            run(["git", "config", key, value], self.root, check=True)
        result = self.check("--base", base)
        self.assertEqual(result.returncode, 0, result.stderr)
        diff = self.log.read_text()
        self.assertIn("--- a/crates/a/src/lib.rs\n+++ b/crates/a/src/lib.rs\n", diff)
        self.assertIn("+pub fn c() {}\n", diff)
        self.assertIn("+++ b/crates/a/src/copy.rs\n@@ -0,0 +1,2 @@\n+pub fn a() {}\n+pub fn b() {}\n", diff)
        self.assertNotIn("\x1b[", diff)

    def test_a_runner_mutates_in_place_and_a_machine_mutates_copies(self):
        # A runner throws its checkout away, so it reuses target/ in place; a maintainer's checkout must never
        # be left mutated by a killed run.
        base, _ = self.real_git_repo()
        self.stub("cargo", f'if [ "$1" = mutants ]; then echo "$@" >> "{self.log}"; fi')
        runs = {}
        # The machine runs first: a pass the runner recorded would let it skip, as a runner never does.
        for name, ci in (("machine", ""), ("runner", "true")):
            self.log.write_text("")
            result = self.check("--base", base, CI=ci)
            self.assertEqual(result.returncode, 0, result.stderr)
            runs[name] = [line for line in self.log.read_text().splitlines() if "--baseline skip" in line]
        self.assertEqual(len(runs["runner"]), 1)
        self.assertIn("--in-place", runs["runner"][0])
        self.assertNotIn("--jobs", runs["runner"][0])
        self.assertEqual(len(runs["machine"]), 1)
        self.assertIn("--jobs 1", runs["machine"][0])
        self.assertNotIn("--in-place", runs["machine"][0])

    def test_committed_rust_undone_on_disk_fails_rather_than_testing_nothing(self):
        base, lib = self.real_git_repo()
        run(["git", "checkout", base, "--", "crates"], self.root, check=True)
        (lib.with_name("copy.rs")).unlink()
        result = self.check("--base", base)
        self.assertEqual(result.returncode, 1)
        self.assertIn("commits since", result.stderr)
        self.assertNotIn("+pub fn", self.log.read_text())

    def mutants_listed(self, count):
        """Makes `cargo mutants --list` report `count` mutants, and logs every cargo call."""
        self.stub(
            "cargo",
            f'echo "cargo $*" >> "{self.log}"; '
            f'case "$*" in *--list*) for i in $(seq {count}); do echo mutant; done;; esac',
        )

    def test_a_few_mutants_run_in_one_job(self):
        self.mutants_listed(5)
        self.check(CARGO_MUTANTS_JOBS="")
        self.assertIn("--jobs 1", self.log.read_text())

    def test_many_mutants_run_in_parallel_up_to_the_core_budget(self):
        self.mutants_listed(85)
        self.check(CARGO_MUTANTS_JOBS="", TETSU_CHECK_CORES="12")
        self.assertIn("--jobs 3", self.log.read_text())

    def slow_stages_ran(self):
        log = self.log.read_text()
        self.log.unlink()
        return "cargo llvm-cov --json" in log, "cargo mutants" in log

    def test_a_pass_is_reused_while_the_rust_is_unchanged(self):
        self.assertEqual(self.check().returncode, 0)
        self.assertEqual(self.slow_stages_ran(), (True, True))
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.slow_stages_ran(), (False, False))
        self.assertIn("coverage and mutation testing are skipped", result.stderr)

    def test_a_pass_is_reused_after_a_push_moves_the_base(self):
        # A push moves origin/main, but the same Rust and checks still give the same result, so a docs-only
        # push after it skips coverage rather than running it again.
        self.assertEqual(self.check(MERGE_BASE="abc").returncode, 0)
        self.assertEqual(self.slow_stages_ran(), (True, True))
        result = self.check(MERGE_BASE="def")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.slow_stages_ran(), (False, False))
        self.assertIn("coverage and mutation testing are skipped", result.stderr)

    def test_changed_rust_or_checks_run_everything_again(self):
        for changed in ("crates/a/migrations/0001.sql", "scripts/check_coverage.py", "Cargo.lock"):
            self.assertEqual(self.check().returncode, 0)
            self.slow_stages_ran()
            write(self.root / changed, f"changed {changed}\n")
            self.assertEqual(self.check().returncode, 0)
            self.assertEqual(self.slow_stages_ran(), (True, True), changed)

    def test_ci_never_reuses_a_machines_pass_when_rust_changed(self):
        self.assertEqual(self.check().returncode, 0)
        self.slow_stages_ran()
        self.assertEqual(self.check(CI="true", DIFF_QUIET="1").returncode, 0)
        self.assertEqual(self.slow_stages_ran(), (True, True))

    def test_ci_skips_each_group_unchanged_since_the_base_and_says_so(self):
        result = self.check("--base", "abc123", CI="true")
        self.assertEqual(result.returncode, 0, result.stderr)
        log = self.log.read_text()
        for skipped in (
            "parallel_unittest.py",
            "cargo llvm-cov",
            "--doc",
            "cargo mutants",
            "vitest run",
            "stryker run",
        ):
            self.assertNotIn(skipped, log)
        for always in ("floor_guard.py", "cargo clippy", "cargo deny", "lizard", "jscpd"):
            self.assertIn(always, log)
        for said in (
            "script tests are skipped",
            "coverage, the doc tests, and mutation testing",
            "front end",
        ):
            self.assertIn(said, result.stderr)
        # The Rust group asks about exactly the inputs the fingerprint reads.
        self.assertIn("diff --quiet abc -- crates tools Cargo.toml Cargo.lock rust-toolchain.toml", log)

    def test_ci_runs_a_group_whose_inputs_changed(self):
        result = self.check("--base", "abc123", CI="true", DIFF_QUIET="1")
        self.assertEqual(result.returncode, 0, result.stderr)
        log = self.log.read_text()
        for ran in ("parallel_unittest.py", "cargo llvm-cov", "vitest run", "stryker run"):
            self.assertIn(ran, log)

    def test_a_failed_run_records_no_pass(self):
        self.stub(
            "cargo",
            f'echo "cargo $*" >> "{self.log}"; [ "$1" = mutants ] && [ -n "${{FAIL-}}" ] && exit 1; exit 0',
        )
        self.assertNotEqual(self.check(FAIL="1").returncode, 0)
        self.slow_stages_ran()
        self.assertEqual(self.check().returncode, 0)
        self.assertEqual(self.slow_stages_ran(), (True, True))

    def test_a_job_is_added_per_fifteen_mutants(self):
        self.mutants_listed(30)
        self.check(CARGO_MUTANTS_JOBS="", TETSU_CHECK_CORES="24")
        self.assertIn("--jobs 2", self.log.read_text())

    def budgets(self, **env):
        """Runs a check whose tools log the parallelism they were given, and returns the log."""
        self.mutants_listed(85)
        self.stub(
            "cargo",
            f'echo "cargo $1 build=${{CARGO_BUILD_JOBS-}} threads=${{RUST_TEST_THREADS-}}" >> "{self.log}"; '
            'case "$*" in *--list*) for i in $(seq 85); do echo mutant; done;; esac',
        )
        self.stub("python3", f'echo "python $1 cores=${{TETSU_CHECK_CORES-}}" >> "{self.log}"')
        result = self.check(CARGO_MUTANTS_JOBS="", DIFF_QUIET="1", **env)
        self.assertEqual(result.returncode, 0, result.stderr)
        return self.log.read_text()

    def test_a_machine_check_uses_half_its_cores_by_default(self):
        # The rest stay free for whoever is using the machine while the check runs.
        half = max(1, (os.cpu_count() or 1) // 2)
        log = self.budgets(TETSU_CHECK_CORES="")
        self.assertIn(f"cargo clippy build={half} threads={half}", log)
        self.assertIn(f"python scripts/parallel_unittest.py cores={half}", log)

    def test_the_core_budget_can_be_set_and_every_stage_keeps_to_it(self):
        log = self.budgets(TETSU_CHECK_CORES="8")
        self.assertIn("cargo clippy build=8 threads=8", log)
        self.assertIn("cargo llvm-cov build=8 threads=8", log)
        self.assertIn("python scripts/parallel_unittest.py cores=8", log)
        self.assertIn("vitest run --coverage --config web/vitest.config.ts --maxWorkers=8", log)
        self.assertIn("stryker run .config/stryker.config.json --concurrency 8", log)
        # Two mutant jobs share the budget rather than each taking all of it.
        self.assertIn("cargo mutants build=4 threads=4", log)

    def test_ci_uses_every_core_a_runner_has(self):
        cores = os.cpu_count() or 1
        log = self.budgets(CI="true", TETSU_CHECK_CORES="")
        self.assertIn(f"cargo clippy build={cores} threads={cores}", log)

    def test_a_core_budget_that_is_not_a_positive_number_is_refused(self):
        for bad in ("0", "half", "-2"):
            with self.subTest(bad=bad):
                result = self.check(TETSU_CHECK_CORES=bad)
                self.assertEqual(result.returncode, 1)
                self.assertIn("TETSU_CHECK_CORES must be a whole number of cores, at least 1", result.stderr)

    def test_a_machine_check_runs_at_low_priority_and_ci_does_not(self):
        self.stub("renice", f'echo "renice $*" >> "{self.log}"')
        self.stub("ionice", f'echo "ionice $*" >> "{self.log}"')
        self.check()
        self.assertRegex(self.log.read_text(), r"renice -n 10 -p \d+")
        self.assertRegex(self.log.read_text(), r"ionice -c 2 -n 7 -p \d+")
        self.log.unlink()
        self.check(CI="true")
        self.assertNotIn("renice", self.log.read_text())

    def test_a_mutant_job_count_that_is_not_a_positive_number_is_refused(self):
        result = self.check(CARGO_MUTANTS_JOBS="0")
        self.assertEqual(result.returncode, 1)
        self.assertIn("CARGO_MUTANTS_JOBS must be a whole number of jobs, at least 1", result.stderr)

    def test_the_mutant_job_count_can_be_set(self):
        self.check(CARGO_MUTANTS_JOBS="3")
        self.assertIn("cargo mutants --in-diff", self.log.read_text())
        self.assertIn("--jobs 3", self.log.read_text())

    def test_skips_mutation_testing_when_no_rust_changed(self):
        self.stub("git", 'if [ "$1" = merge-base ]; then echo abc; fi')
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("cargo mutants", self.log.read_text())

    def test_drops_every_test_database_when_it_finishes(self):
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("cargo run -q -p tetsu-postgres --example drop_test_databases", self.log.read_text())

    def test_drops_every_test_database_even_after_a_failing_stage(self):
        self.stub("cargo", f'echo "cargo $*" >> "{self.log}"; [ "$1" != llvm-cov ]')
        result = self.check()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("cargo run -q -p tetsu-postgres --example drop_test_databases", self.log.read_text())

    def test_a_run_stopped_at_its_first_step_still_cleans_up(self):
        self.stub("node", "echo v10.15.3")
        result = self.check()
        self.assertEqual(result.returncode, 1)
        log = self.log.read_text()
        self.assertIn("python scripts/clean.py --routine", log)
        # No test Postgres has answered yet, so there is none to drop databases on.
        self.assertNotIn("drop_test_databases", log)
        self.assertNotIn("test databases were not dropped", result.stderr)

    def test_a_run_stopped_for_want_of_a_test_postgres_cleans_up_without_dropping(self):
        self.no_database = True
        result = self.check()
        self.assertEqual(result.returncode, 1)
        log = self.log.read_text()
        self.assertIn("python scripts/clean.py --routine", log)
        self.assertNotIn("drop_test_databases", log)
        self.assertNotIn("test databases were not dropped", result.stderr)

    def test_fast_mode_leaves_the_cleanup_to_full_runs(self):
        result = self.check("--fast")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("clean.py", self.log.read_text())

    def test_fails_when_the_final_sweep_cannot_drop_the_test_databases(self):
        # Only the sweep's run fails, not the template format check's.
        # POSIX shell, since CI's sh is dash.
        self.stub(
            "cargo", f'echo "cargo $*" >> "{self.log}"; case "$*" in *drop_test_databases*) exit 1;; esac'
        )
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("test databases were not dropped", result.stderr)

    def test_the_database_hint_uses_settings_for_throwaway_data(self):
        self.no_database = True
        result = self.check()
        for setting in ("fsync=off", "synchronous_commit=off", "full_page_writes=off"):
            self.assertIn(setting, result.stderr)
        # No password, so only this machine may connect; and nothing worth keeping, so nothing goes to disk.
        self.assertIn("-p 127.0.0.1:54329:5432", result.stderr)
        self.assertIn("--tmpfs /var/lib/postgresql", result.stderr)

    def test_ci_sets_every_server_setting_in_the_database_hint(self):
        self.no_database = True
        settings = re.findall(r"-c (\w+)=(\S+)", self.check().stderr)
        self.assertIn(("ssl", "on"), settings)
        workflow = (ROOT / ".github" / "workflows" / "ci.yml").read_text()
        for name, value in settings:
            with self.subTest(name=name):
                self.assertRegex(workflow, rf"ALTER SYSTEM SET {name} = '?{re.escape(value)}'?\"")

    def test_the_security_guide_starts_the_same_test_postgres(self):
        # The guide shows the command plainly, for people who never run the checks, so it must not drift.
        self.no_database = True
        hint = self.check().stderr
        guide = (ROOT / "docs" / "security" / "testing-with-an-agent.md").read_text()
        for part in re.findall(r"(-[cpe] \S+|--name \S+|--tmpfs \S+|postgres:[\d.]+)", hint):
            with self.subTest(part=part):
                self.assertIn(part, guide)

    def test_stops_on_a_failing_stage(self):
        self.stub("cargo", "exit 1")
        result = self.check()
        self.assertEqual(result.returncode, 1)
        # Nothing after the failing stage runs but the cleanup.
        self.assertEqual(self.log.read_text().strip(), "python scripts/clean.py --routine")

    def test_switches_node_through_nvm(self):
        self.stub("node", 'if [ -n "$NVM_SWITCHED" ]; then echo v26.1.0; else echo v10.15.3; fi')
        nvm = self.root / "nvm"
        write(nvm / "nvm.sh", "nvm() { export NVM_SWITCHED=1; }\n")
        result = self.check(nvm_dir=nvm)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_switches_node_through_mise(self):
        self.stub("node", "echo v10.15.3")
        installed = self.root / "mise-node"
        write(installed / "bin" / "node", "#!/bin/sh\necho v26.1.0\n", EXECUTABLE)
        # mise installs the release .nvmrc names, then says where it is.
        self.stub(
            "mise",
            f'echo "mise $*" >> "{self.log}"\n'
            'case "$*" in "install --quiet node@26") exit 0;; '
            f'"where node@26") echo "{installed}";; *) exit 1;; esac',
        )
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("mise install --quiet node@26", self.log.read_text())

    def test_a_mise_that_cannot_install_node_leaves_the_error(self):
        self.stub("node", "echo v10.15.3")
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("mise install node@26", result.stderr)

    def test_wrong_node_version_fails_with_a_fix(self):
        self.stub("node", "echo v10.15.3")
        result = self.check()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Node 26", result.stderr)

    def default_database(self, port):
        """Writes the local default address, as .cargo/config.toml does in the repository."""
        url = f"postgres://postgres@localhost:{port}/postgres"
        write(self.root / ".cargo" / "config.toml", f'[env]\nTEST_DATABASE_URL = "{url}"\n')
        return url

    def test_with_nothing_set_the_default_address_is_used_when_it_answers(self):
        listener = socket.create_server(("127.0.0.1", 0))
        self.addCleanup(listener.close)
        self.default_database(listener.getsockname()[1])
        self.stub("cargo", f'echo "cargo $* url=${{TEST_DATABASE_URL-unset}}" >> "{self.log}"')
        self.no_database = True
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        # Unset, not empty, so Cargo's own default applies to the tests.
        self.assertIn("url=unset", self.log.read_text())

    def test_a_default_address_with_nothing_listening_fails_with_a_fix(self):
        with socket.create_server(("127.0.0.1", 0)) as closed:
            port = closed.getsockname()[1]
        self.default_database(port)
        self.no_database = True
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn(f"no test Postgres answers at localhost:{port}", result.stderr)
        self.assertIn("docker run", result.stderr)

    def test_the_setup_hint_starts_postgres_at_the_default_address(self):
        self.no_database = True
        hint = self.check().stderr
        config = (ROOT / ".cargo" / "config.toml").read_text()
        default = re.search(r'^TEST_DATABASE_URL = "postgres://\w+@localhost:(\d+)/', config, re.MULTILINE)
        self.assertIsNotNone(default, config)
        self.assertIn(f"-p 127.0.0.1:{default.group(1)}:5432", hint)

    def test_an_explicit_address_that_answers_is_used_as_given(self):
        self.default_database(1)
        self.stub("cargo", f'echo "cargo $* url=${{TEST_DATABASE_URL-unset}}" >> "{self.log}"')
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("no test Postgres", result.stderr)
        self.assertIn(f"url={self.database_url}", self.log.read_text())

    def test_an_explicit_address_with_nothing_listening_stops_before_any_test(self):
        # Set in a shell profile, it once let a stopped Docker show up as failing tests.
        with socket.create_server(("127.0.0.1", 0)) as closed:
            port = closed.getsockname()[1]
        self.database_url = f"postgres://postgres:hunter2@127.0.0.1:{port}/postgres"
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn(f"no test Postgres answers at 127.0.0.1:{port}", result.stderr)
        self.assertNotIn("hunter2", result.stderr)
        self.assertIn("Docker", result.stderr)
        log = self.log.read_text() if self.log.exists() else ""
        self.assertNotIn("cargo llvm-cov", log)

    def test_an_explicit_address_naming_no_host_and_port_is_used_as_given(self):
        # A Unix socket named in the query, or Postgres's own default port: nothing here to try over TCP.
        for url in ("postgres:///postgres?host=/run/postgresql", "postgres://postgres@127.0.0.1/postgres"):
            with self.subTest(url=url):
                self.database_url = url
                result = self.check()
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_a_stopped_docker_is_named_as_a_likely_cause(self):
        self.no_database = True
        result = self.check()
        self.assertIn("If Docker is stopped, start it", result.stderr)

    def test_a_missing_test_database_fails_with_a_fix(self):
        self.no_database = True
        result = self.check()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("TEST_DATABASE_URL", result.stderr)
        self.assertIn("docker run", result.stderr)

    def test_only_the_nightly_pass_builds_in_its_own_folder(self):
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        log = self.log.read_text()
        self.assertRegex(log, r"(?m)^cargo \+nightly-2026-11-01 llvm-cov --branch .* dir=target/nightly$")
        self.assertRegex(log, r"(?m)^cargo llvm-cov --json .* dir=$")
        # Doc tests run on both: stable proves the examples on the toolchain we ship, and only nightly's
        # rustdoc checks the error code a compile_fail test names.
        self.assertRegex(log, r"(?m)^cargo test --doc dir=$")
        self.assertRegex(log, r"(?m)^cargo \+nightly-2026-11-01 test --doc dir=target/nightly$")

    def test_a_missing_nightly_fails_before_the_slow_stages(self):
        self.stub("rustup", "exit 1")
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("nightly-2026-11-01 with llvm-tools-preview is missing", result.stderr)
        self.assertIn("scripts/install-tools.sh", result.stderr)
        log = self.log.read_text() if self.log.exists() else ""
        for stage in ("cargo fmt", "cargo clippy", "floor_guard.py", "unittest", "cargo llvm-cov"):
            self.assertNotIn(stage, log)

    def test_a_nightly_without_its_coverage_tools_fails_too(self):
        self.stub("rustup", 'echo "rust-std-x86_64-unknown-linux-gnu"')
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("with llvm-tools-preview is missing", result.stderr)

    def test_fast_mode_does_not_need_the_nightly(self):
        self.stub("rustup", "exit 1")
        result = self.check("--fast")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_a_missing_tool_fails_with_a_fix(self):
        (self.root / ".tools" / "bin" / "lizard").unlink()
        result = self.check()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("scripts/install-tools.sh", result.stderr)


# What the pinned Mailpit prints for its version.
MAILPIT = "#!/bin/sh\necho 'mailpit v1.34.0 compiled with go1.27.4 on linux/amd64'\n"


class InstallTools(unittest.TestCase):
    def test_pins_every_tool(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            log = root / "log"
            (root / "scripts").mkdir()
            shutil.copy(ROOT / "scripts" / "install-tools.sh", root / "scripts")
            write(root / ".config/nightly-toolchain", "nightly-2026-11-01\n")
            write(root / ".tools" / "bin" / "betterleaks", "#!/bin/sh\necho 1.10.0\n", EXECUTABLE)
            write(root / ".tools" / "bin" / "mailpit", MAILPIT, EXECUTABLE)
            for tool in ("cargo", "uv", "rustup", "curl"):
                write(root / "bin" / tool, f'#!/bin/sh\necho "{tool} $*" >> "{log}"\n', EXECUTABLE)
            result = run([root / "scripts" / "install-tools.sh"], root, PATH=f"{root / 'bin'}:/usr/bin:/bin")
            self.assertEqual(result.returncode, 0, result.stderr)
            text = log.read_text()
            self.assertRegex(text, r"cargo install --locked --version \S+ cargo-llvm-cov")
            self.assertRegex(text, r"cargo install --locked --version \S+ cargo-mutants")
            self.assertRegex(text, r"uv venv .*--python 3\.\d+ .*\.tools")
            for tool in ("lizard", "ruff", "squawk-cli", "shellcheck-py", "shfmt-py", "sqlfluff"):
                self.assertRegex(text, rf"uv pip install .*{tool}==\S+")
            nightly = (
                "rustup toolchain install nightly-2026-11-01 --profile minimal --component llvm-tools-preview"
            )
            self.assertIn(nightly, text)
            self.assertNotIn("rustup default none", text)
            self.assertNotIn("curl", text, "a scanner already at the pinned version is not downloaded again")

    def test_the_nightly_never_becomes_the_default_toolchain(self):
        """rustup makes the first toolchain it installs the default when none is set, so that is undone."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            log = root / "log"
            (root / "scripts").mkdir()
            shutil.copy(ROOT / "scripts" / "install-tools.sh", root / "scripts")
            write(root / ".config/nightly-toolchain", "nightly-2026-11-01\n")
            write(root / ".tools" / "bin" / "betterleaks", "#!/bin/sh\necho 1.10.0\n", EXECUTABLE)
            write(root / ".tools" / "bin" / "mailpit", MAILPIT, EXECUTABLE)
            for tool in ("cargo", "uv"):
                write(root / "bin" / tool, "#!/bin/sh\n", EXECUTABLE)
            no_default = f'[ "$1" = default ] && [ -z "$2" ] && exit 1\necho "rustup $*" >> "{log}"'
            write(root / "bin" / "rustup", f"#!/bin/sh\n{no_default}\n", EXECUTABLE)
            result = run([root / "scripts" / "install-tools.sh"], root, PATH=f"{root / 'bin'}:/usr/bin:/bin")
            self.assertEqual(result.returncode, 0, result.stderr)
            text = log.read_text()
            # Unset after the install, which is what set it.
            self.assertLess(text.index("rustup toolchain install"), text.index("rustup default none"))

    def test_a_scanner_download_that_fails_its_checksum_is_not_installed(self):
        """A missing or outdated scanner is downloaded, from the pinned release, and installed only if it
        matches the SHA-256 in the script."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            log = root / "log"
            (root / "scripts").mkdir()
            shutil.copy(ROOT / "scripts" / "install-tools.sh", root / "scripts")
            write(root / ".config/nightly-toolchain", "nightly-2026-11-01\n")
            write(root / ".tools" / "bin" / "betterleaks", "#!/bin/sh\necho 1.9.0\n", EXECUTABLE)
            for tool in ("cargo", "uv", "rustup"):
                write(root / "bin" / tool, "#!/bin/sh\n", EXECUTABLE)
            # Writes a file that is not the release, wherever -o points.
            curl = f'echo "curl $*" >> "{log}"\nwhile [ "$1" != -o ]; do shift; done\necho tampered > "$2"'
            write(root / "bin" / "curl", f"#!/bin/sh\n{curl}\n", EXECUTABLE)
            result = run([root / "scripts" / "install-tools.sh"], root, PATH=f"{root / 'bin'}:/usr/bin:/bin")
            self.assertEqual(result.returncode, 1)
            self.assertIn("does not match its published SHA-256", result.stderr)
            self.assertRegex(
                log.read_text(),
                r"releases/download/v1\.10\.0/betterleaks_1\.10\.0_(linux|darwin)_(x64|arm64)\.tar\.gz",
            )
            self.assertIn("1.9.0", (root / ".tools" / "bin" / "betterleaks").read_text())

    def test_a_mail_server_download_that_fails_its_checksum_is_not_installed(self):
        """Mailpit, for the SMTP tests, is downloaded from its pinned release only when missing or outdated,
        and installed only if it matches the SHA-256 in the script (ADR-0074)."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            log = root / "log"
            (root / "scripts").mkdir()
            shutil.copy(ROOT / "scripts" / "install-tools.sh", root / "scripts")
            write(root / ".config/nightly-toolchain", "nightly-2026-11-01\n")
            write(root / ".tools" / "bin" / "betterleaks", "#!/bin/sh\necho 1.10.0\n", EXECUTABLE)
            old = "#!/bin/sh\necho 'mailpit v1.33.0 compiled with go1.27.4 on linux/amd64'\n"
            write(root / ".tools" / "bin" / "mailpit", old, EXECUTABLE)
            for tool in ("cargo", "uv", "rustup"):
                write(root / "bin" / tool, "#!/bin/sh\n", EXECUTABLE)
            curl = f'echo "curl $*" >> "{log}"\nwhile [ "$1" != -o ]; do shift; done\necho tampered > "$2"'
            write(root / "bin" / "curl", f"#!/bin/sh\n{curl}\n", EXECUTABLE)
            result = run([root / "scripts" / "install-tools.sh"], root, PATH=f"{root / 'bin'}:/usr/bin:/bin")
            self.assertEqual(result.returncode, 1)
            self.assertIn("the Mailpit download does not match its published SHA-256", result.stderr)
            self.assertRegex(
                log.read_text(),
                r"axllent/mailpit/releases/download/v1\.34\.0/mailpit-(linux|darwin)-(amd64|arm64)\.tar\.gz",
            )
            self.assertIn("v1.33.0", (root / ".tools" / "bin" / "mailpit").read_text())


class PrePushHook(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        run(["git", "init", "-q"], self.root)
        (self.root / ".githooks").mkdir()
        shutil.copy(ROOT / ".githooks" / "pre-push", self.root / ".githooks")
        self.pushed = self.root / "pushed"
        scan = f'[ "$1" = --push ] || exit 5\necho "$2" > "{self.pushed}"\ncat >> "{self.pushed}"'
        write(self.root / "scripts" / "scan-secrets.sh", f"#!/bin/sh\n{scan}\n", EXECUTABLE)

    def test_hook_fails_when_the_script_fails(self):
        write(self.root / "scripts" / "check.sh", "#!/bin/sh\nexit 3\n", EXECUTABLE)
        result = run([self.root / ".githooks" / "pre-push"], self.root)
        self.assertEqual(result.returncode, 3)

    def test_pre_commit_runs_the_fast_checks(self):
        shutil.copy(ROOT / ".githooks" / "pre-commit", self.root / ".githooks")
        write(self.root / "scripts" / "check.sh", '#!/bin/sh\n[ "$1" = --fast ] || exit 4\n', EXECUTABLE)
        result = run([self.root / ".githooks" / "pre-commit"], self.root)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_hook_passes_when_the_script_passes(self):
        write(self.root / "scripts" / "check.sh", "#!/bin/sh\nexit 0\n", EXECUTABLE)
        result = run([self.root / ".githooks" / "pre-push"], self.root)
        self.assertEqual(result.returncode, 0)

    def test_hook_scans_the_refs_git_pushes(self):
        write(self.root / "scripts" / "check.sh", "#!/bin/sh\nexit 0\n", EXECUTABLE)
        refs = f"refs/heads/main {'a' * 40} refs/heads/main {'b' * 40}\n"
        result = run(
            [self.root / ".githooks" / "pre-push", "origin", "git@example.com:o/r"], self.root, input=refs
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.pushed.read_text(), "origin\n" + refs)

    def test_hook_fails_and_skips_the_checks_when_the_scan_fails(self):
        write(self.root / "scripts" / "scan-secrets.sh", "#!/bin/sh\nexit 1\n", EXECUTABLE)
        write(self.root / "scripts" / "check.sh", f'#!/bin/sh\ntouch "{self.root / "checked"}"\n', EXECUTABLE)
        result = run([self.root / ".githooks" / "pre-push"], self.root)
        self.assertEqual(result.returncode, 1)
        self.assertFalse((self.root / "checked").exists())


if __name__ == "__main__":
    unittest.main()
