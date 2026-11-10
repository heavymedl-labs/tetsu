"""Checks the dated ignore entries in a TOML list, such as deny.toml's advisories.ignore (ADR-0028).

Each entry is a one-line table whose reason ends "Tracked in #<issue>. Review by <YYYY-MM-DD>.". The date
may be at most 30 days after the commit that last changed the entry's line, or after today for a line not
yet committed, and must not have passed in UTC. Reading lines rather than parsing TOML keeps each entry to
one line, which git blame can date, and runs on the system Python, which may predate tomllib. So the list
has one accepted spelling: `KEY = [` on its own line under a bare `[SECTION]` header, one entry per line,
then `]`. Any other spelling the tool reading the file would accept fails, rather than going unread.

Usage: check_dated_entries.py FILE SECTION KEY [--today YYYY-MM-DD]
"""

import argparse
import datetime
import re
import subprocess
import sys
from pathlib import Path

ENTRY = re.compile(r'^\s*\{ id = "(?P<id>[^"]+)", reason = "(?P<reason>[^"]*)" \},?\s*$')
DATED = re.compile(r"Tracked in #\d+\. Review by (?P<date>\d{4}-\d{2}-\d{2})\.$")
MAX_DAYS = 30
# git blame names a line not yet committed with this id, and dates it now.
UNCOMMITTED = "0" * 40


def bare(name):
    """A TOML key or table name without its quotes or spaces, so other spellings of it compare equal."""
    return re.sub(r'[\s"\']', "", name)


def other_spelling(stripped, in_section, section, key):
    """Whether a line outside the accepted list would also give SECTION.KEY, or SECTION, a value."""
    header = re.match(r"^\[\[?(?P<name>[^\]]*)\]", stripped)
    if header:
        name = bare(header["name"])
        return name == f"{section}.{key}" or (name == section and stripped != f"[{section}]")
    assigned = re.match(r"^(?P<name>[^=]+?)\s*=", stripped)
    if not assigned:
        return False
    name = bare(assigned["name"])
    return name in (f"{section}.{key}", section) or (in_section and name == key)


def entry_lines(lines, section, key):
    """Yields (line number, text) for each line inside SECTION's KEY = [ ... ] list."""
    in_section, start, closed = False, None, False
    for number, text in enumerate(lines, 1):
        stripped = text.strip()
        inside = start is not None and not closed
        if inside and stripped == "]":
            closed = True
        elif inside:
            yield number, text
        elif in_section and start is None and stripped == f"{key} = [":
            start = number
        elif stripped and not stripped.startswith("#"):
            if other_spelling(stripped, in_section, section, key):
                raise ValueError(
                    f"line {number}: write {section}.{key} only as `{key} = [` on its own line under "
                    f"[{section}], one entry per line"
                )
            if stripped.startswith("["):
                in_section = stripped == f"[{section}]"
    if start is not None and not closed:
        raise ValueError(f"the {section}.{key} list is not closed")


def git(path, *args):
    return subprocess.run(["git", *args], cwd=path.parent, capture_output=True, text=True, check=False)


def commit_dates(path):
    """The UTC date each line was last committed, or None for a line not yet committed.

    Raises RuntimeError when git cannot say, since counting from today would loosen the rule.
    """
    if git(path, "rev-parse", "--is-inside-work-tree").returncode != 0:
        raise RuntimeError("git cannot read the repository, so entries cannot be dated")
    if git(path, "ls-files", "--error-unmatch", "--", path.name).returncode != 0:
        # A file git does not track yet is all new lines.
        return {}
    blame = git(path, "blame", "--porcelain", "--", path.name)
    if blame.returncode != 0:
        raise RuntimeError(f"git blame failed: {blame.stderr.strip()}")
    blame = blame.stdout
    dates, current, committed = {}, None, {}
    for line in blame.splitlines():
        header = re.match(r"^([0-9a-f]{40}) \d+ (\d+)", line)
        if header:
            current, number = header.group(1), int(header.group(2))
            dates[number] = current
        elif line.startswith("committer-time ") and current and current != UNCOMMITTED:
            stamp = datetime.datetime.fromtimestamp(int(line.split()[1]), datetime.timezone.utc)
            committed[current] = stamp.date()
    return {number: committed.get(commit) for number, commit in dates.items()}


def date_problem(reason, changed, today):
    """What is wrong with an entry's reason and review date, or None."""
    found = DATED.search(reason)
    if not found:
        return 'the reason must end "Tracked in #<issue>. Review by <YYYY-MM-DD>."'
    try:
        review = datetime.date.fromisoformat(found["date"])
    except ValueError:
        return f"{found['date']} is not a date"
    since = changed or today
    if (review - since).days > MAX_DAYS:
        return f"review by {review} is more than {MAX_DAYS} days after {since}"
    if review < today:
        return f"review by {review} has passed; renew or remove it"
    return None


def problem(number, text, changed, today, where):
    """What is wrong with one line of the list, or None."""
    if not text.strip() or text.strip().startswith("#"):
        return None
    entry = ENTRY.match(text)
    if not entry:
        return f"{where}:{number}: each entry must be a one-line table, {{ id = ..., reason = ... }}"
    why = date_problem(entry["reason"], changed, today)
    return why and f"{where}:{number}: {entry['id']}: {why}"


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("file", type=Path)
    parser.add_argument("section")
    parser.add_argument("key")
    parser.add_argument("--today", type=datetime.date.fromisoformat)
    args = parser.parse_args()
    today = args.today or datetime.datetime.now(datetime.timezone.utc).date()
    path = args.file.resolve()
    try:
        lines = list(entry_lines(path.read_text().splitlines(), args.section, args.key))
        changed = commit_dates(path)
    except (ValueError, RuntimeError) as error:
        print(f"{args.file}: {error}", file=sys.stderr)
        return 1
    problems = [p for n, t in lines if (p := problem(n, t, changed.get(n), today, args.file.name))]
    for found in problems:
        print(found, file=sys.stderr)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
