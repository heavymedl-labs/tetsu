#!/usr/bin/env python3
"""Fails if AGENTS.md is too long, and warns about other long docs.

Agents load AGENTS.md every session. Tokens are estimated at 4 characters each.
"""

import subprocess
import sys
from pathlib import Path

AGENTS_MAX_LINES = 100
DOC_WARN_TOKENS = 3000


def main():
    agents = Path("AGENTS.md")
    if not agents.exists():
        print("error: AGENTS.md is missing", file=sys.stderr)
        return 1
    tracked = subprocess.run(["git", "ls-files", "*.md"], capture_output=True, text=True, check=True).stdout
    for name in tracked.splitlines():
        path = Path(name)
        tokens = len(path.read_text()) // 4 if path.exists() else 0
        if tokens > DOC_WARN_TOKENS:
            print(f"warning: {name} is about {tokens} tokens. Merge or remove what you can.", file=sys.stderr)
    lines = len(agents.read_text().splitlines())
    if lines > AGENTS_MAX_LINES:
        print(f"error: AGENTS.md has {lines} lines, over the limit of {AGENTS_MAX_LINES}.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
