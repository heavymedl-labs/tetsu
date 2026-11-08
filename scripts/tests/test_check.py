"""check.sh must run each stage and stop on the first failure, and the hook must call it."""

import os
import re
import shutil
import stat
import tempfile
import unittest
from pathlib import Path

from scripts.tests.support import ROOT, run, write

EXECUTABLE = stat.S_IRWXU


class CheckScript(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.log = self.root / "log"
        (self.root / "scripts").mkdir()
        shutil.copy(ROOT / "scripts" / "check.sh", self.root / "scripts")
        write(self.root / ".nvmrc", "26\n")
        self.bin = self.root / "bin"
        self.stub("node", "echo v26.1.0")
        for tool in ("npx", "cargo", "cargo-llvm-cov", "cargo-mutants"):
            self.stub(tool, f'echo "{tool} $*" >> "{self.log}"')
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

    no_database = False

    def stub(self, name, body):
        write(self.bin / name, f"#!/bin/sh\n{body}\n", EXECUTABLE)

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
            "unittest",
            "adrs",
            "check_doc_sizes.py",
            "markdownlint-cli2",
            "cargo fmt --all --check",
            "cargo clippy",
            "cargo llvm-cov --json",
            "cargo metadata",
            "check_coverage.py",
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
        ):
            self.assertIn(stage, log)
        for slow in ("floor_guard.py", "unittest", "cargo llvm-cov", "cargo mutants"):
            self.assertNotIn(slow, log)

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

    def test_a_missing_test_database_fails_with_a_fix(self):
        self.no_database = True
        result = self.check()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("TEST_DATABASE_URL", result.stderr)
        self.assertIn("docker run", result.stderr)

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
            for tool in ("cargo", "uv"):
                write(root / "bin" / tool, f'#!/bin/sh\necho "{tool} $*" >> "{log}"\n', EXECUTABLE)
            result = run([root / "scripts" / "install-tools.sh"], root, PATH=f"{root / 'bin'}:/usr/bin:/bin")
            self.assertEqual(result.returncode, 0, result.stderr)
            text = log.read_text()
            self.assertRegex(text, r"cargo install --locked --version \S+ cargo-llvm-cov")
            self.assertRegex(text, r"cargo install --locked --version \S+ cargo-mutants")
            self.assertRegex(text, r"uv venv .*--python 3\.\d+ .*\.tools")
            for tool in ("lizard", "ruff", "squawk-cli", "shellcheck-py", "shfmt-py", "sqlfluff"):
                self.assertRegex(text, rf"uv pip install .*{tool}==\S+")


class PrePushHook(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        run(["git", "init", "-q"], self.root)
        (self.root / ".githooks").mkdir()
        shutil.copy(ROOT / ".githooks" / "pre-push", self.root / ".githooks")

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


if __name__ == "__main__":
    unittest.main()
