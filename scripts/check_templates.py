"""Checks templates against the built stylesheet (ADR-0032). Fails on a class the stylesheet does not define,
which Tailwind drops without a word; on `{{ }}` inside a class attribute, since Tailwind finds only class
names written whole; and on a `style=` attribute, which the content security policy blocks; and on a
transform utility, such as `-translate-x-full`, inside `starting:`, since WebKit ignores the variables
Tailwind registers for it in `@starting-style`, so nothing would animate in Safari or any iOS browser; write
a plain value, such as `starting:open:[translate:-100%_0]`. Marker classes such as `group` and `peer` style
nothing themselves, so they pass.

Usage: check_templates.py STYLESHEET TEMPLATE...
"""

import re
import sys
from pathlib import Path

CLASS = re.compile(r"""\bclass=(?P<quote>["'])(?P<value>.*?)(?P=quote)""", re.DOTALL)
STYLE = re.compile(r"\sstyle\s*=|<style\b", re.IGNORECASE)
TAG = re.compile(r"\{%.*?%\}", re.DOTALL)
# A class selector as Tailwind writes it, with special characters escaped by a backslash.
SELECTOR = re.compile(r"\.((?:\\.|[A-Za-z0-9_-])+)")
MARKERS = re.compile(r"^(group|peer)(/[\w-]+)?$")
# A `starting:` class whose utility sets a transform through Tailwind's registered variables.
STARTING_TRANSFORM = re.compile(r"^(?:[\w-]+:)*starting:(?:[\w-]+:)*-?(?:translate|scale|rotate|skew)-")


def defined(css):
    """Every class name the stylesheet has a rule for, unescaped."""
    return {re.sub(r"\\(.)", r"\1", name) for name in SELECTOR.findall(css)}


def problems(path, text, classes):
    found = []
    for match in STYLE.finditer(text):
        line = text.count("\n", 0, match.start()) + 1
        found.append(
            f"{path}:{line}: a style= attribute or <style> element, which the policy blocks; use a class"
        )
    for match in CLASS.finditer(text):
        line = text.count("\n", 0, match.start()) + 1
        value = match["value"]
        if "{{" in value:
            found.append(f"{path}:{line}: {{{{ }}}} inside a class; write each class name whole")
            continue
        names = TAG.sub(" ", value).split()
        found.extend(
            f"{path}:{line}: {name} animates nothing in WebKit; write a plain value, as [translate:-100%_0]"
            for name in names
            if STARTING_TRANSFORM.match(name)
        )
        missing = [n for n in names if n not in classes and not MARKERS.match(n)]
        found.extend(f"{path}:{line}: {name} is not in the built stylesheet" for name in missing)
    return found


def main():
    stylesheet, *templates = sys.argv[1:] or [""]
    if not templates:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    classes = defined(Path(stylesheet).read_text())
    found = [p for name in templates for p in problems(name, Path(name).read_text(), classes)]
    for problem in found:
        print(problem, file=sys.stderr)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
