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


if __name__ == "__main__":
    unittest.main()
