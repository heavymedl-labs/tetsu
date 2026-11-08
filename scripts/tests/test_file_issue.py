"""Filing an issue puts it on the board with its milestone, tier, and priority in one step."""

import json
import stat
import tempfile
import unittest
from pathlib import Path

from scripts.tests.support import ROOT, run, write

SCRIPT = ROOT / "scripts" / "file_issue.py"
FIELDS = {
    "fields": [
        {"id": "F-status", "name": "Status", "options": [{"id": "O-todo", "name": "Todo"}]},
        {
            "id": "F-tier",
            "name": "Tier",
            "options": [{"id": "O-t2", "name": "2"}, {"id": "O-tnone", "name": "None"}],
        },
        {
            "id": "F-priority",
            "name": "Priority",
            "options": [{"id": "O-now", "name": "Now"}, {"id": "O-later", "name": "Later"}],
        },
    ]
}
MILESTONES = [{"number": 2, "title": "2. Second"}, {"number": 1, "title": "1. First"}]
# Answers each gh call the script makes, and logs every call so a test can see what was asked.
STUB = f"""#!/usr/bin/env python3
import json, os, sys
args = sys.argv[1:]
with open(os.environ["GH_LOG"], "a") as log:
    log.write(json.dumps(args) + "\\n")
if os.environ.get("GH_ALREADY") and args[:2] == ["project", "item-add"]:
    print("GraphQL: Content already exists in this project (addProjectV2ItemById)", file=sys.stderr)
    sys.exit(1)
if os.environ.get("GH_FAIL") == " ".join(args[:2]):
    print("refused", file=sys.stderr)
    sys.exit(1)
if args[:2] == ["issue", "create"]:
    print("https://github.com/o/r/issues/7")
elif args[:2] == ["project", "item-add"]:
    print(json.dumps({{"id": "ITEM"}}))
elif args[:2] == ["project", "item-list"]:
    items = [
        {{"id": "OTHER", "content": {{"url": "https://github.com/o/r/issues/6"}}}},
        {{"id": "REDACTED", "content": None}},
        {{"id": "AUTO", "content": {{"url": "https://github.com/o/r/issues/7"}}}},
    ]
    if os.environ.get("GH_ALREADY") == "missing":
        items = items[:1]
    print(json.dumps({{"items": items}}))
elif args[:2] == ["project", "view"]:
    print(json.dumps({{"id": "PROJECT"}}))
elif args[:2] == ["project", "field-list"]:
    print(json.dumps({json.dumps(FIELDS)}))
elif args[0] == "api":
    print(json.dumps({json.dumps(MILESTONES)}))
"""


class FileIssue(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.dir = Path(tmp.name)
        write(self.dir / "bin" / "gh", STUB, stat.S_IRWXU)
        self.log = self.dir / "gh.log"
        self.body = self.dir / "body.md"
        self.body.write_text("### Goal\n\nSomething.\n")

    def file(self, *args, fail="", already=""):
        return run(
            ["python3", SCRIPT, *args],
            self.dir,
            PATH=f"{self.dir / 'bin'}:/usr/bin:/bin",
            GH_LOG=str(self.log),
            GH_FAIL=fail,
            GH_ALREADY=already,
        )

    def calls(self):
        if not self.log.exists():
            return []
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_files_the_issue_and_sets_its_fields_on_the_board(self):
        result = self.file("task", "Do a thing", self.body, "--tier", "2", "--priority", "now")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "https://github.com/o/r/issues/7")
        calls = self.calls()
        create = next(c for c in calls if c[:2] == ["issue", "create"])
        self.assertIn("1. First", create, "the earliest open milestone by number")
        self.assertEqual(create[create.index("--label") + 1], "task")
        self.assertEqual(create[create.index("--body-file") + 1], str(self.body))
        edits = [c for c in calls if c[:2] == ["project", "item-edit"]]
        options = {e[e.index("--field-id") + 1]: e[e.index("--single-select-option-id") + 1] for e in edits}
        self.assertEqual(options, {"F-tier": "O-t2", "F-priority": "O-now"})
        for edit in edits:
            self.assertEqual(edit[edit.index("--id") + 1], "ITEM")
            self.assertEqual(edit[edit.index("--project-id") + 1], "PROJECT")

    def test_a_named_milestone_and_tier_none_are_used_as_given(self):
        result = self.file(
            "adr",
            "Decide a thing",
            self.body,
            "--tier",
            "none",
            "--priority",
            "later",
            "--milestone",
            "2. Second",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        create = next(c for c in self.calls() if c[:2] == ["issue", "create"])
        self.assertIn("2. Second", create)
        self.assertFalse(any(c[0] == "api" for c in self.calls()), "no milestone lookup when one is named")
        edits = [c for c in self.calls() if c[:2] == ["project", "item-edit"]]
        self.assertIn("O-tnone", [e[e.index("--single-select-option-id") + 1] for e in edits])

    def test_refuses_an_unknown_label_tier_or_priority_before_creating_anything(self):
        for args in (
            ("bug", "T", self.body, "--tier", "2", "--priority", "now"),
            ("task", "T", self.body, "--tier", "4", "--priority", "now"),
            ("task", "T", self.body, "--tier", "2", "--priority", "soon"),
        ):
            with self.subTest(args=args):
                result = self.file(*args)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(any(c[:2] == ["issue", "create"] for c in self.calls()))

    def test_a_board_missing_a_field_option_fails_before_creating_anything(self):
        result = self.file("task", "T", self.body, "--tier", "3", "--priority", "now")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Tier has no option 3", result.stderr)
        self.assertFalse(any(c[:2] == ["issue", "create"] for c in self.calls()))

    def test_an_issue_auto_add_placed_first_still_gets_its_fields(self):
        result = self.file("task", "T", self.body, "--tier", "2", "--priority", "now", already="1")
        self.assertEqual(result.returncode, 0, result.stderr)
        edits = [c for c in self.calls() if c[:2] == ["project", "item-edit"]]
        self.assertEqual(len(edits), 2)
        for edit in edits:
            self.assertEqual(edit[edit.index("--id") + 1], "AUTO")

    def test_an_auto_added_item_that_cannot_be_found_names_the_filed_issue(self):
        result = self.file("task", "T", self.body, "--tier", "2", "--priority", "now", already="missing")
        self.assertEqual(result.returncode, 1)
        self.assertIn("filed https://github.com/o/r/issues/7", result.stderr)
        self.assertIn("its item was not found", result.stderr)
        self.assertFalse(any(c[:2] == ["project", "item-edit"] for c in self.calls()))

    def test_a_board_that_refuses_an_edit_names_the_filed_issue(self):
        for step in ("project item-add", "project item-edit"):
            with self.subTest(step=step):
                result = self.file("task", "T", self.body, "--tier", "2", "--priority", "now", fail=step)
                self.assertEqual(result.returncode, 1)
                self.assertIn("filed https://github.com/o/r/issues/7", result.stderr)
                self.assertIn("tier (2) and priority (Now)", result.stderr)
                self.assertIn("rather than filing it again", result.stderr)

    def test_a_missing_body_file_fails_before_creating_anything(self):
        result = self.file("task", "T", self.dir / "none.md", "--tier", "2", "--priority", "now")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("none.md", result.stderr)
        self.assertEqual(self.calls(), [])


if __name__ == "__main__":
    unittest.main()
