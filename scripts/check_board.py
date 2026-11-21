#!/usr/bin/env python3
"""Lists each issue that breaks CONTRIBUTING.md's board rules, so a session can fix them before it ends.

An open issue must be on the board, with a type, a milestone, a tier, and a priority. A Task, Decision, or Bug
also needs a parent feature and an `area:` label; a Feature needs neither. A closed issue on the board must be
Done. Prints one line per issue that needs fixing and exits 1, or says the board is tidy and exits 0. Exits 2
when GitHub cannot be read.

usage: scripts/check_board.py
"""

import json
import subprocess
import sys

OWNER = "heavymedl-labs"
PROJECT = "1"
# Every issue, open and closed, a page at a time: gh pages through it only with a variable named endCursor.
ISSUES = """
query($endCursor: String) {
  repository(owner: "heavymedl-labs", name: "tetsu") {
    issues(first: 100, after: $endCursor) {
      pageInfo { hasNextPage endCursor }
      nodes {
        number title state
        issueType { name }
        milestone { title }
        labels(first: 20) { nodes { name } }
        parent { number }
      }
    }
  }
}
"""


class ReadError(Exception):
    pass


def gh(*args):
    result = subprocess.run(["gh", *args], capture_output=True, text=True, check=False)
    if result.returncode != 0:
        raise ReadError(f"could not read gh {' '.join(args[:2])}: {result.stderr.strip()}")
    return json.loads(result.stdout)


def issues():
    pages = gh("api", "graphql", "--paginate", "--slurp", "-f", f"query={ISSUES}")
    return [issue for page in pages for issue in page["data"]["repository"]["issues"]["nodes"]]


def board_items():
    listed = gh("project", "item-list", PROJECT, "--owner", OWNER, "--limit", "1000", "--format", "json")
    numbered = (item for item in listed["items"] if item.get("content", {}).get("number"))
    return {item["content"]["number"]: item for item in numbered}


def problems(issue, item):
    """What one issue is missing, as short phrases, or none."""
    if issue["state"] != "OPEN":
        if item is not None and item.get("status") != "Done":
            return [f"closed, but the board says {item.get('status') or 'nothing'}"]
        return []
    if item is None:
        return ["not on the board"]
    kind = (issue.get("issueType") or {}).get("name")
    missing = [
        name
        for name, value in (
            ("type", kind),
            ("milestone", issue.get("milestone")),
            ("tier", item.get("tier")),
            ("priority", item.get("priority")),
        )
        if not value
    ]
    found = [f"no {name}" for name in missing]
    if kind != "Feature":
        if not issue.get("parent"):
            found.append("no parent feature")
        if not any(label["name"].startswith("area: ") for label in issue["labels"]["nodes"]):
            found.append("no area: label")
    return found


def main():
    try:
        every, items = issues(), board_items()
    except ReadError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    broken = 0
    for issue in sorted(every, key=lambda issue: issue["number"]):
        found = problems(issue, items.get(issue["number"]))
        if found:
            broken += 1
            print(f"#{issue['number']} {issue['title']}: {', '.join(found)}")
    if broken:
        print(
            f"{broken} issue{'s' if broken > 1 else ''} need{'' if broken > 1 else 's'} fixing;"
            " CONTRIBUTING.md#project-board says how"
        )
        return 1
    opened = sum(issue["state"] == "OPEN" for issue in every)
    print(f"the board is tidy: {opened} open issues, {len(every) - opened} closed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
