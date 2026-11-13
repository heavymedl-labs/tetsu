"""Checks npm dependencies as ADR-0034 asks, from package.json, its lockfile, and .config/npm-checks.toml.

Offline it checks sources, licenses, install scripts, exact versions, and overrides. Given `npm audit --json`
output and a registry, it also fails on advisories, ignore entries that no longer match, and deprecated
packages. Dates on ignore and override entries are scripts/check_dated_entries.py's to check.

Exits 1 on any problem, and 3 when the audit or the registry could not be reached, so check.sh can fall back
to the last passing check of the same lockfile.

Usage: check_npm.py ROOT [--audit FILE --registry URL]
"""

import argparse
import concurrent.futures
import json
import re
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path

# Run as a script or imported by the tests, it finds its sibling either way.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from check_dated_entries import ENTRY, entry_lines

REGISTRY = "https://registry.npmjs.org/"
EXACT = re.compile(r"^\d+\.\d+\.\d+$")
STRING = re.compile(r'^\s*"(?P<value>[^"]+)",?\s*$')
UNREACHABLE = 3
# What npm prints when it cannot reach the registry, as opposed to a check that failed.
OFFLINE = re.compile(r"ENOTFOUND|EAI_AGAIN|ETIMEDOUT|ECONNREFUSED|ECONNRESET|ENETUNREACH|network")
GHSA = re.compile(r"GHSA-[0-9a-z]{4}-[0-9a-z]{4}-[0-9a-z]{4}")


def listed(lines, section, key):
    """The quoted strings, or the ids of the one-line tables, in SECTION's KEY list."""
    values = []
    for _, text in entry_lines(lines, section, key):
        found = STRING.match(text) or ENTRY.match(text)
        if found:
            values.append(found.groupdict().get("value") or found["id"])
    return values


def allowed(expression, licenses):
    """Whether an SPDX expression is satisfied by the allowed licenses: OR needs one side, AND both."""
    tokens = re.findall(r"\(|\)|[^\s()]+", expression)

    def either():
        result = both()
        while tokens and tokens[0] == "OR":
            tokens.pop(0)
            result = both() or result
        return result

    def both():
        result = one()
        while tokens and tokens[0] == "AND":
            tokens.pop(0)
            result = one() and result
        return result

    def one():
        part = tokens.pop(0) if tokens else ""
        if part == "(":
            result = either()
            if tokens:
                tokens.pop(0)
            return result
        if tokens and tokens[0] == "WITH":
            tokens[:2] = []
        return part in licenses

    return bool(tokens) and either() and not tokens


def name_of(path):
    return path.rsplit("node_modules/", 1)[-1]


def source_problems(path, package):
    if package.get("link"):
        target = package.get("resolved", "")
        if target.startswith(("/", "..")) or ":" in target:
            return [f"{path}: links outside the repository, to {target}"]
        return []
    if package.get("inBundle"):
        return []
    found = []
    if not package.get("resolved", "").startswith(REGISTRY):
        found.append(f"{path}: comes from {package.get('resolved')}, not the npm registry")
    if not package.get("integrity", "").startswith("sha512-"):
        found.append(f"{path}: has no sha512 integrity hash")
    return found


def package_problems(path, package, licenses, scripts):
    found = source_problems(path, package)
    if package.get("link"):
        return found
    license = package.get("license")
    if not isinstance(license, str):
        found.append(f"{path}: names no license")
    elif not allowed(license, licenses):
        found.append(f"{path}: {license} is not an allowed license")
    name = name_of(path)
    if package.get("hasInstallScript") and name not in scripts["denied"]:
        found.append(f"{path}: has an install script, so it must be listed in npm-checks.toml")
    elif package.get("hasInstallScript") and scripts["allow"].get(name) is not False:
        found.append(f"{path}: its install script must be denied in package.json's allowScripts")
    return found


def manifest_problems(manifest, overrides):
    found = []
    for field in ("dependencies", "devDependencies", "overrides"):
        for name, version in manifest.get(field, {}).items():
            if not (isinstance(version, str) and EXACT.match(version)):
                found.append(f"package.json: {field} {name} must be an exact version, not {version}")
    given = set(manifest.get("overrides", {}))
    for name in sorted(given ^ set(overrides)):
        where = "has no entry in npm-checks.toml" if name in given else "is not in package.json's overrides"
        found.append(f"override {name} {where}")
    return found


def advisory_problems(audit, ignored):
    seen, found = set(), []
    for name, vulnerability in audit.get("vulnerabilities", {}).items():
        for via in vulnerability.get("via", []):
            # A string names another package whose own entry carries the advisory.
            if isinstance(via, dict):
                ids = GHSA.findall(via.get("url", ""))
                seen.update(ids)
                if not ids:
                    found.append(f"node_modules/{name}: an advisory with no GHSA id, {via.get('url')}")
    found += [f"advisory {ghsa} is not ignored in npm-checks.toml" for ghsa in sorted(seen - set(ignored))]
    found += [
        f"ignored advisory {ghsa} no longer matches any package" for ghsa in sorted(set(ignored) - seen)
    ]
    return found


def deprecation(registry, path, package):
    url = f"{registry.rstrip('/')}/{name_of(path)}/{package['version']}"
    try:
        with urllib.request.urlopen(url, timeout=30) as reply:  # noqa: S310 reason: npm's registry, or a test's
            note = json.load(reply).get("deprecated")
    except urllib.error.HTTPError as error:
        # Not found is an answer: the version is gone. Any other status means the registry could not say.
        if error.code == 404:  # noqa: PLR2004 reason: HTTP's not found
            return f"{path}: {package['version']} is not on the registry"
        raise
    return f"{path}: is deprecated: {note}" if note else None


def signature_problems(root):
    """`npm audit signatures`: each installed package's registry signature (ADR-0034)."""
    result = subprocess.run(
        ["npm", "audit", "signatures", "--include=dev"],
        cwd=root,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode == 0:
        return []
    output = (result.stderr + result.stdout).strip()
    if OFFLINE.search(output):
        raise urllib.error.URLError(f"npm audit signatures: {output.splitlines()[-1]}")
    return [f"registry signatures did not verify: {line}" for line in output.splitlines()[-5:]]


def deprecation_problems(registry, packages):
    from_registry = {p: v for p, v in packages.items() if not v.get("link") and not v.get("inBundle")}
    with concurrent.futures.ThreadPoolExecutor(16) as pool:
        notes = pool.map(lambda item: deprecation(registry, *item), from_registry.items())
        return [note for note in notes if note]


def problems(root, audit, registry):
    lines = (root / ".config" / "npm-checks.toml").read_text().splitlines()
    manifest = json.loads((root / "package.json").read_text())
    lock = json.loads((root / "package-lock.json").read_text())
    packages = {path: package for path, package in lock["packages"].items() if path}
    licenses = set(listed(lines, "licenses", "allow"))
    scripts = {
        "denied": listed(lines, "install-scripts", "denied"),
        "allow": manifest.get("allowScripts", {}),
    }
    found = manifest_problems(manifest, listed(lines, "overrides", "entries"))
    for path, package in packages.items():
        found += package_problems(path, package, licenses, scripts)
    names = {name_of(p) for p, v in packages.items() if v.get("hasInstallScript")}
    found += [
        f"install script {n} is listed, but no package has one" for n in scripts["denied"] if n not in names
    ]
    if audit is not None:
        found += advisory_problems(audit, listed(lines, "advisories", "ignore"))
        found += deprecation_problems(registry, packages)
        found += signature_problems(root)
    return found


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("root", type=Path)
    parser.add_argument("--audit", type=Path)
    parser.add_argument("--registry", default=REGISTRY)
    args = parser.parse_args()
    audit = json.loads(args.audit.read_text() or "{}") if args.audit else None
    if audit is not None and ("error" in audit or "vulnerabilities" not in audit):
        print("could not audit: npm audit gave no report", file=sys.stderr)
        return UNREACHABLE
    try:
        found = problems(args.root, audit, args.registry)
    except ValueError as error:
        found = [f"npm-checks.toml: {error}"]
    except (urllib.error.URLError, TimeoutError) as error:
        print(f"could not reach the registry: {error}", file=sys.stderr)
        return UNREACHABLE
    for problem in found:
        print(problem, file=sys.stderr)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
