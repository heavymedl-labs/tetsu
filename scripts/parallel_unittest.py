"""Runs every test under a folder, each in its own process, as many at once as TETSU_CHECK_CORES, or else the
machine's cores.

The slowest test then sets the time, not the sum of them. Each process buffers its output, and only a failing
test's output is printed, whole, so failures from tests running at once never interleave. Exits 1 if any
fails.

Usage: python3 scripts/parallel_unittest.py <start folder> <top-level folder>
"""

import os
import subprocess
import sys
import unittest
from concurrent.futures import ThreadPoolExecutor

# The script's name, the start folder, and the top-level folder.
ARGUMENTS = 3


def test_ids(start, top):
    """The id of every test unittest discovers, in discovery order."""
    found = []

    def walk(suite):
        for test in suite:
            if isinstance(test, unittest.TestSuite):
                walk(test)
            else:
                found.append(test.id())

    walk(unittest.TestLoader().discover(start, top_level_dir=top))
    return found


def run(test_id, top):
    """Runs one test in its own interpreter, returning whether it passed and what it printed."""
    result = subprocess.run(
        [sys.executable, "-m", "unittest", "-b", test_id],
        cwd=top,
        capture_output=True,
        text=True,
        check=False,
    )
    return result.returncode == 0, result.stdout + result.stderr


def main(argv):
    if len(argv) != ARGUMENTS:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    start, top = argv[1], argv[2]
    ids = test_ids(start, top)
    if not ids:
        print(f"error: no tests found under {start}", file=sys.stderr)
        return 1
    workers = int(os.environ.get("TETSU_CHECK_CORES") or os.cpu_count() or 1)
    with ThreadPoolExecutor(max_workers=workers) as pool:
        results = list(pool.map(lambda test_id: run(test_id, top), ids))
    failed = [(test_id, output) for test_id, (passed, output) in zip(ids, results) if not passed]
    for test_id, output in failed:
        print(f"FAILED {test_id}\n{output}", file=sys.stderr)
    print(f"ran {len(ids)} tests, {len(failed)} failed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
