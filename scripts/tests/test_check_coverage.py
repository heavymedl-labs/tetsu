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


def covered_file(name, percent, branches=None):
    summary = {
        "lines": {"count": 100, "covered": percent},
        "regions": {"count": 100, "covered": percent},
        "branches": {"count": 100, "covered": percent if branches is None else branches},
    }
    return {"filename": f"/repo/crates/{name}/src/lib.rs", "summary": summary}


class CheckCoverage(unittest.TestCase):
    def check(self, packages, files, *flags, exceptions=()):
        with tempfile.TemporaryDirectory() as tmp:
            rows = "".join(f"{row}\n" for row in exceptions)
            (Path(tmp) / "CONSTRAINTS.md").write_text(
                "# Constraints\n\n## Exceptions\n\n| ID | Rule | Path | Reason | Owner | Expires |\n"
                "| --- | --- | --- | --- | --- | --- |\n" + rows
            )
            metadata = Path(tmp) / "metadata.json"
            report = Path(tmp) / "report.json"
            metadata.write_text(json.dumps({"packages": packages}))
            report.write_text(json.dumps({"data": [{"files": files}]}))
            return run(["python3", SCRIPT, *flags, metadata, report], tmp)

    def test_branches_are_checked_only_when_asked(self):
        files = [covered_file("a", 95, branches=50)]
        self.assertEqual(self.check([package("a", tier=2)], files).returncode, 0)
        result = self.check([package("a", tier=2)], files, "--branches")
        self.assertEqual(result.returncode, 1)
        self.assertIn("a: 50.0% of branches, under the tier 2 limit of 90%", result.stderr)

    def test_branch_limits_follow_the_tier(self):
        files = [covered_file("a", 100, branches=99)]
        self.assertEqual(self.check([package("a", tier=1)], files, "--branches").returncode, 1)
        self.assertEqual(self.check([package("a", tier=2)], files, "--branches").returncode, 0)

    def test_an_exception_lowers_one_crate_branch_limit(self):
        row = "| E9 | Branch limit 80% | `crates/a` | Unreachable | Maintainer | 2099-01-01 |"
        files = [covered_file("a", 100, branches=85), covered_file("b", 100, branches=85)]
        packages = [package("a"), package("b")]
        result = self.check(packages, files, "--branches", exceptions=[row])
        self.assertEqual(result.returncode, 1)
        self.assertNotIn("a: ", result.stderr)
        self.assertIn("b: 85.0% of branches", result.stderr)

    def test_an_exception_never_raises_a_limit_or_touches_lines(self):
        row = "| E9 | Branch limit 95% | `crates/a` | Odd | Maintainer | 2099-01-01 |"
        files = [covered_file("a", 92, branches=92)]
        self.assertEqual(self.check([package("a")], files, "--branches", exceptions=[row]).returncode, 0)
        low = "| E9 | Branch limit 80% | `crates/a` | Unreachable | Maintainer | 2099-01-01 |"
        lines = self.check([package("a")], [covered_file("a", 85, branches=100)], exceptions=[low])
        self.assertEqual(lines.returncode, 1)

    def test_an_expired_exception_no_longer_applies(self):
        row = "| E9 | Branch limit 80% | `crates/a` | Unreachable | Maintainer | 2000-01-01 |"
        result = self.check(
            [package("a")], [covered_file("a", 100, branches=85)], "--branches", exceptions=[row]
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("E9 expired on 2000-01-01", result.stderr)
        self.assertIn("a: 85.0% of branches, under the tier 2 limit of 90%", result.stderr)

    def test_a_branch_report_with_no_branches_at_all_fails(self):
        files = [covered_file("a", 100, branches=0)]
        files[0]["summary"]["branches"]["count"] = 0
        result = self.check([package("a", tier=2)], files, "--branches")
        self.assertEqual(result.returncode, 1)
        self.assertIn("the report counts no branches", result.stderr)

    def test_a_crate_with_no_branches_passes_when_others_have_them(self):
        files = [covered_file("a", 100), covered_file("b", 100, branches=0)]
        files[1]["summary"]["branches"]["count"] = 0
        result = self.check([package("a"), package("b")], files, "--branches")
        self.assertEqual(result.returncode, 0, result.stderr)

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
