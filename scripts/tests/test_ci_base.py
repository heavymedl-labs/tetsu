"""The base a CI run checks since: given, found from the last successful manual run, or refused."""

import shutil
import tempfile
import unittest
from pathlib import Path

from scripts.tests.support import ROOT, run, write

SCRIPT = ROOT / "scripts" / "ci-base.sh"
HEAD = "a" * 40
EARLIER = "b" * 40


class CiBaseTest(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root)
        self.output = self.root / "output"
        self.output.write_text("")
        self.found = ""

    def base(self, **env):
        # gh answers with the head of the last successful manual run, as its --jq would print it.
        write(self.root / "bin" / "gh", f"#!/bin/sh\necho '{self.found}'\n", 0o755)
        values = {"PR_BASE": "", "INPUT_BASE": "", "GITHUB_SHA": HEAD, "GITHUB_REPOSITORY": "o/r", **env}
        return run(
            [SCRIPT],
            self.root,
            PATH=f"{self.root / 'bin'}:/usr/bin:/bin",
            GITHUB_OUTPUT=self.output,
            **values,
        )

    def test_a_given_base_is_used_and_is_not_the_runs_own_commit(self):
        result = self.base(INPUT_BASE=EARLIER)
        self.assertEqual((result.returncode, self.output.read_text()), (0, f"sha={EARLIER}\nsame=false\n"))

    def test_a_pull_requests_base_comes_before_any_given_one(self):
        self.base(PR_BASE=EARLIER, INPUT_BASE="c" * 40)
        self.assertEqual(self.output.read_text(), f"sha={EARLIER}\nsame=false\n")

    def test_with_none_given_it_is_the_last_successful_manual_runs_commit(self):
        self.found = EARLIER
        self.base()
        self.assertEqual(self.output.read_text(), f"sha={EARLIER}\nsame=false\n")

    def test_a_base_that_is_the_runs_own_commit_has_nothing_to_check(self):
        self.found = HEAD
        result = self.base()
        self.assertEqual(self.output.read_text(), f"sha={HEAD}\nsame=true\n")
        self.assertIn("nothing to check", result.stdout)

    def test_no_base_at_all_fails_rather_than_check_too_little(self):
        result = self.base()
        self.assertEqual((result.returncode, self.output.read_text()), (1, ""))
        self.assertIn("No successful manual run", result.stdout)

    def test_a_base_that_is_not_a_full_commit_sha_never_reaches_git(self):
        for base in ["--output=/tmp/x", "abc123", "A" * 40]:
            result = self.base(INPUT_BASE=base)
            self.assertEqual((result.returncode, self.output.read_text()), (1, ""), base)
