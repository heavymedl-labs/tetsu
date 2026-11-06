"""The size check must fail a long AGENTS.md and only warn on other long docs."""
import tempfile
import unittest
from pathlib import Path

from scripts.tests.support import ROOT, run, write

SCRIPT = ROOT / "scripts" / "check_doc_sizes.py"


class CheckDocSizes(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        run(["git", "init", "-q"], self.root)
        write(self.root / "AGENTS.md", "line\n" * 100)

    def check(self):
        run(["git", "add", "-A"], self.root)
        return run(["python3", SCRIPT], self.root)

    def test_agents_at_the_limit_passes(self):
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr, "")

    def test_agents_over_the_limit_fails(self):
        write(self.root / "AGENTS.md", "line\n" * 101)
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("AGENTS.md has 101 lines", result.stderr)

    def test_a_long_doc_only_warns(self):
        write(self.root / "docs" / "long.md", "x" * 12_004)
        result = self.check()
        self.assertEqual(result.returncode, 0)
        self.assertIn("docs/long.md", result.stderr)

    def test_a_short_doc_does_not_warn(self):
        write(self.root / "docs" / "short.md", "x" * 11_996)
        self.assertEqual(self.check().stderr, "")

    def test_a_missing_agents_file_fails(self):
        (self.root / "AGENTS.md").unlink()
        self.assertEqual(self.check().returncode, 1)


if __name__ == "__main__":
    unittest.main()
