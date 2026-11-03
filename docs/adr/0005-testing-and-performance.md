# 0005. Testing and performance

- Status: Proposed
- Date: 2026-11-02

## Context

Tetsu handles money, identity, and many tenants' data, and coding agents write
much of its code. Tests are the main guard against regressions, and a booking
platform under load is judged on speed. We want testing to be as rigorous as
the testing pyramid allows, and performance to improve over time.

## Decision

- **Testing pyramid.** Every module has many unit tests, module tests against
  its use cases and its own schema, and fewer end-to-end tests across the
  composed system.
- **Coverage.** CI measures line and branch coverage on every pull request.
  Coverage never goes down.
- **Rigor by risk.**

  | Tier | Areas | Bar |
  | --- | --- | --- |
  | 1 | Identity and access, money, tenant isolation | 100% branch coverage; mutation testing with no surviving mutants; property-based tests for invariants such as a balanced ledger; negative tests at every authorization and isolation boundary; maintainer review plus an independent agent review in a fresh context; no `unsafe`. |
  | 2 | Booking, scheduling, and any module not listed | At least 90% branch coverage; tests for domain rules, including concurrency and capacity. |
  | 3 | UI | Coverage never goes down; end-to-end tests for critical journeys. |

  Code that moves or records money, including pricing, orders, and payments, is
  Tier 1.
- **Performance.** Each module lists its hot paths, such as request handling,
  availability and booking under contention, checkout, and database queries.
  Hot paths have micro-benchmarks, and end-to-end flows have load tests with
  latency budgets.
- **Benchmarks.** Benchmarks run on every pull request that touches a hot path,
  and nightly. Results are stored so trends are visible. A regression of more
  than 5% fails the build; an improvement becomes the new baseline.
- **Tools.** Each tool gets its own ADR. Leading options: `cargo-llvm-cov` for
  coverage, `cargo-mutants` for mutation testing, `proptest` for property
  tests, and `criterion` for benchmarks.

## Alternatives considered

- **One coverage target for all code.** Spends effort on low-risk code and says
  nothing about whether the dangerous paths are tested.
- **Coverage without mutation testing.** Coverage shows code ran, not that a
  test would catch a wrong result.
- **Performance testing before release only.** Regressions pile up unseen and
  are expensive to trace back.

## Consequences

Tier 1 work is slower by design. CI takes longer, so slow suites such as
mutation tests and load tests may run only on affected modules. Benchmark
noise must be kept under 5% for the gate to be fair, which means stable CI
runners or repeated runs.

## Sources

- Mike Cohn, *Succeeding with Agile*, Addison-Wesley, 2009: the test pyramid.
- Martin Fowler, "TestPyramid," martinfowler.com, 2012.
- Yue Jia and Mark Harman, "An Analysis and Survey of the Development of
  Mutation Testing," *IEEE Transactions on Software Engineering*, 2011.
- Brendan Gregg, *Systems Performance*, 2nd ed., Addison-Wesley, 2020.
- OWASP Application Security Verification Standard 5.0: verification levels.
