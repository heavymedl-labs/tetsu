#!/usr/bin/env python3
"""Checks a deployed Tetsu's pages after a deploy (#137): the home page answers 200 and a page that does not
exist answers 404, each with the security headers the server sets. Waits for a sleeping free instance to wake,
retrying while it cannot connect or the host answers 502, 503, or 504. Prints each problem with its page, and
fails if any.

usage: scripts/smoke_check.py <base URL> [--wait <seconds>] [--interval <seconds>]
"""

import http.client
import sys
import time
import urllib.parse

# The page Tetsu serves at the root, and one no tenant or site page can hold, since slugs are lowercase.
PAGES = (("/", 200), ("/Smoke-Check-Missing-Page", 404))
# What ADR-0029 and ASVS 3.4 ask every response to carry. The content security policy grows as pages need
# more, so only the directives that must never loosen are checked.
NEVER_LOOSENED = ("default-src 'none'", "object-src 'none'", "frame-ancestors 'none'")
HEADERS = {
    "strict-transport-security": lambda value: value.startswith("max-age=31536000"),
    "x-content-type-options": lambda value: value == "nosniff",
    "referrer-policy": lambda value: value == "no-referrer",
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


def problems(path, expected, answer):
    """What is wrong with one page's answer."""
    status, headers = answer
    found = [] if status == expected else [f"{path}: status {status}, expected {expected}"]
    for name, holds in HEADERS.items():
        value = headers.get(name)
        if value is None:
            found.append(f"{path}: no {name} header")
        elif not holds(value):
            found.append(f"{path}: {name} is {value!r}")
    return found


def check(base, wait, interval, clock=time.monotonic, sleep=time.sleep):
    """Every problem across the pages, once the site answers, or a single one if it never wakes."""
    deadline = clock() + wait
    found = []
    for path, expected in PAGES:
        while (answer := fetch(base + path)) is None or answer[0] in WAKING:
            if clock() >= deadline:
                shown = "no answer" if answer is None else f"status {answer[0]}"
                return [f"{path}: {shown} after waiting {wait} seconds for the site to wake"]
            sleep(interval)
        found += problems(path, expected, answer)
    return found


def main(args):
    base, options = (args[0], args[1:]) if args else ("", [])
    settings = {"--wait": 600, "--interval": 10}
    pairs = list(zip(options[::2], options[1::2], strict=False))
    valid = (
        base.startswith(("https://", "http://"))
        and len(options) % 2 == 0
        and all(name in settings and value.isdigit() for name, value in pairs)
    )
    if not valid:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    settings.update({name: int(value) for name, value in pairs})
    found = check(base.rstrip("/"), settings["--wait"], settings["--interval"])
    for problem in found:
        print(f"error: {problem}", file=sys.stderr)
    if not found:
        print(f"{base}: every page answered as expected")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
