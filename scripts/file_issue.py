#!/usr/bin/env python3
"""Files an issue onto the Tetsu board with its milestone, tier, and priority, in one step.

usage: scripts/file_issue.py <task|adr> <title> <body file> --tier <1|2|3|none>
           --priority <now|next|later|anytime> [--milestone <title>]

Everything the board needs is looked up before the issue exists, so a bad value or a changed board fails with
nothing filed. If the board refuses an edit after that, the error names the filed issue and what is still
unset, so a rerun does not file it twice. GitHub's own workflows set the status: Todo when added, and Done
when closed.
"""

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

OWNER = "heavymedl-labs"
REPOSITORY = f"{OWNER}/tetsu"
PROJECT = "1"
TIERS = {"1": "1", "2": "2", "3": "3", "none": "None"}
# How many times to look for an item auto-add made before giving up.
LOOKS = 5
PRIORITIES = {"now": "Now", "next": "Next", "later": "Later", "anytime": "Anytime"}


class FileIssueError(Exception):
    pass


def gh(*args):
    result = subprocess.run(["gh", *args], capture_output=True, text=True, check=False)
    if result.returncode != 0:
        raise FileIssueError(f"gh {' '.join(args[:2])} failed: {result.stderr.strip()}")
    return result.stdout.strip()


def earliest_open_milestone():
    milestones = json.loads(gh("api", f"repos/{REPOSITORY}/milestones?state=open"))
    if not milestones:
        raise FileIssueError("no open milestone; name one with --milestone")
    return min(milestones, key=lambda milestone: milestone["number"])["title"]


def board_item(url):
    """The issue's item on the board. The board's auto-add workflow may add it first, and then adding it again
    fails, so that case finds the item it made."""
    try:
        return json.loads(
            gh("project", "item-add", PROJECT, "--owner", OWNER, "--url", url, "--format", "json")
        )["id"]
    except FileIssueError as error:
        if "already exists" not in str(error):
            raise
    # GitHub can take a few seconds to list an item auto-add just made, so look again before giving up.
    for attempt in range(LOOKS):
        if attempt:
            time.sleep(float(os.environ.get("FILE_ISSUE_RETRY_SECONDS", "2")))
        listed = gh("project", "item-list", PROJECT, "--owner", OWNER, "--limit", "1000", "--format", "json")
        items = json.loads(listed)["items"]
        match = next((i["id"] for i in items if (i.get("content") or {}).get("url") == url), None)
        if match is not None:
            return match
    raise FileIssueError("the board says the issue is on it, but its item was not found")


def option_ids(fields, wanted):
    """The field and option ids for each (field name, option name), or an error naming what is missing."""
    by_name = {field["name"]: field for field in fields}
    found = []
    for name, value in wanted:
        field = by_name.get(name)
        if field is None:
            raise FileIssueError(f"the board has no {name} field")
        option = next((o for o in field.get("options", []) if o["name"] == value), None)
        if option is None:
            raise FileIssueError(f"{name} has no option {value}")
        found.append((field["id"], option["id"]))
    return found


def file_issue(args):
    """Looks up everything the board needs, then files the issue and sets its fields. Returns its URL."""
    if not args.body.is_file():
        raise FileIssueError(f"{args.body} is not a file")
    milestone = args.milestone or earliest_open_milestone()
    project = json.loads(gh("project", "view", PROJECT, "--owner", OWNER, "--format", "json"))["id"]
    fields = json.loads(gh("project", "field-list", PROJECT, "--owner", OWNER, "--format", "json"))["fields"]
    edits = option_ids(fields, [("Tier", TIERS[args.tier]), ("Priority", PRIORITIES[args.priority])])
    url = gh(
        "issue",
        "create",
        "--repo",
        REPOSITORY,
        "--title",
        args.title,
        "--label",
        args.label,
        "--milestone",
        milestone,
        "--body-file",
        str(args.body),
    )
    try:
        item = board_item(url)
        for field, option in edits:
            gh(
                "project",
                "item-edit",
                "--project-id",
                project,
                "--id",
                item,
                "--field-id",
                field,
                "--single-select-option-id",
                option,
            )
    except FileIssueError as error:
        raise FileIssueError(
            f"filed {url}, but {error}. Set its tier ({TIERS[args.tier]}) and priority "
            f"({PRIORITIES[args.priority]}) on the board by hand rather than filing it again"
        ) from error
    return url


def main():
    parser = argparse.ArgumentParser(description="Files an issue onto the Tetsu board.")
    parser.add_argument("label", choices=["task", "adr"])
    parser.add_argument("title")
    parser.add_argument("body", type=Path)
    parser.add_argument("--tier", required=True, choices=sorted(TIERS))
    parser.add_argument("--priority", required=True, choices=sorted(PRIORITIES))
    parser.add_argument("--milestone")
    args = parser.parse_args()
    try:
        url = file_issue(args)
    except FileIssueError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(url)
    return 0


if __name__ == "__main__":
    sys.exit(main())
