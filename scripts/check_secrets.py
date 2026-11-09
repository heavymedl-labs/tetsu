#!/usr/bin/env python3
"""Fails if docs/operations/secrets.md misses a secret or permission a workflow uses, or a yearly rotation.

ADR-0027: secrets rotate yearly, so a last-rotated date over 13 months old fails. The list names every
secret a workflow uses and every GITHUB_TOKEN permission each workflow sets, and lists no workflow that is
gone. A date is YYYY-MM-DD, or automatic for a token GitHub issues per run.

usage: scripts/check_secrets.py [--today YYYY-MM-DD]
"""

import calendar
import datetime
import re
import sys
from pathlib import Path

LIST = Path("docs/operations/secrets.md")
WORKFLOWS = Path(".github/workflows")
MONTHS = 13
# A Markdown table starts with its header row and the row of dashes under it.
HEADER_ROWS = 2


def permissions(workflow):
    """Each permission a workflow sets, as `scope: level`, or `permissions: read-all` for the shorthand."""
    lines = workflow.splitlines()
    for number, line in enumerate(lines):
        found = re.match(r"^(\s*)permissions:\s*([^#\s]*)", line)
        if not found:
            continue
        indent, inline = len(found.group(1)), found.group(2)
        if inline:
            yield f"permissions: {inline}"
            continue
        for inner in lines[number + 1 :]:
            if inner.strip() and len(inner) - len(inner.lstrip()) <= indent:
                break
            scope = re.match(r"^\s+([a-z-]+):\s*([a-z]+)", inner)
            if scope:
                yield f"{scope.group(1)}: {scope.group(2)}"


def table(text, heading):
    """The rows under a `## heading`, as cells with backticks dropped, or None if it has no table."""
    match = re.search(rf"^## {re.escape(heading)}\n(.*?)(?=^## |\Z)", text, re.M | re.S)
    lines = [line for line in (match.group(1) if match else "").splitlines() if line.startswith("|")]
    if len(lines) < HEADER_ROWS:
        return None
    return [[cell.strip().strip("`") for cell in line.strip("|").split("|")] for line in lines[HEADER_ROWS:]]


def deadline(rotated):
    """The same day 13 calendar months later, or the last day of that month if it is shorter."""
    months = rotated.month - 1 + MONTHS
    year, month = rotated.year + months // 12, months % 12 + 1
    return rotated.replace(year=year, month=month, day=min(rotated.day, calendar.monthrange(year, month)[1]))


def parsed(rotated):
    """A YYYY-MM-DD date, or None for anything else."""
    if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", rotated):
        return None
    try:
        return datetime.date.fromisoformat(rotated)
    except ValueError:
        return None


def rotation_problem(name, rotated, today):
    if rotated == "automatic":
        return None
    date = parsed(rotated)
    if date is None:
        return f"{name}: last rotated must be YYYY-MM-DD or automatic"
    if date > today:
        return f"{name} was last rotated {rotated}, after today"
    if today > deadline(date):
        return f"{name} was last rotated {rotated}, over {MONTHS} months ago; rotate it (ADR-0027)"
    return None


def problems(text, workflows, today):
    secrets = table(text, "Secrets")
    tokens = table(text, "Workflow tokens")
    if secrets is None:
        yield "no Secrets table"
    if tokens is None:
        yield "no Workflow tokens table"
        return
    for name, *_, rotated in secrets or []:
        problem = rotation_problem(name, rotated, today)
        if problem:
            yield problem
    named = {row[0] for row in secrets or []}
    rows = {row[0]: "|".join(row[1:]) for row in tokens}
    for name in sorted(set(rows) - set(workflows)):
        yield f"{name} is listed but no such workflow exists"
    for name, workflow in sorted(workflows.items()):
        yield from workflow_problems(name, workflow, rows.get(name), named)


def workflow_problems(name, workflow, row, named):
    """What one workflow sets or uses that the list leaves out."""
    if row is None:
        yield f"{name} has no row in the workflow tokens table"
        return
    for permission in sorted(set(permissions(workflow))):
        if permission not in row:
            yield f"{name} sets {permission}, which its row does not list"
    for secret in sorted(set(re.findall(r"secrets\.([A-Za-z0-9_]+)", workflow)) - named):
        yield f"{name} uses secrets.{secret}, which has no row in the secrets table"


def main(args):
    today = datetime.date.today() if not args else None
    if args[:1] == ["--today"] and len(args) == len(["--today", "date"]):
        today = parsed(args[1])
    if today is None:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    if not LIST.is_file():
        print(f"error: {LIST} is missing", file=sys.stderr)
        return 1
    workflows = {path.name: path.read_text() for path in WORKFLOWS.glob("*.y*ml")}
    found = list(problems(LIST.read_text(), workflows, today))
    for problem in found:
        print(f"error: {LIST}: {problem}", file=sys.stderr)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
