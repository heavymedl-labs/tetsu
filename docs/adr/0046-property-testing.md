# 0046. Property testing

- Status: Proposed
- Date: 2026-11-11

## Context

[ADR-0005](0005-testing-and-performance.md) requires property-based tests for Tier 1 code, names `proptest` as
the leading option, and gives each tool its own ADR. Tenancy is the first Tier 1 module
([#63](https://github.com/heavymedl-labs/tetsu/issues/63)), and sign-in follows. A test-only dependency is cheap to
replace and on no hot path, so [ADR-0010](0010-evaluating-technology.md)'s steps 1, 4, and 7 apply.

**Criteria.** Generate structured values, such as slugs, names with any Unicode, and pairs of tenants, from
composable strategies. Shrink a failure to a small case. Replay failures in later runs. Run on stable Rust inside
`cargo test`. Keep a permissive license and an active project.

## Decision

Tenancy and every later Tier 1 module write property tests with **`proptest`**, as a dev-dependency only.

- **Failures persist** in `proptest-regressions/` files, committed, so a found case runs on every later test.
- **The default of 256 cases** runs on each check, and CI can raise it through `PROPTEST_CASES`.
- **Properties state invariants from the domain,** such as "every accepted name shows a visible character" and
  "no address resolves to another tenant's data", rather than repeating an example test.

## Alternatives considered

Read on crates.io on 2026-11-11.

| Crate | Latest | Downloads, 90 days | Fit |
| --- | --- | --- | --- |
| `proptest` 1.12.0 | 2026-10-10 | 74 million | Composable strategies, integrated shrinking, persisted failures; MIT or Apache-2.0; needs Rust 1.88 |
| `quickcheck` 1.1.0 | 2026-02-10 | 16 million | Type-driven generation, so structured values need hand-written types, and shrinking is per type |
| `bolero` 0.13.7 | 2026-10-03 | 1.5 million | Joins property tests and fuzzing, but fuzzing engines need nightly or extra tools, and it is far less used |

`proptest` is the common default and the option ADR-0005 named, and nothing in the criteria argues against it.

## Consequences

- `proptest` joins the workspace as a dev-dependency, and the advisory checks cover it.
- #63 can proceed: Tenancy's property tests are written with it.
- Property tests add seconds to each run. If they grow slow, CI keeps the default and a nightly run raises the
  case count.
- Replacing it later touches only tests.

## Sources

- [proptest on crates.io](https://crates.io/crates/proptest) and the
  [Proptest book](https://proptest-rs.github.io/proptest/), read 2026-11-11.
- [quickcheck on crates.io](https://crates.io/crates/quickcheck) and [bolero on crates.io](https://crates.io/crates/bolero),
  read 2026-11-11.
- John Hughes, "QuickCheck testing for fun and profit," PADL 2007.
