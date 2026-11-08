#!/usr/bin/env python3
"""Fails a diff that lowers the bar set by the floor in CONSTRAINTS.md.

usage: scripts/floor_guard.py [--base <ref>]   (default: origin/main)

Exits 0 when clean, 1 on a violation, and 2 when it could not run. A
"Floor-reason: <why>" trailer on a commit that touches the file turns that
file's violations into warnings.

The patterns are shallow on purpose: they catch the cheap road to green, not a
determined person, so the fresh review still applies.

Adapted from the floor guard in addyosmani/agent-skills (MIT), with Rust and
Python patterns. It does not scan for secrets; see issue 31.
"""

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path

# This guard and its tests name the patterns they look for.
SCAN_EXEMPT = {"scripts/floor_guard.py", "scripts/tests/test_floor_guard.py"}
# Lint, coverage, and mutation config: removing a line can switch a rule off, and so can adding a loose one.
LINT_CONFIG = {
    "clippy.toml",
    "ruff.toml",
    ".squawk.toml",
    ".sqlfluff",
    ".editorconfig",
    ".markdownlint-cli2.yaml",
    ".cargo/mutants.toml",
    ".cargo/config.toml",
}
# Removing a line from these lowers the bar, so each removal needs a reason.
PROTECTED = LINT_CONFIG | {
    "scripts/check.sh",
    "scripts/check_coverage.py",
    "scripts/check_doc_sizes.py",
    "scripts/check-adrs.sh",
    "scripts/floor_guard.py",
    ".githooks/pre-push",
    ".githooks/pre-commit",
    ".github/workflows/ci.yml",
    "Cargo.toml",
}
LOOSENING = re.compile(
    r"\ballow|\bfalse\b|\boff\b|ignore|exclude|disable|examine|skip|rustflags|force|runner|wrapper|clippy_conf",
    re.IGNORECASE,
)
ALLOW_LEVEL = re.compile(r"""["']allow["']""")
# A tool reads the nearest of these first, so a new one anywhere can override the config in LINT_CONFIG.
NEW_CONFIG = re.compile(
    r"(^|/)(\.?clippy\.toml|\.?ruff\.toml|pyproject\.toml|\.?rustfmt\.toml|\.markdownlint[^/]*|\.sqlfluff"
    r"|\.sqlfluffignore|\.squawk\.toml|\.editorconfig|\.cargo/(mutants\.toml|config(\.toml)?)|\.shellcheckrc"
    r"|\.jscpd[^/]*|whitelizard\.txt|setup\.cfg|tox\.ini)$"
)
CRATE_DECLARATION = re.compile(r"^(tier|kind)\s*=")
CRATE_MANIFEST = re.compile(r"^crates/[^/]+/Cargo\.toml$")
CRATE_LINTS = re.compile(r"^\[lints\]|^(lints\.)?workspace\s*=\s*true")
WORKSPACE_LINTS = re.compile(
    r"^\[lints\]\s*\n(\s*(#.*)?\n)*\s*workspace\s*=\s*true|^lints\.workspace\s*=\s*true"
    r"|^lints\s*=\s*\{\s*workspace\s*=\s*true",
    re.MULTILINE,
)
DIFF_FLAGS = (
    "--no-renames",
    "--no-color",
    "--no-ext-diff",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "--unified=0",
)

SUPPRESSION = re.compile(
    r"#!?\[(allow|expect)\(|cfg_attr\(.*allow\(|@ts-ignore|@ts-nocheck|@ts-expect-error"
    r"|eslint-disable|biome-ignore|#\s*((flake8|ruff):\s*)?noqa|#\s*nosec|#\s*type:\s*ignore"
    r"|pyright:\s*ignore|#\s*pragma:\s*no cover|pylint:\s*disable|shellcheck disable"
    r"|istanbul ignore|nosemgrep|gitleaks:allow|Stryker disable|continue-on-error|exclude_re"
    r"|squawk-ignore|--\s*noqa|exclude_globs|examine_(re|globs)|mutants::skip|cfg\(not\(coverage\)\)"
    r"|coverage\(off\)|#lizard forgive|jscpd:ignore|rustfmt::skip",
    re.IGNORECASE,
)
WRITTEN_REASON = re.compile(r"\breason\s*[=:]", re.IGNORECASE)
NO_VERIFY = re.compile(r"--no-verify")
MARKDOWN_SUPPRESSION = re.compile(r"markdownlint-(disable|configure)")
STUB = re.compile(
    r"\btodo!\(|\bunimplemented!\(|raise NotImplementedError|\bTODO\b|\bFIXME\b"
    r"|catch\s*(\([^)]*\))?\s*\{\s*\}|except\b.*:\s*pass\b"
    r"|Err\((_\w*|\.\.)\)\s*(=>\s*(\{\s*\}|\(\))|=\s*\S+\s*\{\s*\})"
)
SKIP = re.compile(
    r"#\[ignore|cfg_attr\(.*\bignore\b|#!?\[cfg\(any\(\s*\)\)\]|\b(it|test|describe)\.(skip|todo|only)\b|\bxit\(|\bxdescribe\("
    r"|unittest\.skip|skipTest\(|@skip\w*\(|expectedFailure"
    r"|pytest\.mark\.(skip|xfail)|pytest\.skip\(|t\.Skip\("
)
ASSERTION = re.compile(r"\b(assert\w*|expect|should)\b")
RUST_TEST = re.compile(r"#\[(\w+::)?test\b|\bassert\w*!")
TEST_PATH = re.compile(r"(^|/)tests?/|(^|/)test_|_tests?\.|\.(test|spec)\.|(^|/)tests\.rs$")
EXCEPTION_ROW = re.compile(r"^\|\s*[A-Za-z]+-?\d+\s*\|")
REASON = re.compile(r"^Floor-reason:[ \t]*(\S.*)$", re.MULTILINE)


class CouldNotRunError(Exception):
    pass


def git(*args, allowed=(0,)):
    result = subprocess.run(["git", *args], capture_output=True, text=True, errors="replace", check=False)
    if result.returncode not in allowed:
        raise CouldNotRunError(f"git {args[0]} failed: {result.stderr.strip()}")
    return result.stdout


def read_diff(merge_base):
    """Returns the diff against the working tree, including untracked files."""
    untracked = git("ls-files", "--others", "--exclude-standard").splitlines()
    # --no-index exits 1 whenever the two sides differ, which is the normal case here.
    new_files = [git("diff", "--no-index", *DIFF_FLAGS, "/dev/null", f, allowed=(0, 1)) for f in untracked]
    return "\n".join([git("diff", *DIFF_FLAGS, merge_base, "--"), *new_files])


def parse_diff(diff):
    """Returns (added, removed, deleted, created), where added and removed are (file, text) pairs."""
    added, removed, deleted, created = [], [], [], []
    current, old, in_header = "", "", False
    for line in diff.splitlines():
        if line.startswith("diff "):
            in_header = True
        elif line.startswith("@@"):
            in_header = False
        elif in_header and line.startswith("--- "):
            old = header_path(line)
        elif in_header and line.startswith("+++ "):
            new = header_path(line)
            current = old if new == "/dev/null" else new
            if "/dev/null" in (old, new):
                (deleted if new == "/dev/null" else created).append(current)
        elif in_header:
            continue
        elif line.startswith("+"):
            added.append((current, line[1:]))
        elif line.startswith("-"):
            removed.append((current, line[1:]))
    return added, removed, deleted, created


def header_path(line):
    return re.sub(r"^[ab]/", "", line[4:].rstrip("\t"))


def scan_code(added):
    previous = ("", "")
    for path, text in added:
        if path not in SCAN_EXEMPT:
            yield from scan_line(path, text, previous)
        previous = (path, text)


def scan_line(path, text, previous):
    if path.endswith(".md"):
        if MARKDOWN_SUPPRESSION.search(text):
            yield "silenced-checker", path, text
        return
    if NO_VERIFY.search(text) or (SUPPRESSION.search(text) and not WRITTEN_REASON.search(text)):
        yield "silenced-checker", path, text
    empty_handler = (
        text.strip() == "pass" and previous[0] == path and previous[1].strip().startswith("except")
    )
    if STUB.search(text) or empty_handler:
        yield "unfinished-work", path, text
    if SKIP.search(text):
        yield "test-made-easier", path, text


def scan_tests(removed, deleted):
    for path in deleted:
        if TEST_PATH.search(path):
            yield "test-deleted", path, "file deleted"
    for path, text in removed:
        in_test_file = TEST_PATH.search(path) and path not in deleted and ASSERTION.search(text)
        if in_test_file or (path.endswith(".rs") and RUST_TEST.search(text)):
            yield "assertion-removed", path, text


def scan_protected(added, removed, created):
    for path, text in removed:
        crate_manifest = CRATE_MANIFEST.match(path)
        if path in PROTECTED:
            yield "guard-weakened", path, text
        elif crate_manifest and CRATE_DECLARATION.match(text):
            yield "tier-changed", path, text
        elif crate_manifest and CRATE_LINTS.match(text.strip()):
            yield "lints-dropped", path, text
    for path, text in added:
        loosens = LOOSENING if path in LINT_CONFIG else ALLOW_LEVEL if path.endswith("Cargo.toml") else None
        if loosens and loosens.search(text):
            yield "guard-weakened", path, text
    for path in created:
        if NEW_CONFIG.search(path):
            yield "config-added", path, "new config file"
        if CRATE_MANIFEST.match(path) and not WORKSPACE_LINTS.search(Path(path).read_text(errors="replace")):
            yield "lints-dropped", path, "new crate without the workspace lints"


def exception_rows():
    """Returns the table rows in the Exceptions section of CONSTRAINTS.md today."""
    path = Path("CONSTRAINTS.md")
    if not path.exists():
        return set()
    section = path.read_text().partition("## Exceptions")[2]
    return {line for line in section.splitlines() if line.startswith("|")}


def scan_constraints(added, removed):
    """Adding a rule is silent. Adding an exception or removing anything else is not."""
    rows = exception_rows()
    for path, text in added:
        if path.endswith("CONSTRAINTS.md") and text in rows:
            yield "new-exception", path, text
    for path, text in removed:
        if path.endswith("CONSTRAINTS.md") and text.strip() and not EXCEPTION_ROW.match(text):
            yield "rule-removed", path, text


def reasons_for(path, merge_base):
    return REASON.findall(git("log", "--format=%B", f"{merge_base}..HEAD", "--", path))


def report(findings, merge_base):
    """Prints each finding and returns how many have no reason."""
    unexcused = 0
    for rule, path, text in findings:
        reasons = reasons_for(path, merge_base)
        unexcused += not reasons
        label = f"warning, Floor-reason: {reasons[0]}" if reasons else "violation"
        print(f"  [{rule}] {path}: {text.strip()[:120]} ({label})", file=sys.stderr)
    if unexcused:
        print(
            "Fix the code. If the maintainer has approved this, add a "
            '"Floor-reason: <why>" trailer to a commit that touches the file.',
            file=sys.stderr,
        )
    return unexcused


def main():
    parser = argparse.ArgumentParser(description="Fails a diff that lowers the bar.")
    parser.add_argument("--base", default="origin/main")
    base = parser.parse_args().base
    try:
        top = Path(git("rev-parse", "--show-toplevel").strip())
        os.chdir(top)
        merge_base = git("merge-base", base, "HEAD").strip()
        added, removed, deleted, created = parse_diff(read_diff(merge_base))
        findings = [
            *scan_code(added),
            *scan_tests(removed, deleted),
            *scan_protected(added, removed, created),
            *scan_constraints(added, removed),
        ]
        if not findings:
            print("floor-guard: clean")
            return 0
        print(f"floor-guard: {len(findings)} finding(s):", file=sys.stderr)
        return 1 if report(findings, merge_base) else 0
    except CouldNotRunError as error:
        print(f"floor-guard: could not run: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
