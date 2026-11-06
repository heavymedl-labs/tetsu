# Constraints

Tetsu's quality bar, in the format the `constraint-driven-development` skill expects. The rules and
why they exist are in [ADR-0014](docs/adr/0014-checks-and-duties.md). Limits by risk are in
[ADR-0005](docs/adr/0005-testing-and-performance.md), which wins over the skill's default numbers.

Only the maintainer approves loosening anything here.

## Floor

These apply to every change now. No tool enforces them yet, so the
[fresh review](docs/review.md) does, until
[#33](https://github.com/heavymedl-labs/tetsu/issues/33) adapts the skill's floor guard.

- No new suppression comment without a written reason beside it, such as `#[allow]` or `# noqa`.
- No unfinished stubs or empty error handlers.
- No skipped or deleted test without a reason in the commit message.
- No secrets in source.
- No bypassing checks with `--no-verify`.
- This file is not weakened to make a change pass.

## Checks by language

A language lands only once its row is filled. "Gap" means no good tool exists, so the fresh review
covers that rule. **Nothing in this table runs yet.** Tools named are candidates that have not been
evaluated, and each needs an ADR under [ADR-0010](docs/adr/0010-evaluating-technology.md).

| Language | Tests catch mistakes | Coverage | New duplication, outside tests | Complexity | Boundaries |
| --- | --- | --- | --- | --- | --- |
| Rust | `cargo-mutants` | `cargo-llvm-cov` | jscpd | Clippy, lizard | [#25](https://github.com/heavymedl-labs/tetsu/issues/25) |
| SQL | To choose | To choose | To choose | To choose | [#25](https://github.com/heavymedl-labs/tetsu/issues/25) |
| Bash | To choose | To choose | To choose | To choose | To choose |
| Python | To choose | To choose | To choose | To choose | To choose |

Docs and workflows have no row, because tests and complexity do not apply. What runs on them today
is in [AGENTS.md](AGENTS.md#checks), and docs also get the
[docs audit](CONTRIBUTING.md#docs-audit).

## Measured, not yet enforced

Nothing yet. Add a metric here with today's value and its direction once code exists.

## Exceptions

| ID | Rule | Path | Reason | Owner | Expires |
| --- | --- | --- | --- | --- | --- |
