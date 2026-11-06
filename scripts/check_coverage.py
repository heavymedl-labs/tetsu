#!/usr/bin/env python3
"""Fails if any crate is below the coverage limit for its tier.

usage: scripts/check_coverage.py <cargo metadata json> <cargo llvm-cov summary json>

Each crate under crates/ declares its tier and kind in [package.metadata.tetsu].
Limits are in ADR-0016. Stable Rust measures lines and regions, not branches (E3).
"""

import json
import sys
from pathlib import Path

LIMITS = {1: 100, 2: 90, 3: 90}
KINDS = {"platform", "vertical", "root"}
MEASURES = ("lines", "regions")


def crates(metadata):
    for package in metadata["packages"]:
        directory = Path(package["manifest_path"]).parent
        if directory.parent.name == "crates":
            yield package["name"], directory, (package.get("metadata") or {}).get("tetsu") or {}


def declaration_errors(name, declared):
    if declared.get("tier") not in LIMITS:
        yield f"{name}: set tier to 1, 2, or 3 in [package.metadata.tetsu]"
    if declared.get("kind") not in KINDS:
        yield f"{name}: set kind to platform, vertical, or root in [package.metadata.tetsu]"


def totals(directory, files):
    found = {measure: [0, 0] for measure in MEASURES}
    for file in files:
        if Path(file["filename"]).is_relative_to(directory):
            for measure in MEASURES:
                found[measure][0] += file["summary"][measure]["covered"]
                found[measure][1] += file["summary"][measure]["count"]
    return found


def coverage_errors(name, tier, found):
    limit = LIMITS[tier]
    for measure, (covered, count) in found.items():
        percent = 100 * covered / count if count else 100
        if percent < limit:
            yield f"{name}: {percent:.1f}% of {measure}, under the tier {tier} limit of {limit}%"


def path_errors(files, directories):
    """A report that matches no crate would otherwise read as nothing to cover."""
    if not files:
        yield "the coverage report lists no files"
    for file in files:
        path = Path(file["filename"])
        if not any(path.is_relative_to(directory) for directory in directories):
            yield f"{path} is in no crate, so its coverage cannot be checked"


def main():
    metadata = json.loads(Path(sys.argv[1]).read_text())
    files = json.loads(Path(sys.argv[2]).read_text())["data"][0]["files"]
    errors = list(path_errors(files, [directory for _, directory, _ in crates(metadata)]))
    for name, directory, declared in crates(metadata):
        problems = list(declaration_errors(name, declared))
        if not problems:
            problems = list(coverage_errors(name, declared["tier"], totals(directory, files)))
        errors.extend(problems)
    for error in errors:
        print(f"error: {error}", file=sys.stderr)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
