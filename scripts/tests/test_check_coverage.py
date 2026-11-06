"""Each crate must meet the coverage limit for its own tier."""

import json
import tempfile
import unittest
from pathlib import Path

from scripts.tests.support import ROOT, run

SCRIPT = ROOT / "scripts" / "check_coverage.py"


def package(name, tier=2, kind="platform"):
    tetsu = {}
    if tier is not None:
        tetsu["tier"] = tier
    if kind is not None:
        tetsu["kind"] = kind
    return {
        "name": name,
        "manifest_path": f"/repo/crates/{name}/Cargo.toml",
        "metadata": {"tetsu": tetsu} if tetsu else None,
    }


def covered_file(name, percent):
    return {
        "filename": f"/repo/crates/{name}/src/lib.rs",
        "summary": {
            "lines": {"count": 100, "covered": percent},
            "regions": {"count": 100, "covered": percent},
        },
    }


class CheckCoverage(unittest.TestCase):
    def check(self, packages, files):
        with tempfile.TemporaryDirectory() as tmp:
            metadata = Path(tmp) / "metadata.json"
            report = Path(tmp) / "report.json"
            metadata.write_text(json.dumps({"packages": packages}))
            report.write_text(json.dumps({"data": [{"files": files}]}))
            return run(["python3", SCRIPT, metadata, report], tmp)

    def test_a_crate_at_its_limit_passes(self):
        result = self.check([package("a", tier=2)], [covered_file("a", 90)])
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_a_crate_below_its_limit_fails(self):
        result = self.check([package("a", tier=2)], [covered_file("a", 89)])
        self.assertEqual(result.returncode, 1)
        self.assertIn("a", result.stderr)
        self.assertIn("tier 2", result.stderr)

    def test_a_well_covered_crate_cannot_hide_a_weak_one(self):
        result = self.check(
            [package("good", tier=2), package("weak", tier=2)],
            [covered_file("good", 100), covered_file("weak", 80)],
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("weak", result.stderr)
        self.assertNotIn("good", result.stderr)

    def test_tier_1_needs_full_coverage(self):
        self.assertEqual(self.check([package("a", tier=1)], [covered_file("a", 99)]).returncode, 1)
        self.assertEqual(self.check([package("a", tier=1)], [covered_file("a", 100)]).returncode, 0)

    def test_tier_3_keeps_the_tier_2_limit(self):
        self.assertEqual(self.check([package("a", tier=3)], [covered_file("a", 90)]).returncode, 0)
        self.assertEqual(self.check([package("a", tier=3)], [covered_file("a", 89)]).returncode, 1)

    def test_lines_and_regions_each_count(self):
        file = covered_file("a", 100)
        file["summary"]["regions"]["covered"] = 50
        result = self.check([package("a", tier=2)], [file])
        self.assertEqual(result.returncode, 1)
        self.assertIn("regions", result.stderr)

    def test_files_in_one_crate_add_up(self):
        second = covered_file("a", 80)
        second["filename"] = "/repo/crates/a/src/other.rs"
        result = self.check([package("a", tier=2)], [covered_file("a", 100), second])
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_a_crate_with_nothing_to_cover_passes(self):
        result = self.check([package("contract", tier=2), package("a", tier=2)], [covered_file("a", 100)])
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_an_empty_report_fails(self):
        result = self.check([package("a", tier=2)], [])
        self.assertEqual(result.returncode, 1)
        self.assertIn("no files", result.stderr)

    def test_a_file_in_no_crate_fails(self):
        stray = covered_file("a", 0)
        stray["filename"] = "/elsewhere/crates/a/src/lib.rs"
        result = self.check([package("a", tier=2)], [stray])
        self.assertEqual(result.returncode, 1)
        self.assertIn("/elsewhere/crates/a/src/lib.rs", result.stderr)

    def test_a_crate_with_no_tier_fails(self):
        result = self.check([package("a", tier=None)], [])
        self.assertEqual(result.returncode, 1)
        self.assertIn("package.metadata.tetsu", result.stderr)

    def test_a_crate_with_a_bad_tier_fails(self):
        self.assertEqual(self.check([package("a", tier=4)], []).returncode, 1)

    def test_a_crate_with_no_kind_fails(self):
        self.assertEqual(self.check([package("a", kind=None)], []).returncode, 1)

    def test_a_crate_with_a_bad_kind_fails(self):
        self.assertEqual(self.check([package("a", kind="other")], []).returncode, 1)

    def test_packages_outside_crates_are_ignored(self):
        harness = {"name": "bench", "manifest_path": "/repo/benchmarks/Cargo.toml", "metadata": None}
        result = self.check([harness, package("a", tier=2)], [covered_file("a", 100)])
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
