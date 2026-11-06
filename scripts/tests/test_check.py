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
        self.stub("node", 'echo v26.1.0')
        self.stub("npx", f'echo lint >> "{self.log}"')
        self.stub("python3", f'echo "python $1 $2 $3" >> "{self.log}"')
        write(self.root / "scripts" / "check-adrs.sh", f'#!/bin/sh\necho adrs >> "{self.log}"\n', EXECUTABLE)

    def stub(self, name, body):
        write(self.bin / name, f"#!/bin/sh\n{body}\n", EXECUTABLE)

    def check(self, *args):
        return run([self.root / "scripts" / "check.sh", *args], self.root,
                   PATH=f"{self.bin}:/usr/bin:/bin", NVM_DIR=str(self.root / "no-nvm"))

    def test_runs_every_stage(self):
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        log = self.log.read_text()
        for stage in ("floor_guard.py", "unittest", "adrs", "check_doc_sizes.py", "lint"):
            self.assertIn(stage, log)

    def test_passes_arguments_to_the_guard(self):
        self.check("--base", "abc123")
        self.assertIn("floor_guard.py --base abc123", self.log.read_text())

    def test_stops_on_a_failing_stage(self):
        self.stub("python3", "exit 1")
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertFalse(self.log.exists())

    def test_switches_node_through_nvm(self):
        self.stub("node", 'if [ -n "$NVM_SWITCHED" ]; then echo v26.1.0; else echo v10.15.3; fi')
        nvm = self.root / "nvm"
        write(nvm / "nvm.sh", 'nvm() { export NVM_SWITCHED=1; }\n')
        result = run([self.root / "scripts" / "check.sh"], self.root,
                     PATH=f"{self.bin}:/usr/bin:/bin", NVM_DIR=str(nvm))
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_wrong_node_version_fails_with_a_fix(self):
        self.stub("node", "echo v10.15.3")
        result = self.check()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Node 26", result.stderr)


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
