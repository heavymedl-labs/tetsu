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
    'if [ "$1 $2" = "deny fetch" ]; then exit "${DENY_FETCH-0}"; fi\n'
)


class CheckScript(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.log = self.root / "log"
        (self.root / "scripts").mkdir()
        shutil.copy(ROOT / "scripts" / "check.sh", self.root / "scripts")
        write(self.root / ".nightly-toolchain", "nightly-2026-11-01\n")
        write(self.root / ".nvmrc", "26\n")
        self.bin = self.root / "bin"
        self.stub("node", "echo v26.1.0")
        for tool in ("npx", "cargo", "cargo-llvm-cov", "cargo-mutants"):
            self.stub(tool, f'echo "{tool} $*" >> "{self.log}"')
        # Cargo also logs its build folder, which the nightly pass keeps apart.
        self.stub("cargo", f'echo "cargo $* dir=${{CARGO_TARGET_DIR-}}" >> "{self.log}"')
        self.stub("rustup", 'echo "llvm-tools-x86_64-unknown-linux-gnu"')
        self.stub("python3", f'echo "python $1 $2 $3" >> "{self.log}"')
        self.stub(
            "git",
            'if [ "$1" = merge-base ]; then echo abc; else echo "$@" >> "'
            + str(self.log)
            + '"; echo changed; fi',
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
        write(self.root / "scripts" / "scan_secrets.sh", scan, EXECUTABLE)
        write(self.root / ".tools" / "bin" / "betterleaks", "#!/bin/sh\n", EXECUTABLE)

    no_database = False

    def stub(self, name, body):
        prefix = CARGO_DENY if name == "cargo" else ""
        write(self.bin / name, f"#!/bin/sh\n{prefix}{body}\n", EXECUTABLE)

    def check(self, *args, nvm_dir=None, **env):
        return run(
            [self.root / "scripts" / "check.sh", *args],
            self.root,
            PATH=f"{self.bin}:/usr/bin:/bin",
            NVM_DIR=str(nvm_dir or self.root / "no-nvm"),
            TEST_DATABASE_URL="" if self.no_database else "postgres://test",
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
            "cargo deny check --hide-inclusion-graph -D advisory-not-detected",
            "check_dated_entries.py deny.toml advisories",
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
        self.assertIn("cargo deny --offline check", self.log.read_text())

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
            "scan --staged",
        ):
            self.assertIn(stage, log)
        for slow in ("floor_guard.py", "scan --history", "unittest", "cargo llvm-cov", "cargo mutants"):
            self.assertNotIn(slow, log)

    def test_a_failing_secret_scan_fails_both_modes(self):
        write(self.root / "scripts" / "scan_secrets.sh", "#!/bin/sh\nexit 1\n", EXECUTABLE)
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

    def test_many_mutants_run_in_parallel_up_to_the_machine(self):
        self.mutants_listed(85)
        self.check(CARGO_MUTANTS_JOBS="")
        cores = os.cpu_count() or 1
        self.assertIn(f"--jobs {max(1, min(3, cores // 4))}", self.log.read_text())

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

    def test_fails_when_the_final_sweep_cannot_drop_the_test_databases(self):
        self.stub("cargo", f'echo "cargo $*" >> "{self.log}"; [ "$1" != run ]')
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

    def test_stops_on_a_failing_stage(self):
        self.stub("cargo", "exit 1")
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertFalse(self.log.exists())

    def test_switches_node_through_nvm(self):
        self.stub("node", 'if [ -n "$NVM_SWITCHED" ]; then echo v26.1.0; else echo v10.15.3; fi')
        nvm = self.root / "nvm"
        write(nvm / "nvm.sh", "nvm() { export NVM_SWITCHED=1; }\n")
        result = self.check(nvm_dir=nvm)
        self.assertEqual(result.returncode, 0, result.stderr)

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
        url = self.default_database(port)
        self.no_database = True
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn(f"no test Postgres answers at {url}", result.stderr)
        self.assertIn("docker run", result.stderr)

    def test_the_setup_hint_starts_postgres_at_the_default_address(self):
        self.no_database = True
        hint = self.check().stderr
        config = (ROOT / ".cargo" / "config.toml").read_text()
        default = re.search(r'^TEST_DATABASE_URL = "postgres://\w+@localhost:(\d+)/', config, re.MULTILINE)
        self.assertIsNotNone(default, config)
        self.assertIn(f"-p 127.0.0.1:{default.group(1)}:5432", hint)

    def test_an_explicit_address_is_used_as_given(self):
        self.default_database(1)
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("no test Postgres", result.stderr)

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


class InstallTools(unittest.TestCase):
    def test_pins_every_tool(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            log = root / "log"
            (root / "scripts").mkdir()
            shutil.copy(ROOT / "scripts" / "install-tools.sh", root / "scripts")
            write(root / ".nightly-toolchain", "nightly-2026-11-01\n")
            write(root / ".tools" / "bin" / "betterleaks", "#!/bin/sh\necho 1.10.0\n", EXECUTABLE)
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
            write(root / ".nightly-toolchain", "nightly-2026-11-01\n")
            write(root / ".tools" / "bin" / "betterleaks", "#!/bin/sh\necho 1.10.0\n", EXECUTABLE)
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
            write(root / ".nightly-toolchain", "nightly-2026-11-01\n")
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
        write(self.root / "scripts" / "scan_secrets.sh", f"#!/bin/sh\n{scan}\n", EXECUTABLE)

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
        write(self.root / "scripts" / "scan_secrets.sh", "#!/bin/sh\nexit 1\n", EXECUTABLE)
        write(self.root / "scripts" / "check.sh", f'#!/bin/sh\ntouch "{self.root / "checked"}"\n', EXECUTABLE)
        result = run([self.root / ".githooks" / "pre-push"], self.root)
        self.assertEqual(result.returncode, 1)
        self.assertFalse((self.root / "checked").exists())


if __name__ == "__main__":
    unittest.main()
