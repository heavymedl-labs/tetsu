"""check.sh must run each stage and stop on the first failure, and the hook must call it."""

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

    def check(self, *args, nvm_dir=None):
        return run(
            [self.root / "scripts" / "check.sh", *args],
            self.root,
            PATH=f"{self.bin}:/usr/bin:/bin",
            NVM_DIR=str(nvm_dir or self.root / "no-nvm"),
            TEST_DATABASE_URL="" if self.no_database else "postgres://test",
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

    def test_the_mutation_diff_ignores_personal_git_settings(self):
        # cargo-mutants reads only plain a/ and b/ diffs, so any other style tests nothing.
        (self.bin / "git").unlink()
        self.stub("cargo", f'if [ "$1" = mutants ]; then cat "$3" >> "{self.log}"; fi')
        git = ["git", "-c", "user.email=t@example.com", "-c", "user.name=t"]
        run([*git, "init", "-q"], self.root)
        run([*git, "commit", "-q", "--allow-empty", "-m", "base"], self.root)
        base = run(["git", "rev-parse", "HEAD"], self.root).stdout.strip()
        write(self.root / "crates" / "a" / "src" / "lib.rs", "pub fn a() {}\n")
        write(self.root / ".gitattributes", "*.rs diff=hostile\n")
        run([*git, "add", "."], self.root)
        run([*git, "commit", "-q", "-m", "change"], self.root)
        for key, value in (
            ("diff.mnemonicPrefix", "true"),
            ("diff.noprefix", "true"),
            ("diff.srcPrefix", "SRC/"),
            ("diff.dstPrefix", "DST/"),
            ("diff.external", "true"),
            ("diff.hostile.textconv", "false"),
            ("color.ui", "always"),
            ("color.diff", "always"),
        ):
            run(["git", "config", key, value], self.root)
        result = self.check("--base", base)
        self.assertEqual(result.returncode, 0, result.stderr)
        diff = self.log.read_text()
        self.assertIn("+++ b/crates/a/src/lib.rs\n@@ -0,0 +1 @@\n+pub fn a() {}\n", diff)
        self.assertNotIn("\x1b[", diff)

    def test_skips_mutation_testing_when_no_rust_changed(self):
        self.stub("git", 'if [ "$1" = merge-base ]; then echo abc; fi')
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("cargo mutants", self.log.read_text())

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
