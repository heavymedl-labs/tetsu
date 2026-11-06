# 0015. Rust checks

- Status: Accepted
- Date: 2026-11-06

## Context

Opening the implementation phase ([#30](https://github.com/heavymedl-labs/tetsu/issues/30)) needs the Rust
row in [CONSTRAINTS.md](../../CONSTRAINTS.md) filled, as [ADR-0014](0014-checks-and-duties.md) requires.
[ADR-0005](0005-testing-and-performance.md) asks for branch coverage and names leading tools, and
[ADR-0007](0007-server-stack.md) asks for lints. The evidence is in
[Rust checks research](../research/rust-checks.md).

## Decision

- **Tools.** Clippy and rustfmt from the pinned toolchain, `cargo-llvm-cov`, `cargo-mutants`, jscpd, and lizard.
  [scripts/install-tools.sh](../../scripts/install-tools.sh) and [scripts/check.sh](../../scripts/check.sh) pin the rest.
- **Starting limits.** These hold from the first line and tighten as real code gives us results.
  - Coverage: at least 90% of lines and regions. A stricter Tier 1 limit comes with the first Tier 1 code.
  - Mutation: no surviving mutant in changed code, in any tier. An equivalent mutant goes in
    `exclude_re` in `.cargo/mutants.toml`, which the floor guard flags, with a `Floor-reason:`. This is stricter than ADR-0014's plan to tune Tier 2 and 3 limits, because about 30 lines of
    code gave us nothing to tune against.
  - Complexity: cyclomatic complexity 10, 50 lines, and 5 parameters per function.
  - Duplication: no clones outside tests.
- **Lints.** Clippy's pedantic group, with warnings as failures. `unsafe` is forbidden in `crates/` for now,
  and `unwrap`, `expect`, `panic`, `todo`, and `unimplemented` are denied outside tests. Clippy rejects
  `axum::Json` and `Row::get`, as ADR-0007 and [ADR-0011](0011-server-stack-guardrails.md) require.
- **Branch coverage is an exception for now.** It needs nightly Rust, so stable measures lines and regions
  instead. Exception E3 records this, and it must be settled before the first Tier 1 code.
- **The image** is built from `rust:1.99.1-slim-bookworm` onto a distroless `cc` base pinned by digest, and CI
  builds and runs it. Pushing it comes with the [first deploy](https://github.com/heavymedl-labs/tetsu/issues/19).
- **The benchmark harness** joins the workspace, as ADR-0008 planned. Cargo skips it unless asked, so product
  checks stay fast, and CI still confirms it compiles.

## Alternatives considered

- **Nightly Rust for coverage only.** Meets ADR-0005 today, but adds a second toolchain before any code needs
  it. This is the leading way to settle E3.
- **Clippy's complexity lint alone.** One less tool, but the lint is disputed and does not measure cyclomatic
  complexity, which the maintainer asked for.
- **A plain Debian base image.** Easier to debug, but larger and with more in it to patch.
- **Keeping the harness outside the workspace.** Avoids sharing a lockfile with its many dependencies, but
  ADR-0008 planned the move, and two lockfiles would drift.

## Consequences

Contributors run `scripts/install-tools.sh` once, which takes a few minutes. Each push runs mutation testing on
changed Rust, which will slow as code grows and may need to move to CI only. The workspace lockfile now carries
the harness's dependencies, so dependency updates and #31's advisory checks see them too. Formatting the harness
to join the workspace changed about 330 lines there, with no change in behavior.

**Tradeoffs.** Each tool leans on one maintainer. Line and region coverage can pass where branch coverage would not.
A strict no-survivors rule may produce noise as equivalent mutants appear.

**Weakest evidence.** Everything ran on about 30 lines of code, the limits are not tuned, and community practice
was not reviewed.

**Cost to replace.** Low. Each tool is a line or two across `scripts/check.sh` and the install script.

**Revisit** when the first Tier 1 code lands (E3 and the Tier 1 limit), when mutation testing makes a push slow,
or if a limit produces more noise than findings.

## Sources

- [Rust checks research](../research/rust-checks.md): criteria, project health, and what we ran, 2026-11-06.
- [cargo-llvm-cov README](https://github.com/taiki-e/cargo-llvm-cov): branch coverage needs nightly.
- [cargo-mutants `--in-diff`](https://mutants.rs/in-diff.html).
- [Clippy lint configuration](https://doc.rust-lang.org/clippy/lint_configuration.html).
