#!/usr/bin/env python3
"""Removes what local work leaves behind (AGENTS.md, Clean up).

With --test-servers, stops only the `tetsu` servers tests started and left running, such as when cargo-mutants
kills a test that timed out: those whose DATABASE_URL names a test database, t_<seconds>_<process>_<count>.
scripts/check.sh runs it on the way out, so a server someone started by hand keeps running. Either way, it
also removes temporary files a test process left once that process is gone: `tetsu-<process>-*` and
`t_<seconds>_<process>_<count>.pem`.

With no option, it stops every `tetsu` built in this repository, drops leftover test databases, deletes
cargo-mutants' copies and output, and removes dangling container images. The test Postgres and its data stay.

usage: scripts/clean.py [--test-servers]
"""

import contextlib
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TEST_DATABASE = re.compile(rb"DATABASE_URL=.*/t_\d+_\d+_\d+(\?.*)?")
TEST_TEMPORARY = re.compile(r"tetsu-(\d+)-.+|t_\d+_(\d+)_\d+\.pem")
LEFTOVERS = [
    "target/mutants-tmp",
    "target/mutants.out",
    "target/mutants.out.old",
    "mutants.out",
    "mutants.out.old",
]


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


def servers(processes, test_only, root):
    def stoppable(exe, environ):
        return is_test_server(environ) if test_only else built_here(exe, root)

    return [pid for pid, exe, environ in processes if stoppable(exe, environ)]


def left_by_gone_tests(temporary, running):
    """Temporary files and folders a test process named for itself, when that process is no longer running."""
    for entry in sorted(temporary.iterdir()):
        named = TEST_TEMPORARY.fullmatch(entry.name)
        if named and not running(int(named.group(1) or named.group(2))):
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


def main(args, proc=Path("/proc"), root=ROOT, temporary=None, alive=running):
    if args not in ([], ["--test-servers"]):
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    test_only = bool(args)
    found = servers(tetsu_processes(proc), test_only, root)
    stop(found)
    if found:
        print(f"stopped {len(found)} tetsu {'test servers' if test_only else 'processes'}")
    for leftover in left_by_gone_tests(temporary or Path(tempfile.gettempdir()), alive):
        remove(leftover)
    if test_only:
        return 0
    drop = ["cargo", "run", "-q", "-p", "tetsu-postgres", "--example", "drop_test_databases"]
    dropped = subprocess.run(drop, cwd=root, check=False)
    for leftover in LEFTOVERS:
        shutil.rmtree(root / leftover, ignore_errors=True)
    if shutil.which("docker"):
        subprocess.run(["docker", "image", "prune", "-f"], check=False, capture_output=True)
    return dropped.returncode


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
