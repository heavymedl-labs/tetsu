"""Checking the board names each issue that breaks CONTRIBUTING.md's board rules, and passes a tidy board."""

import json
import stat
import tempfile
import unittest
from pathlib import Path

from scripts.tests.support import ROOT, run, write

SCRIPT = ROOT / "scripts" / "check_board.py"


def issue(number, **given):
    """An issue as the query returns it: an open Task under #1 with an area, unless `given` says otherwise."""
    wanted = {"state": "OPEN", "kind": "Task", "milestone": "1. First", "labels": ("task", "area: server")}
    wanted |= {"parent": 1} | given
    return {
        "number": number,
        "title": f"Issue {number}",
        "state": wanted["state"],
        "issueType": {"name": wanted["kind"]} if wanted["kind"] else None,
        "milestone": {"title": wanted["milestone"]} if wanted["milestone"] else None,
        "labels": {"nodes": [{"name": label} for label in wanted["labels"]]},
        "parent": {"number": wanted["parent"]} if wanted["parent"] else None,
    }


def item(number, status="Todo", tier="2", priority="Next"):
    found = {"content": {"number": number, "type": "Issue"}, "status": status}
    if tier:
        found["tier"] = tier
    if priority:
        found["priority"] = priority
    return found


FEATURE = issue(1, kind="Feature", labels=(), parent=None)
TIDY = {
    "issues": [FEATURE, issue(2), issue(3, state="CLOSED")],
    "items": [item(1), item(2), item(3, status="Done")],
}
# Answers the two reads the script makes from files a test writes, and fails when GH_FAIL names one.
STUB = """#!/usr/bin/env python3
import json, os, sys
args = sys.argv[1:]
if os.environ.get("GH_FAIL") and os.environ["GH_FAIL"] in args:
    print("gh: refused", file=sys.stderr)
    sys.exit(1)
board = json.load(open(os.environ["BOARD"]))
if args[:2] == ["api", "graphql"]:
    # One page per document, as --paginate --slurp prints them.
    half = len(board["issues"]) // 2
    pages = [board["issues"][:half], board["issues"][half:]]
    print(json.dumps([{"data": {"repository": {"issues": {"nodes": page}}}} for page in pages]))
elif args[:2] == ["project", "item-list"]:
    print(json.dumps({"items": board["items"]}))
"""


class CheckBoard(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.dir = Path(tmp.name)
        write(self.dir / "bin" / "gh", STUB, stat.S_IRWXU)

    def check(self, board, fail=""):
        (self.dir / "board.json").write_text(json.dumps(board))
        return run(
            ["python3", SCRIPT],
            self.dir,
            PATH=f"{self.dir / 'bin'}:/usr/bin:/bin",
            BOARD=str(self.dir / "board.json"),
            GH_FAIL=fail,
        )

    def broken(self, issues, items):
        result = self.check({"issues": [FEATURE, *issues], "items": [item(1), *items]})
        self.assertEqual(result.returncode, 1, result.stdout)
        return result.stdout

    def test_a_tidy_board_passes_quietly(self):
        result = self.check(TIDY)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(result.stdout.strip(), "the board is tidy: 2 open issues, 1 closed")

    def test_an_open_issue_off_the_board_is_named(self):
        self.assertIn("#2 Issue 2: not on the board", self.broken([issue(2)], []))

    def test_each_missing_field_is_named(self):
        out = self.broken([issue(2, kind=None, milestone=None)], [item(2, tier=None, priority=None)])
        self.assertIn("#2 Issue 2: no type, no milestone, no tier, no priority", out)

    def test_work_needs_a_feature_and_an_area_but_a_feature_needs_neither(self):
        out = self.broken([issue(2, labels=("task",), parent=None)], [item(2)])
        self.assertIn("#2 Issue 2: no parent feature, no area: label", out)
        self.assertNotIn("#1 ", out)

    def test_area_none_counts_as_an_area(self):
        result = self.check(
            {"issues": [FEATURE, issue(2, labels=("task", "area: none"))], "items": [item(1), item(2)]}
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_a_closed_issue_not_done_is_named(self):
        out = self.broken([issue(2, state="CLOSED")], [item(2, status="Todo")])
        self.assertIn("#2 Issue 2: closed, but the board says Todo", out)

    def test_a_closed_issue_off_the_board_is_not_a_problem(self):
        result = self.check({"issues": [FEATURE, issue(2, state="CLOSED")], "items": [item(1)]})
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_every_problem_is_listed_and_counted(self):
        out = self.broken([issue(2, kind=None), issue(3)], [item(2)])
        self.assertIn("#2 Issue 2: no type", out)
        self.assertIn("#3 Issue 3: not on the board", out)
        self.assertIn("2 issues need fixing; CONTRIBUTING.md#project-board says how", out)

    def test_one_problem_is_counted_in_the_singular(self):
        self.assertIn("1 issue needs fixing;", self.broken([issue(2)], []))

    def test_issues_across_pages_are_all_read(self):
        # The stub splits the issues over two pages, so a check that read one page would miss #3.
        out = self.broken([issue(2), issue(3)], [item(2)])
        self.assertIn("#3 Issue 3: not on the board", out)

    def test_a_failing_read_says_which(self):
        for which in ("graphql", "item-list"):
            with self.subTest(which=which):
                result = self.check(TIDY, fail=which)
                self.assertEqual(result.returncode, 2)
                self.assertIn("could not read", result.stderr)
                self.assertIn("refused", result.stderr)


if __name__ == "__main__":
    unittest.main()
