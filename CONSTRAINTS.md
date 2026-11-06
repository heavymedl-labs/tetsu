# Constraints

Tetsu's quality bar, in the format the `constraint-driven-development` skill expects. The rules and
why they exist are in [ADR-0014](docs/adr/0014-checks-and-duties.md). Limits by risk are in
[ADR-0005](docs/adr/0005-testing-and-performance.md), which wins over the skill's default numbers.

Only the maintainer approves loosening anything here.

## Floor

These apply to every change. [scripts/floor_guard.py](scripts/floor_guard.py) catches the common
forms, and the [fresh review](docs/review.md) covers the rest, including secrets until
[#31](https://github.com/heavymedl-labs/tetsu/issues/31). A `Floor-reason:` trailer on a commit
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
covers that rule. **Only the Bash and Python tests run today.** Tools named for Rust and SQL are
candidates that have not been evaluated, and each needs an ADR under
[ADR-0010](docs/adr/0010-evaluating-technology.md).

| Language | Tests catch mistakes | Coverage | New duplication, outside tests | Complexity | Boundaries |
| --- | --- | --- | --- | --- | --- |
| Rust | `cargo-mutants` | `cargo-llvm-cov` | jscpd | Clippy, lizard | [#25](https://github.com/heavymedl-labs/tetsu/issues/25) |
| SQL | To choose | To choose | To choose | To choose | [#25](https://github.com/heavymedl-labs/tetsu/issues/25) |
| Bash | `unittest`, from Python | E2 | E2 | E2 | E2 |
| Python | `unittest` | E2 | E2 | E2 | E2 |

Docs and workflows have no row, because tests and complexity do not apply. What runs on them today
is in [AGENTS.md](AGENTS.md#checks), and docs also get the
[docs audit](CONTRIBUTING.md#docs-audit).

## Measured, not yet enforced

Nothing yet. Add a metric here with today's value and its direction once code exists.

## Exceptions

| ID | Rule | Path | Reason | Owner | Expires |
| --- | --- | --- | --- | --- | --- |
| E1 | Scripts need tests | `scripts/generate-brand.py` | One-off generator, and its output is checked by eye | Maintainer | 2027-02-04 |
| E2 | Bash and Python checks beyond tests | `scripts/` | Tooling only, with no numeric limit, and too small to justify more tools yet, so the fresh review covers these | Maintainer | 2027-02-04 |
