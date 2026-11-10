"""Dated ignore entries: each names its issue and a review date within 30 days of its change (ADR-0028)."""

import tempfile
import unittest
from pathlib import Path

from scripts.tests.support import ROOT, run, write

SCRIPT = ROOT / "scripts" / "check_dated_entries.py"
REASON = "fxhash comes through monoio. Tracked in #94. Review by {date}."


def config(*entries):
    lines = "\n".join(f"    {entry}" for entry in entries)
    return f'[advisories]\nunmaintained = "all"\nignore = [\n{lines}\n]\n\n[bans]\nwildcards = "deny"\n'


def entry(date, advisory="RUSTSEC-2025-0057"):
    return f'{{ id = "{advisory}", reason = "{REASON.format(date=date)}" }},'


class DatedEntriesTest(unittest.TestCase):
    def setUp(self):
        self.repo = Path(tempfile.mkdtemp())
        self.addCleanup(lambda: run(["rm", "-rf", self.repo], ROOT))
        run(["git", "init", "-q"], self.repo, check=True)
        self.file = self.repo / "deny.toml"

    def commit(self, text, date):
        write(self.file, text)
        run(["git", "add", "deny.toml"], self.repo, check=True)
        stamp = f"{date}T12:00:00Z"
        run(
            ["git", "-c", "user.name=t", "-c", "user.email=t@example.com", "commit", "-qm", "c"],
            self.repo,
            check=True,
            GIT_AUTHOR_DATE=stamp,
            GIT_COMMITTER_DATE=stamp,
        )

    def check(self, today):
        return run(["python3", SCRIPT, self.file, "advisories", "ignore", "--today", today], self.repo)

    def assert_fails(self, result, *words):
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        for word in words:
            self.assertIn(word, result.stderr)

    def test_an_entry_within_30_days_of_its_commit_passes(self):
        self.commit(config(entry("2026-12-10")), "2026-11-10")
        result = self.check("2026-11-20")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_no_entries_passes(self):
        self.commit(config(), "2026-11-10")
        self.assertEqual(self.check("2026-11-20").returncode, 0)

    def test_a_bare_id_fails(self):
        self.commit(config('"RUSTSEC-2025-0057",'), "2026-11-10")
        self.assert_fails(self.check("2026-11-20"), "deny.toml:4", "one-line table")

    def test_a_reason_without_its_issue_and_date_fails(self):
        self.commit(config('{ id = "RUSTSEC-2025-0057", reason = "harness only" },'), "2026-11-10")
        self.assert_fails(self.check("2026-11-20"), "RUSTSEC-2025-0057", "Tracked in #")

    def test_a_date_more_than_30_days_after_the_commit_fails(self):
        self.commit(config(entry("2026-12-11")), "2026-11-10")
        self.assert_fails(self.check("2026-11-20"), "RUSTSEC-2025-0057", "30 days")

    def test_a_date_that_has_passed_fails(self):
        self.commit(config(entry("2026-12-10")), "2026-11-10")
        self.assertEqual(self.check("2026-12-10").returncode, 0)
        self.assert_fails(self.check("2026-12-11"), "RUSTSEC-2025-0057", "passed")

    def test_renewing_an_entry_restarts_its_30_days(self):
        self.commit(config(entry("2026-12-10")), "2026-11-10")
        self.commit(config(entry("2027-01-08")), "2026-12-09")
        self.assertEqual(self.check("2027-01-01").returncode, 0)

    def test_an_uncommitted_entry_counts_from_today(self):
        self.commit(config(), "2026-11-10")
        write(self.file, config(entry("2026-12-20")))
        self.assertEqual(self.check("2026-11-20").returncode, 0)
        self.assert_fails(self.check("2026-11-19"), "30 days")

    def test_a_file_git_does_not_track_yet_counts_from_today(self):
        write(self.file, config(entry("2026-12-20")))
        self.assertEqual(self.check("2026-11-20").returncode, 0)
        self.assert_fails(self.check("2026-11-19"), "30 days")

    def test_an_entry_split_over_lines_fails(self):
        self.commit(config('{ id = "RUSTSEC-2025-0057",', '  reason = "x" },'), "2026-11-10")
        self.assert_fails(self.check("2026-11-20"), "one-line table")

    def test_every_bad_entry_is_reported(self):
        self.commit(config(entry("2026-12-11"), entry("2027-02-01", "RUSTSEC-2026-0441")), "2026-11-10")
        result = self.check("2026-11-20")
        self.assert_fails(result, "RUSTSEC-2025-0057", "RUSTSEC-2026-0441")

    def test_other_ways_of_writing_the_list_fail_rather_than_pass_unread(self):
        """cargo-deny reads all of these, so each must fail rather than let a bare id through unchecked."""
        bare = '"RUSTSEC-2025-0057"'
        for text in (
            f"[advisories]\nignore = [{bare}]\n",
            f"[advisories]\nignore = [ # held\n    {bare},\n]\n",
            f"[advisories] # rules\nignore = [\n    {bare},\n]\n",
            f"[[advisories.ignore]]\nid = {bare}\n",
            f"[ advisories ]\nignore = [\n    {bare},\n]\n",
            f'[advisories]\n"ignore" = [\n    {bare},\n]\n',
            f"advisories.ignore = [\n    {bare},\n]\n",
        ):
            with self.subTest(text=text):
                self.commit(text, "2026-11-10")
                self.assert_fails(self.check("2026-11-20"), "ignore")

    def test_an_impossible_date_fails_with_its_line(self):
        self.commit(config(entry("2026-02-31")), "2026-11-10")
        self.assert_fails(self.check("2026-11-20"), "deny.toml:4", "2026-02-31")

    def test_a_file_outside_a_git_repository_fails_rather_than_counting_from_today(self):
        outside = Path(tempfile.mkdtemp())
        self.addCleanup(lambda: run(["rm", "-rf", outside], ROOT))
        write(outside / "deny.toml", config(entry("2026-12-20")))
        result = run(
            ["python3", SCRIPT, outside / "deny.toml", "advisories", "ignore", "--today", "2026-11-20"],
            outside,
            GIT_CEILING_DIRECTORIES=str(outside.parent),
        )
        self.assert_fails(result, "git")

    def test_a_list_that_is_not_closed_fails(self):
        self.commit('[advisories]\nignore = [\n    "RUSTSEC-2025-0057",\n', "2026-11-10")
        self.assert_fails(self.check("2026-11-20"), "not closed")


if __name__ == "__main__":
    unittest.main()
