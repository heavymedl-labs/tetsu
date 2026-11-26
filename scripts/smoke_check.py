#!/usr/bin/env python3
"""Checks a deployed Tetsu's pages after a deploy (#137): each host answers its own pages at its root
(ADR-0094), and a page that does not exist answers as that host should, each with the security headers the
server sets. Waits for a sleeping free instance to wake, retrying while it cannot connect or the host
answers 502, 503, or 504. Prints each problem with its page, and fails if any. A host serves `site`, the
default, `admin`, or `staff`, and the site's check needs `--login`, the staff login its `/login` leads to.

usage: scripts/smoke_check.py <base URL> [--serves <host>] [--login <URL>] [--wait <s>] [--interval <s>]
"""

import http.client
import sys
import time
import urllib.parse

# What each host answers, by what it serves: the site its home page, with `/login` sending a visitor
# to staff login (ADR-0093), and a realm's host its login at its root (ADR-0094), with its root sending
# a visitor who is not logged in there. A page no tenant or site page can hold, since slugs are
# lowercase, is missing on the site, and so are a realm's pages; a realm's host sends a visitor who asks
# for a page it does not hold to log in, as it does for its own pages.
MISSING = "/Smoke-Check-Missing-Page"
PAGES = {
    "site": (("/", 200), ("/login", 303), (MISSING, 404), ("/settings/profile", 404)),
    "admin": (("/", 303), ("/login", 200), (MISSING, 303)),
    "staff": (("/", 303), ("/login", 200), (MISSING, 303)),
}
# Where a page sends a visitor: a realm host's root to its login, and the site's `/login` to the staff login
# `--login` names, which the site must have, so a wrong staff origin, such as the site's own, fails the check.
LOCATIONS = {"admin": {"/": "/login"}, "staff": {"/": "/login"}}
# A realm's own pages send a referrer to Tetsu alone, so a posted form carries its real origin (ADR-0059);
# every other answer, the site's and a realm host's root and missing pages, sends none.
REALM_PAGES = {"admin": "/login", "staff": "/login"}
# What ADR-0029 and ASVS 3.4 ask every response to carry. The content security policy grows as pages need
# more, so only the directives that must never loosen are checked.
NEVER_LOOSENED = ("default-src 'none'", "object-src 'none'", "frame-ancestors 'none'")
HEADERS = {
    "strict-transport-security": lambda value: value.startswith("max-age=31536000"),
    "x-content-type-options": lambda value: value == "nosniff",
    "x-frame-options": lambda value: value == "DENY",
    "content-security-policy": lambda value: all(directive in value for directive in NEVER_LOOSENED),
}
WAKING = (502, 503, 504)
TIMEOUT = 30


def fetch(url):
    """The status and headers of a GET, with any status at all, or None when nothing answered. Only HTTP and
    HTTPS, which `main` checks, and no redirects, so each page is judged as it answers."""
    parts = urllib.parse.urlsplit(url)
    secure = parts.scheme == "https"
    connection = (http.client.HTTPSConnection if secure else http.client.HTTPConnection)(
        parts.netloc, timeout=TIMEOUT
    )
    try:
        connection.request("GET", parts.path or "/", headers={"User-Agent": "tetsu-smoke-check"})
        response = connection.getresponse()
    except (OSError, http.client.HTTPException):
        return None
    else:
        return response.status, response.headers
    finally:
        connection.close()


def problems(path, expected, answer, location=None, referrer="no-referrer"):
    """What is wrong with one page's answer, where it sends a visitor when it should send them to `location`,
    and the referrer policy it should send."""
    status, headers = answer
    found = [] if status == expected else [f"{path}: status {status}, expected {expected}"]
    if location is not None and headers.get("location") != location:
        found.append(f"{path}: sends visitors to {headers.get('location')!r}, expected {location!r}")
    rules = dict(HEADERS, **{"referrer-policy": lambda value: value == referrer})
    for name, holds in rules.items():
        value = headers.get(name)
        if value is None:
            found.append(f"{path}: no {name} header")
        elif not holds(value):
            found.append(f"{path}: {name} is {value!r}")
    return found


def check(pages, wait, interval, clock=time.monotonic, sleep=time.sleep):
    """Every problem across `pages`, each a base URL, path, expected status, where it sends a visitor, and its
    referrer policy, once the host answers, or a single one if it never wakes."""
    deadline = clock() + wait
    found = []
    for base, path, expected, location, referrer in pages:
        while (answer := fetch(base + path)) is None or answer[0] in WAKING:
            if clock() >= deadline:
                shown = "no answer" if answer is None else f"status {answer[0]}"
                return [f"{path}: {shown} after waiting {wait} seconds for the site to wake"]
            sleep(interval)
        found += problems(path, expected, answer, location, referrer)
    return found


def referrer_of(serves, path):
    """The referrer policy a page on a host serving `serves` sends."""
    return "same-origin" if REALM_PAGES.get(serves) == path else "no-referrer"


def main(args):
    base, options = (args[0], args[1:]) if args else ("", [])
    settings = {"--serves": "site", "--login": "", "--wait": "600", "--interval": "10"}
    pairs = list(zip(options[::2], options[1::2], strict=False))
    rules = {
        "--serves": lambda value: value in PAGES,
        "--login": lambda value: value.startswith(("https://", "http://")),
    }
    valid = (
        base.startswith(("https://", "http://"))
        and len(options) % 2 == 0
        and all(name in settings and rules.get(name, str.isdigit)(value) for name, value in pairs)
    )
    settings.update(pairs if valid else [])
    valid = valid and (settings["--serves"] != "site" or settings["--login"] != "")
    if not valid:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    serves = settings["--serves"]
    locations = LOCATIONS.get(serves, {"/login": settings["--login"]})
    pages = [
        (base.rstrip("/"), path, expected, locations.get(path), referrer_of(serves, path))
        for path, expected in PAGES[serves]
    ]
    found = check(pages, int(settings["--wait"]), int(settings["--interval"]))
    for problem in found:
        print(f"error: {problem}", file=sys.stderr)
    if not found:
        print(f"{base}: every page answered as expected")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
