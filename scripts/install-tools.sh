#!/usr/bin/env bash
# Installs the pinned tools scripts/check.sh needs. Safe to run again.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo install --locked --version 0.9.1 cargo-llvm-cov
cargo install --locked --version 27.1.0 cargo-mutants
cargo install --locked --version 0.20.2 cargo-deny
# Branch coverage needs a nightly compiler (ADR-0020). rustup makes the first toolchain it installs the default
# when none is set, which would change what this machine builds with everywhere, so that is undone.
had_default=true
rustup default >/dev/null 2>&1 || had_default=false
rustup toolchain install "$(<.config/nightly-toolchain)" --profile minimal --component llvm-tools-preview
[[ "$had_default" == true ]] || rustup default none
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
# The secret scanner (ADR-0027) has no package, so its release archive is checked against the SHA-256 the
# release published, read 2026-11-08, before it is installed. It goes in after the venv, which --clear empties.
betterleaks=1.10.0
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) asset=linux_x64 sum=9d9ad503cd5d80d18a939352266c3fcca52f10da9b83cd5cfe646989b13940fe ;;
  Linux-aarch64 | Linux-arm64) asset=linux_arm64 sum=3d7585cbe4f947aef6e31662f65beb5b43b104381a691b6b81b157ebfbb0c02d ;;
  Darwin-arm64) asset=darwin_arm64 sum=91eaea97709d3308c7712574a56260539c65fbd0672711537aa68d566ea630b3 ;;
  Darwin-x86_64) asset=darwin_x64 sum=71b3f30602856e622008c248b1b47b2ba15b227f4a033e8bfdfa05a21ec7d531 ;;
  *)
    echo "error: no Betterleaks release for $(uname -s) $(uname -m)" >&2
    exit 1
    ;;
esac
if [[ "$(.tools/bin/betterleaks version 2>/dev/null)" != "$betterleaks" ]]; then
  download="$(mktemp -d)"
  archive="$download/betterleaks.tar.gz"
  trap 'rm -rf "$download"' EXIT
  curl -fsSL -o "$archive" \
    "https://github.com/betterleaks/betterleaks/releases/download/v$betterleaks/betterleaks_${betterleaks}_$asset.tar.gz"
  if [[ "$(sha256sum "$archive" 2>/dev/null || shasum -a 256 "$archive")" != "$sum "* ]]; then
    rm -rf "$download"
    echo "error: the Betterleaks download does not match its published SHA-256" >&2
    exit 1
  fi
  tar -xzf "$archive" -C "$download" betterleaks
  install -m 755 "$download/betterleaks" .tools/bin/betterleaks
  rm -rf "$download"
  trap - EXIT
fi
# Mailpit, a local mail server the SMTP adapter's tests send to (ADR-0074), is checked against the SHA-256 its
# release published, read 2026-11-18, before it is installed. It is never deployed.
mailpit=1.34.0
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) asset=linux-amd64 sum=af236b42f171b1c74f0ac5ff444f44d36f2f2ecc0134f5a2f1d4e307e56a0d2c ;;
  Linux-aarch64 | Linux-arm64) asset=linux-arm64 sum=179f3fe9642edb3a123edc613003e45641869c5b2728c2076c7bef1c3e09dd01 ;;
  Darwin-arm64) asset=darwin-arm64 sum=32933e0bd17e9738f19ab2eb5ec4d50e183448c9f29ba97e762d418a0f72e854 ;;
  Darwin-x86_64) asset=darwin-amd64 sum=a497582d4e9f9a5d0eea84f8622530533dce0797cf8a4952bd7a59487e5d9c7c ;;
  *)
    echo "error: no Mailpit release for $(uname -s) $(uname -m)" >&2
    exit 1
    ;;
esac
if [[ "$(.tools/bin/mailpit version 2>/dev/null)" != *"v$mailpit "* ]]; then
  download="$(mktemp -d)"
  archive="$download/mailpit.tar.gz"
  trap 'rm -rf "$download"' EXIT
  curl -fsSL -o "$archive" "https://github.com/axllent/mailpit/releases/download/v$mailpit/mailpit-$asset.tar.gz"
  if [[ "$(sha256sum "$archive" 2>/dev/null || shasum -a 256 "$archive")" != "$sum "* ]]; then
    rm -rf "$download"
    echo "error: the Mailpit download does not match its published SHA-256" >&2
    exit 1
  fi
  tar -xzf "$archive" -C "$download" mailpit
  install -m 755 "$download/mailpit" .tools/bin/mailpit
  rm -rf "$download"
  trap - EXIT
fi
