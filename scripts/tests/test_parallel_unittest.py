"""Tests for scripts/parallel_unittest.py."""

import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

RUNNER = Path(__file__).resolve().parents[1] / "parallel_unittest.py"


class ParallelUnittest(unittest.TestCase):
    def folder(self, tests):
        """A throwaway package of test files, each holding the given test body."""
        root = Path(tempfile.mkdtemp())
        (root / "pkg").mkdir()
        (root / "pkg" / "__init__.py").write_text("")
        for name, body in tests.items():
            (root / "pkg" / f"test_{name}.py").write_text(
                "import time, unittest\n"
                f"class T(unittest.TestCase):\n    def test_it(self):\n        {body}\n"
            )
        return root

    def run_on(self, root, **env):
        return subprocess.run(
            [sys.executable, RUNNER, "pkg", "."],
            cwd=root,
            capture_output=True,
            text=True,
            check=False,
            env={**os.environ, "TETSU_CHECK_CORES": "", **env},
        )

    def test_passing_tests_pass_and_are_counted(self):
        result = self.run_on(self.folder({"a": "pass", "b": "pass"}))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("ran 2 tests, 0 failed", result.stdout)

    def test_a_failure_fails_the_run_and_shows_only_its_own_output(self):
        root = self.folder({"good": "print('quiet')", "bad": "print('loud'); self.fail('broke')"})
        result = self.run_on(root)
        self.assertEqual(result.returncode, 1)
        self.assertIn("FAILED pkg.test_bad.T.test_it", result.stderr)
        self.assertIn("broke", result.stderr)
        self.assertNotIn("quiet", result.stderr + result.stdout)
        self.assertIn("ran 2 tests, 1 failed", result.stdout)

    def test_tests_run_at_the_same_time(self):
        root = self.folder(dict.fromkeys("abcd", "time.sleep(1)"))
        started = time.monotonic()
        result = self.run_on(root)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertLess(time.monotonic() - started, 3.5, "four one-second tests ran one after another")

    def test_no_more_run_at_once_than_the_core_budget(self):
        root = self.folder(dict.fromkeys("abcd", "time.sleep(1)"))
        started = time.monotonic()
        result = self.run_on(root, TETSU_CHECK_CORES="2")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertGreaterEqual(time.monotonic() - started, 2, "more than two tests ran at a time")

    def test_finding_no_tests_is_an_error(self):
        result = self.run_on(self.folder({}))
        self.assertEqual(result.returncode, 1)
        self.assertIn("no tests found", result.stderr)

    def test_the_wrong_arguments_show_usage(self):
        result = subprocess.run([sys.executable, RUNNER], capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 2)
        self.assertIn("Usage", result.stderr)


if __name__ == "__main__":
    unittest.main()
