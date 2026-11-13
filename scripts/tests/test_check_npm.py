"""npm dependencies are checked like Rust's (ADR-0034): sources, licenses, scripts, and advisories."""

import json
import tempfile
import unittest
import urllib.error
from pathlib import Path
from unittest import mock

from scripts import check_npm
from scripts.tests.support import ROOT, run, write

SCRIPT = ROOT / "scripts" / "check_npm.py"
REASON = "Tracked in #95. Review by 2026-12-10."


def config(ignore=(), overrides=(), denied=(), licenses=("MIT", "Apache-2.0")):
    def table(names):
        return "".join(f'    {{ id = "{n}", reason = "x. {REASON}" }},\n' for n in names)

    def strings(names):
        return "".join(f'    "{n}",\n' for n in names)

    return (
        f"[licenses]\nallow = [\n{strings(licenses)}]\n\n[advisories]\nignore = [\n{table(ignore)}]\n\n"
        f"[overrides]\nentries = [\n{table(overrides)}]\n\n"
        f"[install-scripts]\ndenied = [\n{strings(denied)}]\n"
    )


def registry_package(version="1.2.3", license="MIT", **extra):
    return {
        "version": version,
        "resolved": f"https://registry.npmjs.org/a/-/a-{version}.tgz",
        "integrity": "sha512-abc",
        "license": license,
        **extra,
    }


class NpmChecksTest(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(lambda: run(["rm", "-rf", self.root], ROOT))
        self.registry = self.root / "registry"
        self.manifest = {"devDependencies": {"a": "1.2.3"}}
        self.locked = {"node_modules/a": registry_package()}
        self.settings = config()
        self.audit = {"vulnerabilities": {}}

    def stub_npm(self, body):
        """A stand-in npm for `npm audit signatures`, first on the PATH."""
        write(self.root / "bin" / "npm", f"#!/bin/sh\n{body}\n", 0o755)

    def check(self, online=True):
        write(self.root / "package.json", json.dumps(self.manifest))
        packages = {"": {"name": "t"}, **self.locked}
        write(self.root / "package-lock.json", json.dumps({"lockfileVersion": 3, "packages": packages}))
        write(self.root / ".config" / "npm-checks.toml", self.settings)
        for path, package in self.locked.items():
            if "version" not in package:
                continue
            name = path.rsplit("node_modules/", 1)[-1]
            info = {"deprecated": package["deprecated"]} if "deprecated" in package else {}
            write(self.registry / name / package["version"], json.dumps(info))
        args = ["python3", SCRIPT, self.root]
        if not (self.root / "bin" / "npm").exists():
            self.stub_npm('echo "audited 1 package"')
        if online:
            write(self.root / "audit.json", json.dumps(self.audit))
            args += ["--audit", self.root / "audit.json", "--registry", self.registry.as_uri()]
        return run(args, ROOT, PATH=f"{self.root / 'bin'}:/usr/bin:/bin")

    def assert_fails(self, *words):
        result = self.check()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        for word in words:
            self.assertIn(word, result.stderr)

    def test_a_clean_tree_passes(self):
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_a_version_range_fails(self):
        self.manifest["devDependencies"]["a"] = "^1.2.3"
        self.assert_fails("a", "exact")

    def test_a_source_other_than_the_registry_fails(self):
        for resolved in ("git+ssh://git@github.com/x/a.git", "file:../a", "https://example.com/a.tgz"):
            with self.subTest(resolved=resolved):
                self.locked["node_modules/a"]["resolved"] = resolved
                self.assert_fails("node_modules/a", "registry")

    def test_a_package_without_an_integrity_hash_fails(self):
        del self.locked["node_modules/a"]["integrity"]
        self.assert_fails("node_modules/a", "integrity")

    def test_bundled_and_local_workspace_packages_need_no_registry_source(self):
        self.locked["node_modules/a/node_modules/b"] = {
            "version": "1.0.0",
            "inBundle": True,
            "license": "MIT",
        }
        self.locked["node_modules/w"] = {"resolved": "web/packages/w", "link": True}
        self.assertEqual(self.check(online=False).returncode, 0, self.check(online=False).stderr)

    def test_a_link_outside_the_repository_fails(self):
        self.locked["node_modules/w"] = {"resolved": "../elsewhere", "link": True}
        self.assert_fails("node_modules/w", "outside")

    def test_a_license_outside_the_list_fails(self):
        self.locked["node_modules/a"]["license"] = "GPL-3.0-only"
        self.assert_fails("node_modules/a", "GPL-3.0-only")

    def test_license_expressions_are_read(self):
        for license, passes in (
            ("MIT OR GPL-3.0-only", True),
            ("(MIT OR Apache-2.0)", True),
            ("MIT AND Apache-2.0", True),
            ("MIT AND GPL-3.0-only", False),
            ("(MIT AND GPL-3.0-only) OR Apache-2.0", True),
        ):
            with self.subTest(license=license):
                self.locked["node_modules/a"]["license"] = license
                self.assertEqual(self.check().returncode == 0, passes, license)

    def test_a_package_with_no_license_fails(self):
        del self.locked["node_modules/a"]["license"]
        self.assert_fails("node_modules/a", "license")

    def test_an_install_script_must_be_denied_and_listed(self):
        self.locked["node_modules/a"]["hasInstallScript"] = True
        self.assert_fails("node_modules/a", "install script")
        self.settings = config(denied=["a"])
        self.assert_fails("allowScripts")
        self.manifest["allowScripts"] = {"a": False}
        self.assertEqual(self.check().returncode, 0, self.check().stderr)

    def test_a_listed_install_script_no_package_has_fails(self):
        self.settings = config(denied=["gone"])
        self.assert_fails("gone")

    def test_overrides_and_their_entries_must_match(self):
        self.manifest["overrides"] = {"b": "2.0.0"}
        self.assert_fails("b", "npm-checks.toml")
        self.settings = config(overrides=["b"])
        self.assertEqual(self.check().returncode, 0, self.check().stderr)
        self.manifest["overrides"] = {}
        self.assert_fails("b")

    def test_an_override_must_be_exact(self):
        self.manifest["overrides"] = {"b": "^2.0.0"}
        self.settings = config(overrides=["b"])
        self.assert_fails("b", "exact")

    def test_an_advisory_fails_unless_ignored_and_a_stale_ignore_fails(self):
        url = "https://github.com/advisories/GHSA-aaaa-bbbb-cccc"
        self.audit = {"vulnerabilities": {"a": {"via": [{"url": url, "title": "bad"}]}}}
        self.assert_fails("GHSA-aaaa-bbbb-cccc")
        self.settings = config(ignore=["GHSA-aaaa-bbbb-cccc"])
        self.assertEqual(self.check().returncode, 0, self.check().stderr)
        self.audit = {"vulnerabilities": {}}
        self.assert_fails("GHSA-aaaa-bbbb-cccc", "no longer")

    def test_a_deprecated_package_fails(self):
        self.locked["node_modules/a"]["deprecated"] = "use b instead"
        self.assert_fails("node_modules/a", "deprecated", "use b instead")

    def test_an_audit_that_could_not_run_exits_3(self):
        for audit in ({"error": {"code": "ENOTFOUND"}}, {}):
            with self.subTest(audit=audit):
                self.audit = audit
                self.assertEqual(self.check().returncode, 3)

    def test_a_registry_that_cannot_be_reached_exits_3(self):
        write(self.root / "package.json", json.dumps(self.manifest))
        self.check()
        run(["rm", "-rf", self.registry], ROOT)
        result = run(
            [
                "python3",
                SCRIPT,
                self.root,
                "--audit",
                self.root / "audit.json",
                "--registry",
                self.registry.as_uri(),
            ],
            ROOT,
        )
        self.assertEqual(result.returncode, 3, result.stderr)

    def test_registry_signatures_are_verified_online(self):
        self.stub_npm('echo "1 package has an invalid registry signature" >&2; exit 1')
        self.assert_fails("signature")

    def test_signatures_that_cannot_be_fetched_count_as_offline(self):
        self.stub_npm('echo "npm error code ENOTFOUND" >&2; exit 1')
        self.assertEqual(self.check().returncode, 3)

    def test_an_advisory_without_a_ghsa_id_fails(self):
        self.audit = {"vulnerabilities": {"a": {"via": [{"source": 1, "url": "https://example.com/x"}]}}}
        self.assert_fails("node_modules", "a", "no GHSA")

    def test_a_package_version_the_registry_does_not_have_fails(self):
        gone = urllib.error.HTTPError("https://registry.npmjs.org/a/1.2.3", 404, "Not Found", {}, None)
        with mock.patch("urllib.request.urlopen", side_effect=gone):
            note = check_npm.deprecation(check_npm.REGISTRY, "node_modules/a", registry_package())
        self.assertIn("not on the registry", note)

    def test_a_registry_error_other_than_not_found_counts_as_offline(self):
        down = urllib.error.HTTPError("https://registry.npmjs.org/a/1.2.3", 503, "Unavailable", {}, None)
        with mock.patch("urllib.request.urlopen", side_effect=down), self.assertRaises(urllib.error.URLError):
            check_npm.deprecation(check_npm.REGISTRY, "node_modules/a", registry_package())

    def test_offline_it_checks_everything_but_advisories_and_deprecations(self):
        self.locked["node_modules/a"]["deprecated"] = "use b instead"
        self.assertEqual(self.check(online=False).returncode, 0)
        self.locked["node_modules/a"]["license"] = "GPL-3.0-only"
        self.assertEqual(self.check(online=False).returncode, 1)


if __name__ == "__main__":
    unittest.main()
