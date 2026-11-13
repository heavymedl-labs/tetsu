# .config

Settings only our own scripts read, each passed to its tool by path:

| File | Read by | Decision |
| --- | --- | --- |
| `deny.toml` | `cargo deny`, in `scripts/check.sh` | [ADR-0028](../docs/adr/0028-dependency-checks-revised.md) |
| `squawk.toml` | `squawk`, in `scripts/check.sh` | [ADR-0017](../docs/adr/0017-format-and-lint.md) |
| `npm-checks.toml` | `scripts/check_npm.py` | [ADR-0034](../docs/adr/0034-front-end-build.md) |
| `betterleaks.toml`, `betterleaks-ignore` | `scripts/scan-secrets.sh` | [ADR-0027](../docs/adr/0027-secrets-revised.md) |
| `nightly-toolchain` | `scripts/check.sh` and `scripts/install-tools.sh` | [ADR-0020](../docs/adr/0020-branch-coverage.md) |

Settings that editors also read, such as `ruff.toml`, `biome.json`, and `clippy.toml`, stay at the root, where
editors look. The floor guard protects these files as it did at the root.
