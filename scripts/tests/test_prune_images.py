"""Pruning keeps the ten newest images and the one deployed, and deletes nothing else it is not told about."""

import json
import unittest

from scripts.tests.support import ROOT, run

SCRIPT = ROOT / "scripts" / "prune_images.py"


def version(number):
    return {"id": number, "name": f"sha256:{number:064x}", "created_at": f"2026-11-{number:02d}T00:00:00Z"}


def prune(versions, deployed, *extra):
    lines = "".join(json.dumps(v) + "\n" for v in versions)
    return run(["python3", SCRIPT, "--deployed", deployed, *extra], ROOT, input=lines)


class PruneImagesTest(unittest.TestCase):
    def test_it_keeps_the_ten_newest_and_deletes_the_rest(self):
        versions = [version(n) for n in range(1, 16)]
        result = prune(versions, version(15)["name"])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.split(), ["5", "4", "3", "2", "1"])

    def test_order_comes_from_the_dates_not_the_input(self):
        versions = [version(n) for n in [3, 14, 1, 12, 15, 2, 13, 4, 11, 5, 10, 6, 9, 7, 8]]
        self.assertEqual(prune(versions, version(15)["name"]).stdout.split(), ["5", "4", "3", "2", "1"])

    def test_the_deployed_image_is_kept_however_old(self):
        versions = [version(n) for n in range(1, 16)]
        result = prune(versions, version(2)["name"])
        self.assertEqual(result.stdout.split(), ["5", "4", "3", "1"])

    def test_nothing_is_deleted_when_ten_or_fewer_exist(self):
        result = prune([version(n) for n in range(1, 11)], version(10)["name"])
        self.assertEqual((result.returncode, result.stdout), (0, ""))

    def test_a_deployed_digest_that_is_not_listed_deletes_nothing(self):
        # The list may be stale or the wrong package; deleting without the deployed image in it is unsafe.
        result = prune([version(n) for n in range(1, 16)], "sha256:" + "f" * 64)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")
        self.assertIn("is not in the list", result.stderr)

    def test_a_missing_or_malformed_digest_is_a_usage_error(self):
        for args in [[], ["--deployed"], ["--deployed", "latest"], ["--deployed", "sha256:abc"]]:
            result = run(["python3", SCRIPT, *args], ROOT, input="")
            self.assertEqual(result.returncode, 2, args)

    def test_input_that_is_not_a_version_list_fails(self):
        result = run(["python3", SCRIPT, "--deployed", version(1)["name"]], ROOT, input="not json\n")
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")


if __name__ == "__main__":
    unittest.main()
