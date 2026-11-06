#!/usr/bin/env bash
# Installs the pinned tools scripts/check.sh needs. Safe to run again.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo install --locked --version 0.9.1 cargo-llvm-cov
cargo install --locked --version 27.1.0 cargo-mutants
# uv provides the tools' own Python, so it does not depend on the machine's (ADR-0017).
if ! command -v uv >/dev/null; then
  echo "error: uv is missing. Install it from https://docs.astral.sh/uv/" >&2
  exit 1
fi
if ! .tools/bin/python -c 'import sys; sys.exit(sys.version_info[:2] != (3, 13))' 2>/dev/null; then
  uv venv --clear --python 3.13 .tools
fi
uv pip install --quiet --python .tools lizard==1.25.0 ruff==0.17.4 squawk-cli==2.71.0 \
  shellcheck-py==0.11.0.1 shfmt-py==4.2.0 sqlfluff==4.5.0
