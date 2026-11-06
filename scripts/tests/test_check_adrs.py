"""check-adrs.sh must accept a valid ADR and fail each kind of broken one."""

import shutil
import tempfile
import unittest
from pathlib import Path

from scripts.tests.support import ROOT, run, write

VALID = """# 0001. First

- Status: Accepted
- Date: 2026-01-01

## Context

## Decision

## Alternatives considered

## Consequences

## Sources
"""
INDEX = "| [0001](0001-first.md) | First | Accepted |\n"


class CheckAdrs(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        (self.root / "scripts").mkdir()
        shutil.copy(ROOT / "scripts" / "check-adrs.sh", self.root / "scripts")
        self.adr = self.root / "docs" / "adr" / "0001-first.md"
        self.index = self.root / "docs" / "adr" / "README.md"
        write(self.adr, VALID)
        write(self.index, INDEX)

    def check(self):
        return run([self.root / "scripts" / "check-adrs.sh"], self.root)

    def assertFails(self, message):
        result = self.check()
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn(message, result.stderr)

    def test_valid_adr_passes(self):
        self.assertEqual(self.check().returncode, 0)

    def test_missing_section_fails(self):
        self.adr.write_text(VALID.replace("## Sources\n", ""))
        self.assertFails("missing section '## Sources'")

    def test_bad_status_fails(self):
        self.adr.write_text(VALID.replace("Accepted", "Maybe"))
        self.assertFails("invalid '- Status:'")

    def test_bad_date_fails(self):
        self.adr.write_text(VALID.replace("2026-01-01", "soon"))
        self.assertFails("invalid '- Date:")

    def test_wrong_title_fails(self):
        self.adr.write_text(VALID.replace("# 0001. First", "# First"))
        self.assertFails("first line must be")

    def test_missing_index_row_fails(self):
        self.index.write_text("")
        self.assertFails("not listed in docs/adr/README.md")

    def test_index_status_mismatch_fails(self):
        self.index.write_text(INDEX.replace("Accepted", "Proposed"))
        self.assertFails("index status does not match")

    def test_number_gap_fails(self):
        self.adr.rename(self.adr.with_name("0002-first.md"))
        self.assertFails("expected ADR number 0001")


if __name__ == "__main__":
    unittest.main()
