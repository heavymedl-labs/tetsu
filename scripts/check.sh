#!/usr/bin/env bash
# Runs every local check. The pre-push hook and CI both call this.
# Takes --base <ref>, the commit to compare with. The default is origin/main.
set -euo pipefail
cd "$(dirname "$0")/.."

wanted="$(cat .nvmrc)"
right_node() { [[ "$(node --version 2>/dev/null)" == "v$wanted."* ]]; }
nvm_script="${NVM_DIR:-$HOME/.nvm}/nvm.sh"

if ! right_node && [[ -s "$nvm_script" ]]; then
  # nvm's own script does not run under these shell options.
  set +eu
  . "$nvm_script"
  nvm use "$wanted" >/dev/null
  set -eu
fi
if ! right_node; then
  echo "error: Node $wanted is required. Install it, for example: nvm install $wanted" >&2
  exit 1
fi

for tool in cargo-llvm-cov cargo-mutants .tools/bin/lizard; do
  if ! command -v "$tool" >/dev/null; then
    echo "error: $tool is missing. Run scripts/install-tools.sh" >&2
    exit 1
  fi
done

python3 scripts/floor_guard.py "$@"
python3 -m unittest discover -b -s scripts/tests -t .
scripts/check-adrs.sh
python3 scripts/check_doc_sizes.py
npx --yes markdownlint-cli2@0.24.0 "**/*.md"

# Limits are in ADR-0015 and ADR-0016. Cargo skips the benchmark harness, except for formatting.
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cargo llvm-cov --json --summary-only --output-path "$work/coverage.json"
cargo metadata --no-deps --format-version 1 >"$work/metadata.json"
python3 scripts/check_coverage.py "$work/metadata.json" "$work/coverage.json"
.tools/bin/lizard -l rust -C 10 -L 50 -a 5 -w crates
npx --yes jscpd@5.4.2 crates --threshold 0 --ignore "**/tests/**" --reporters console

base="origin/main"
if [[ "${1:-}" == "--base" ]]; then
  base="$2"
fi
git diff "$(git merge-base "$base" HEAD)" -- crates >"$work/rust.diff"
if [[ -s "$work/rust.diff" ]]; then
  cargo mutants --in-diff "$work/rust.diff"
fi
