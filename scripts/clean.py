#!/usr/bin/env python3
"""Removes what local work leaves behind (AGENTS.md, Clean up).

With --routine, which scripts/check.sh runs on the way out, it removes only what nothing still uses:
- `tetsu` servers tests left running, such as when cargo-mutants kills a test that timed out: those whose
  DATABASE_URL names a test database, t_<seconds>_<process>_<count>. A server started by hand keeps running.
- temporary files a test process left once that process is gone: `tetsu-<process>-*` and
  `t_<seconds>_<process>_<count>.pem`.
- the copies and output mutation and coverage runs keep while they run (cargo-mutants', StrykerJS's, and
  Vitest's), dangling container images, and build cache unused for 7 days.
- superseded build artifacts under target/. Cargo names each `<unit>-<hash>` and never removes the copies a
  change replaces, so target/ only grows. Of each unit's copies it keeps the newest three, and removes older
  ones a day after they were written; one still needed is rebuilt.

With no option, it also stops every `tetsu` built in this repository, drops leftover test databases, deletes
cargo-mutants' reports, and empties the build cache. The test Postgres and its data stay.

usage: scripts/clean.py [--routine]
"""

import contextlib
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TEST_DATABASE = re.compile(rb"DATABASE_URL=.*/t_\d+_\d+_\d+(\?.*)?")
TEST_TEMPORARY = re.compile(r"tetsu-(\d+)-.+|t_\d+_(\d+)_\d+\.pem")
BUILD_OUTPUTS = ("deps", "incremental", "build", ".fingerprint", "examples")
# Cargo hashes outputs in 16 hex digits, and incremental folders in 13 base-36 ones.
BUILD_UNIT = re.compile(r"(.*)-(?:[0-9a-f]{16}|[0-9a-z]{13})(\..*)?")
KEPT_COPIES = 3
# Build output folders sit in a profile folder, such as target/debug or target/nightly/<triple>/debug.
PROFILE_DEPTH = 3
SETTLE = 24 * 3600
# Each is removed when its run ends, unless the run was killed.
RUN_LEFTOVERS = ["target/mutants-tmp", ".tools/stryker-tmp", ".tools/coverage-web"]
REPORTS = ["target/mutants.out", "target/mutants.out.old", "mutants.out", "mutants.out.old"]


def is_test_server(environ):
    """Whether a process's environment, as /proc holds it, points it at a test database."""
    return any(TEST_DATABASE.fullmatch(variable) for variable in environ.split(b"\0"))


def built_here(exe, root):
    """Whether an executable is one this repository's builds made, even one whose file is gone."""
    return exe.removesuffix(" (deleted)").startswith(f"{root / 'target'}{os.sep}")


def tetsu_processes(proc):
    """Each readable process named tetsu, as (pid, executable, environment). Without /proc there are none."""
    if not proc.is_dir():
        return
    for entry in proc.iterdir():
        if not entry.name.isdigit():
            continue
        try:
            if (entry / "comm").read_text().strip() != "tetsu":
                continue
            yield int(entry.name), str((entry / "exe").readlink()), (entry / "environ").read_bytes()
        except OSError:
            continue


def servers(processes, routine, root):
    def stoppable(exe, environ):
        return is_test_server(environ) if routine else built_here(exe, root)

    return [pid for pid, exe, environ in processes if stoppable(exe, environ)]


def left_by_gone_tests(temporary, running):
    """Temporary files and folders a test process named for itself, when that process is no longer running."""
    for entry in sorted(temporary.iterdir()):
        named = TEST_TEMPORARY.fullmatch(entry.name)
        if named and not running(int(named.group(1) or named.group(2))):
            yield entry


def superseded_builds(target, now, kept=KEPT_COPIES, settle=SETTLE):
    """Copies of a build unit in target/ beyond its newest `kept`, once `settle` seconds old."""
    for folder, children, _ in os.walk(target):
        depth = len(Path(folder).relative_to(target).parts)
        outputs = [child for child in children if child in BUILD_OUTPUTS]
        skipped = (*BUILD_OUTPUTS, "mutants.out", "mutants-tmp")
        children[:] = [child for child in children if depth < PROFILE_DEPTH and child not in skipped]
        for name in sorted(outputs):
            copies = {}
            for entry in (Path(folder) / name).iterdir():
                unit = BUILD_UNIT.fullmatch(entry.name)
                if unit:
                    copies.setdefault(unit.groups(), []).append((entry.lstat().st_mtime, entry))
            for unit in copies.values():
                for written, entry in sorted(unit, reverse=True)[kept:]:
                    if now - written > settle:
                        yield entry


def running(pid):
    return Path(f"/proc/{pid}").exists()


def remove(path):
    if path.is_dir() and not path.is_symlink():
        shutil.rmtree(path, ignore_errors=True)
    else:
        path.unlink(missing_ok=True)


def stop(pids):
    for pid in pids:
        with contextlib.suppress(ProcessLookupError):
            os.kill(pid, signal.SIGTERM)


def docker(*args):
    if shutil.which("docker"):
        subprocess.run(["docker", *args], check=False, capture_output=True)


@dataclass(frozen=True)
class Host:
    """Where to look, so tests can stand in a folder for each."""

    proc: Path = Path("/proc")
    root: Path = ROOT
    temporary: Path = field(default_factory=lambda: Path(tempfile.gettempdir()))
    alive: object = running
    now: float = field(default_factory=time.time)


def main(args, host=None):
    host = host or Host()
    if args not in ([], ["--routine"]):
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    routine = bool(args)
    found = servers(tetsu_processes(host.proc), routine, host.root)
    stop(found)
    if found:
        print(f"stopped {len(found)} tetsu {'test servers' if routine else 'processes'}")
    for leftover in left_by_gone_tests(host.temporary, host.alive):
        remove(leftover)
    superseded = list(superseded_builds(host.root / "target", host.now))
    for leftover in [*superseded, *(host.root / path for path in RUN_LEFTOVERS)]:
        remove(leftover)
    if superseded:
        print(f"removed {len(superseded)} superseded build outputs")
    docker("image", "prune", "-f")
    if routine:
        docker("builder", "prune", "-f", "--filter", "until=168h")
        return 0
    docker("builder", "prune", "-f")
    for report in REPORTS:
        remove(host.root / report)
    drop = ["cargo", "run", "-q", "-p", "tetsu-postgres", "--example", "drop_test_databases"]
    return subprocess.run(drop, cwd=host.root, check=False).returncode


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
