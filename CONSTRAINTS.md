# Constraints

Tetsu's quality bar, in the format the `constraint-driven-development` skill expects. The rules and
why they exist are in [ADR-0014](docs/adr/0014-checks-and-duties.md). Limits by risk are in
[ADR-0005](docs/adr/0005-testing-and-performance.md), which wins over the skill's default numbers.

Only the maintainer approves loosening anything here.

## Floor

These apply to every change. [scripts/floor_guard.py](scripts/floor_guard.py) catches the common
forms, and the [fresh review](docs/review.md) covers the rest, including secrets until
the scan in [#81](https://github.com/heavymedl-labs/tetsu/issues/81) lands. A `Floor-reason:` trailer on a commit
records the maintainer's approval for the files that commit touches.

- No new suppression comment without a written reason beside it, such as `#[allow]` or `# noqa`.
- No unfinished stubs or empty error handlers.
- No skipped or deleted test without a reason in the commit message.
- No secrets in source.
- No bypassing checks with `--no-verify`.
- `AGENTS.md` stays at or under 100 lines, because agents load it every session.
- This file is not weakened to make a change pass.

## Checks by language

A language lands only once its row is filled. "Gap" means no good tool exists, so the fresh review
covers that rule. The Rust row and its limits are in
[ADR-0015](docs/adr/0015-rust-checks.md) and [ADR-0016](docs/adr/0016-crate-layout-and-coverage.md), and the format and lint
column is in [ADR-0017](docs/adr/0017-format-and-lint.md). Format and lint run on every commit, and the rest
on every push.

| Language | Format and lint | Tests catch mistakes | Coverage | New duplication, outside tests | Complexity | Boundaries |
| --- | --- | --- | --- | --- | --- | --- |
| Rust | rustfmt, Clippy | `cargo-mutants` | `cargo-llvm-cov`, with branches on a pinned nightly ([ADR-0020](docs/adr/0020-branch-coverage.md)) | jscpd | Clippy, lizard | [#25](https://github.com/heavymedl-labs/tetsu/issues/25) |
| SQL | SQLFluff, and Squawk on migrations | Module tests against Postgres | E4 | E4 | E4 | [#25](https://github.com/heavymedl-labs/tetsu/issues/25) |
| Bash | shfmt, ShellCheck | `unittest`, from Python | E2 | E2 | E2 | E2 |
| Python | Ruff | `unittest` | E2 | E2 | Ruff | E2 |

Docs and workflows have no row, because tests and complexity do not apply. What runs on them today
is in [AGENTS.md](AGENTS.md#checks), and docs also get the
[docs audit](CONTRIBUTING.md#docs-audit).

## Measured, not yet enforced

| Metric | Today (2026-11-08) | Direction |
| --- | --- | --- |
| Region coverage per crate, which ADR-0005 says never goes down but ADR-0016's fixed limits let fall to the limit | kernel 100%, postgres 94.4%, tenancy 93.0%, tetsu 96.6% | Up |

## Exceptions

| ID | Rule | Path | Reason | Owner | Expires |
| --- | --- | --- | --- | --- | --- |
| E1 | Scripts need tests | `scripts/generate-brand.py` | One-off generator, and its output is checked by eye | Maintainer | 2027-02-04 |
| E2 | Bash and Python coverage, duplication, and boundaries, and Bash complexity | `scripts/` | Tooling only, with no numeric limit, and too small to justify more tools yet, so the fresh review covers these | Maintainer | 2027-02-04 |
| E4 | SQL coverage, duplication, and complexity | `crates/` | No good tool fits: module tests run every query against Postgres, and the fresh review covers the rest | Maintainer | 2027-02-04 |
