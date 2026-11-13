#!/usr/bin/env python3
"""Removes what local work leaves behind (AGENTS.md, Clean up).

With --test-servers, stops only the `tetsu` servers tests started and left running, such as when cargo-mutants
kills a test that timed out: those whose DATABASE_URL names a test database, t_<seconds>_<process>_<count>.
scripts/check.sh runs it on the way out, so a server someone started by hand keeps running.

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
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TEST_DATABASE = re.compile(rb"DATABASE_URL=.*/t_\d+_\d+_\d+(\?.*)?")
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


def stop(pids):
    for pid in pids:
        with contextlib.suppress(ProcessLookupError):
            os.kill(pid, signal.SIGTERM)


def main(args, proc=Path("/proc"), root=ROOT):
    if args not in ([], ["--test-servers"]):
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    test_only = bool(args)
    found = servers(tetsu_processes(proc), test_only, root)
    stop(found)
    if found:
        print(f"stopped {len(found)} tetsu {'test servers' if test_only else 'processes'}")
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
