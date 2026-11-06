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
        self.stub("git", 'if [ "$1" = merge-base ]; then echo abc; else echo "$@" >> "' + str(self.log) + '"; echo changed; fi')
        write(self.root / ".tools" / "bin" / "lizard", f'#!/bin/sh\necho "lizard $*" >> "{self.log}"\n', EXECUTABLE)
        write(self.root / "scripts" / "check-adrs.sh", f'#!/bin/sh\necho adrs >> "{self.log}"\n', EXECUTABLE)

    no_database = False

    def stub(self, name, body):
        write(self.bin / name, f"#!/bin/sh\n{body}\n", EXECUTABLE)

    def check(self, *args, nvm_dir=None):
        return run([self.root / "scripts" / "check.sh", *args], self.root,
                   PATH=f"{self.bin}:/usr/bin:/bin", NVM_DIR=str(nvm_dir or self.root / "no-nvm"),
                   TEST_DATABASE_URL="" if self.no_database else "postgres://test")

    def test_runs_every_stage(self):
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        log = self.log.read_text()
        for stage in ("floor_guard.py", "unittest", "adrs", "check_doc_sizes.py", "markdownlint-cli2",
                      "cargo fmt --all --check", "cargo clippy", "cargo llvm-cov --json", "cargo metadata",
                      "check_coverage.py", "lizard", "jscpd",
                      "cargo mutants"):
            self.assertIn(stage, log)

    def test_passes_the_base_to_the_guard_and_the_mutation_diff(self):
        self.check("--base", "abc123")
        log = self.log.read_text()
        self.assertIn("floor_guard.py --base abc123", log)
        self.assertIn("diff abc -- crates", log)

    def test_skips_mutation_testing_when_no_rust_changed(self):
        self.stub("git", 'if [ "$1" = merge-base ]; then echo abc; fi')
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("cargo mutants", self.log.read_text())

    def test_stops_on_a_failing_stage(self):
        self.stub("python3", "exit 1")
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
            pip = f'#!/bin/sh\necho "pip $*" >> "{log}"\n'
            write(root / "bin" / "cargo", f'#!/bin/sh\necho "cargo $*" >> "{log}"\n', EXECUTABLE)
            write(root / "bin" / "python3", f'#!/bin/sh\nmkdir -p .tools/bin\nprintf \'{pip}\' > .tools/bin/pip\nchmod +x .tools/bin/pip\n', EXECUTABLE)
            result = run([root / "scripts" / "install-tools.sh"], root, PATH=f"{root / 'bin'}:/usr/bin:/bin")
            self.assertEqual(result.returncode, 0, result.stderr)
            text = log.read_text()
            self.assertRegex(text, r"cargo install --locked --version \S+ cargo-llvm-cov")
            self.assertRegex(text, r"cargo install --locked --version \S+ cargo-mutants")
            self.assertRegex(text, r"pip install .*lizard==\S+")


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

    def test_hook_passes_when_the_script_passes(self):
        write(self.root / "scripts" / "check.sh", "#!/bin/sh\nexit 0\n", EXECUTABLE)
        result = run([self.root / ".githooks" / "pre-push"], self.root)
        self.assertEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
