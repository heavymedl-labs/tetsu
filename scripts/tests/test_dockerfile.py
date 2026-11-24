"""The Dockerfile builds every crate: it copies each manifest for the dependency layer, and cleans each."""

import re
import unittest
from pathlib import Path

import tomllib

ROOT = Path(__file__).resolve().parent.parent.parent


class Dockerfile(unittest.TestCase):
    def test_every_crate_is_copied_and_cleaned(self):
        dockerfile = (ROOT / "Dockerfile").read_text()
        cleaned = re.search(r"cargo clean --release((?:\s+\\?\s*-p \S+)+)", dockerfile)
        self.assertIsNotNone(cleaned, "the Dockerfile cleans Tetsu's crates")
        packages = set(re.findall(r"-p (\S+)", cleaned.group(1)))
        for manifest in sorted(ROOT.glob("crates/*/Cargo.toml")):
            crate = manifest.parent.name
            name = tomllib.loads(manifest.read_text())["package"]["name"]
            self.assertIn(f"COPY crates/{crate}/Cargo.toml crates/{crate}/\n", dockerfile, crate)
            self.assertIn(name, packages, crate)

    def test_every_workspace_member_is_copied(self):
        """Cargo loads every member to build any one, so the dependency layer needs each manifest."""
        dockerfile = (ROOT / "Dockerfile").read_text()
        members = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["members"]
        manifests = [manifest for member in members for manifest in sorted(ROOT.glob(f"{member}/Cargo.toml"))]
        self.assertGreater(len(manifests), len(list(ROOT.glob("crates/*/Cargo.toml"))))
        for manifest in manifests:
            folder = manifest.parent.relative_to(ROOT).as_posix()
            self.assertIn(f"COPY {folder}/Cargo.toml {folder}/\n", dockerfile, folder)


if __name__ == "__main__":
    unittest.main()
