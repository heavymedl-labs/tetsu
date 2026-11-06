#!/usr/bin/env bash
# Runs every local check. The pre-push hook and CI both call this.
# Arguments go to the floor guard, such as --base <ref>.
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

python3 scripts/floor_guard.py "$@"
python3 -m unittest discover -b -s scripts/tests -t .
scripts/check-adrs.sh
python3 scripts/check_doc_sizes.py
npx --yes markdownlint-cli2@0.24.0 "**/*.md"
