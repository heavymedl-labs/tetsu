# 0020. Branch coverage

- Status: Accepted
- Date: 2026-11-08

## Context

[ADR-0005](0005-testing-and-performance.md) asks for 100% branch coverage at Tier 1 and 90% at Tier 2.
`cargo-llvm-cov` measures branches only on nightly Rust, so [ADR-0015](0015-rust-checks.md) and
[ADR-0016](0016-crate-layout-and-coverage.md) left branch coverage as exception E3, to be settled before the
first Tier 1 code ([#51](https://github.com/heavymedl-labs/tetsu/issues/51)). Tenancy is the first: `resolve`
decides which tenant a request reaches, and ADR-0005 makes tenant isolation Tier 1. Its move to Tier 1 brings
the rest of that bar, such as property-based tests, so it is its own step,
[#63](https://github.com/heavymedl-labs/tetsu/issues/63).

Branch coverage is still a nightly-only compiler feature in November 2026, with no plan to stabilize it in Rust's
tracking issue. Unstable coverage features can also be removed: MC/DC coverage left nightly in 2025.

Regions are not branches: an `if` with no `else` whose condition is always true covers every region and never
takes the other branch. Mutation testing catches some of these, not all.

## Decision

- **A pinned nightly measures branches, and only branches.** `scripts/check.sh` keeps running tests, lines, and
  regions on the pinned stable toolchain, then runs `cargo llvm-cov --branch` again on a nightly pinned by date
  in the script, with `llvm-tools-preview`, in its own target folder. The stable run stays the one that proves
  the code we ship.
- **Branch limits match ADR-0016's line and region limits for each crate:** 100% at Tier 1, as ADR-0005 asks,
  and 90% at Tiers 2 and 3. A branch no test can reach is restructured away. Failing that, a CONSTRAINTS.md
  exception lowers that crate's branch limit to its measured value, which only the maintainer approves and the
  floor guard flags, and `scripts/check_coverage.py` reads it.
- **The branch check fails on an empty report.** A crate with no branches counts as covered, so a run that
  reports no branches at all, such as a dropped flag or a changed report format, fails rather than passing.
- **The nightly date moves** with each stable toolchain update. A test that passes on one toolchain and fails on
  the other is fixed to pass on both, starting with the one that fails on nightly today.
- **If branch coverage leaves nightly,** the check falls back to regions plus mutation testing, the first
  alternative below, through a superseding ADR. The pinned date means a removal cannot break the check until
  the date moves.
- **E3 leaves CONSTRAINTS.md** when the branch check runs, and the E3 lines in ADR-0015 and ADR-0016 are
  marked superseded by this ADR.

## Alternatives considered

- **Treat 100% regions plus mutation testing as the branch rule,** and amend ADR-0005. No second toolchain, but
  it misses the untaken branch of an `if` with no `else`, which is the case branch coverage exists for.
- **Run all coverage, and so all tests, on nightly.** One run instead of two, but the code we ship would no
  longer be tested on the toolchain that builds it.
- **Keep E3 until branch coverage reaches stable.** No cost now, but nothing measures branches meanwhile, and
  the first Tier 1 code, Tenancy, cannot meet ADR-0005 until it does.
- **Move Tenancy to Tier 1 in the same change.** One step instead of two, but it would commit to property-based
  tests and 100% coverage, each with its own cost, without weighing them.

## Consequences

- Each check runs the tests twice, adding about the time of one test run, a few seconds today.
- Contributors and CI install a second toolchain. Installing it where rustup has no default makes it the
  default, so `scripts/install-tools.sh` installs it without changing that. CI's tool cache key must include the
  nightly date and cache the toolchain, or a new date would not be fetched.
- Nightly can change the standard library under the tests. On 2026-11-08, `escape_debug` on nightly escaped a
  lone combining mark that 1.99.1 prints, which changes how `crates/tetsu/src/seed.rs` escapes output.
- Every crate clears the 90% branch limit today: postgres 94.2%, tenancy 93.5%, and tetsu 100%, while the kernel
  has no branches. Tenancy needs more for Tier 1, which #63 tracks.
- Replacing the choice is cheap: one script step and one cache entry. When branch coverage reaches stable, the
  nightly step goes and the stable run adds `--branch`.

**Weakest evidence.** One run on about 2,500 lines of source. Branches are counted per generic instantiation,
read by eye and not compared with another tool, so some may prove unreachable for a given type.

**Revisit** when branch coverage reaches stable Rust, if it leaves nightly, or if nightly breaks the check more
than once in a quarter.

## Sources

- [cargo-llvm-cov README](https://github.com/taiki-e/cargo-llvm-cov): `--branch` is unstable and needs nightly.
- [Rust research](../research/rust-checks.md#branch-coverage-on-nightly-2026-11-08), measured 2026-11-08.
- [rust-lang/rust#79649](https://github.com/rust-lang/rust/issues/79649), the tracking issue for branch
  coverage, still open on 2026-11-08 with no stabilization plan.
- [rust-lang/rust#144999](https://github.com/rust-lang/rust/pull/144999), which removed MC/DC coverage from
  the compiler in 2025.
