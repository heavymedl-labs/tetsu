"""Helpers for tests that run the repository's scripts in throwaway directories."""

import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def clean_env(**extra):
    """Drops the GIT_* variables a git hook sets, so tests never touch the real repository."""
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    env.update(extra)
    return env


def run(args, cwd, check=False, input=None, **extra_env):
    return subprocess.run(
        [str(a) for a in args],
        cwd=cwd,
        env=clean_env(**extra_env),
        capture_output=True,
        text=True,
        check=check,
        # With no input, stdin is closed rather than inherited, so a script that reads it cannot hang.
        input=input,
        stdin=subprocess.DEVNULL if input is None else None,
    )


def write(path, text, mode=None):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
    if mode:
        path.chmod(mode)
