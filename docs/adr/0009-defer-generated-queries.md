# 0009. Defer generated queries

- Status: Accepted
- Date: 2026-11-03

## Context

[ADR-0007](0007-server-stack.md) chose Cornucopia to check queries at build
time, once its statement cache defects are fixed. That fix needs a generator
change and a choice between upstream and a fork
([#28](https://github.com/heavymedl-labs/tetsu/issues/28)), and CI needs code
generation set up for each module. The walking skeleton has few queries, so
those checks would pay off little before milestone 2.

## Decision

Use `tokio-postgres` directly through the walking skeleton. This replaces only
ADR-0007's query checking decision; the rest of that ADR stands.

- **SQL lives in `.sql` files.** Each module keeps its queries in its own files
  and loads them with `include_str!`.
- **Statements are cached.** Outbound adapters run queries through a small
  helper that calls `prepare_cached`.
- **CI prepares every query.** It prepares each one against a migrated database
  and fails on an error, which catches invalid SQL and missing tables or
  columns.
- **Tests cover row mapping.** Mapping is written by hand, and integration tests
  run it against Postgres at the rigor tier of the code
  ([ADR-0005](0005-testing-and-performance.md)).
- **We revisit before milestone 2 starts.** A new ADR adopts Cornucopia or keeps
  this approach, based on the query count, the mapping bugs we have seen, and
  the state of #28.

## Alternatives considered

- **Fix Cornucopia now.** Keeps build-time checks from the first query, but puts
  generator work, a fork decision, and code generation setup ahead of a skeleton
  with few queries.
- **Unfixed Cornucopia.** Correct results and no generator work, but about half
  the throughput of `tokio-postgres` in a local test. We would also be measuring
  the skeleton against the wrong baseline.
- **SQLx.** Build-time checks without generator work, but ADR-0007 rejected it on
  speed and maintenance.
- **SQL strings in Rust, with no CI check.** The least setup, but mistakes surface
  only at runtime, and a later move to generated code costs more.

## Consequences

The skeleton has fewer moving parts and runs at the speed of the driver. We give
up build-time checks: preparing a query does not compare its columns or
parameters with our Rust types, so those mistakes surface only in tests. This
matters most for sign-in and tenant code, the first queries written and among
the highest rigor. Adopting Cornucopia later costs more with each query, so the
revisit point stays fixed. Plain SQL files limit that cost.

Issue #28 moves out of milestone 1 and becomes an input to the revisit. CI still
needs a Postgres service, for the prepare check.

## Sources

- [Server stack research](../research/server-stack.md): driver and Cornucopia
  benchmarks.
- `tokio-postgres` documentation, docs.rs/tokio-postgres: statements and
  preparation.
- `deadpool-postgres` documentation, docs.rs/deadpool-postgres: statement cache.
- Michael Nygard, "Documenting Architecture Decisions," Cognitect, 2011:
  superseding a reversed decision.
