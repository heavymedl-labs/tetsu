#!/usr/bin/env python3
"""Prints the ids of image versions to delete: all but the ten newest and the one deployed (ADR-0033).

Reads one JSON object per line on stdin, each with `id`, `name` (the digest), and `created_at`, as
`gh api --jq '.[] | {id, name, created_at}'` prints them. Prints nothing and fails if the deployed digest is
not in the list, since a stale or wrong list must not lead to deleting the live image.

usage: scripts/prune_images.py --deployed sha256:<64 hex>
"""

import json
import re
import sys

KEEP = 10
DIGEST = re.compile(r"sha256:[0-9a-f]{64}")


def to_delete(versions, deployed):
    newest_first = sorted(versions, key=lambda version: version["created_at"], reverse=True)
    return [version["id"] for version in newest_first[KEEP:] if version["name"] != deployed]


def main(args):
    if len(args) != len(["--deployed", "digest"]) or args[0] != "--deployed" or not DIGEST.fullmatch(args[1]):
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    deployed = args[1]
    try:
        versions = [json.loads(line) for line in sys.stdin if line.strip()]
        names = {version["name"] for version in versions}
    except (json.JSONDecodeError, KeyError, TypeError):
        print("error: the input is not a list of image versions", file=sys.stderr)
        return 1
    if deployed not in names:
        print(f"error: the deployed image {deployed} is not in the list; deleting nothing", file=sys.stderr)
        return 1
    for identifier in to_delete(versions, deployed):
        print(identifier)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
