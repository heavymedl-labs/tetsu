#!/usr/bin/env bash
# Installs the pinned tools scripts/check.sh needs. Safe to run again.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo install --locked --version 0.9.1 cargo-llvm-cov
cargo install --locked --version 27.1.0 cargo-mutants
python3 -m venv .tools
.tools/bin/pip install --quiet --disable-pip-version-check lizard==1.25.0
