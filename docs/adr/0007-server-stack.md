# 0007. Server stack

- Status: Proposed
- Date: 2026-11-03

## Context

The `tetsu` binary needs an async runtime, an HTTP framework, a Postgres access
layer, migrations, and HTML templating before the walking skeleton can serve a
page ([ADR-0003](0003-architecture.md)). These choices are expensive to
reverse, so each was benchmarked against its competitors and a homegrown
baseline where feasible. The evidence is in
[Server stack research](../research/server-stack.md).

The stack has to:

- keep request handling fast under bursts, such as the 6:00 a.m. check-in rush
  at [Tallow Street](../reference-businesses/tallow-street-strength.md), and
  meet the benchmark gate in [ADR-0005](0005-testing-and-performance.md);
- let each module own its schema and migrations, and let
  [tenant isolation](https://github.com/heavymedl-labs/tetsu/issues/8) set
  per-transaction context such as Postgres row-level security settings;
- catch mistakes at compile time, because coding agents write much of the
  code;
- serve server-rendered public pages, such as booking pages for both
  reference businesses, escaped by default.

## Decision

- **Async runtime: Tokio 1.x.** io_uring runtimes did not beat it, and every
  other choice here builds on it. Domain code stays runtime-agnostic.
- **HTTP: Axum 0.8.** It measured no slower than raw hyper, and its middleware
  comes from Tower. Each module exposes its inbound adapters as an Axum
  `Router`, and the composition root merges them.
- **Postgres driver: `tokio-postgres` 0.7 with a `deadpool-postgres` 0.14
  pool.** It served 50% more requests than SQLx end to end. Statements are
  prepared once per connection and cached. A transaction pipelines statements
  that do not depend on each other's results, such as setting the tenant and
  running the query.
- **Query checking: Cornucopia 1.0.** Each module writes its queries in `.sql`
  files, and Cornucopia generates typed Rust by preparing them against a
  migrated database. CI regenerates the code and fails if it differs from what
  is committed. Generated code depends only on `tokio-postgres`. Before the
  walking skeleton relies on it, we fix its two statement-cache defects
  upstream, or in a fork if upstream declines.
- **Migrations: a small runner we own, on `tokio-postgres`.** Each module keeps
  numbered SQL files and a tracking table in its own schema. The runner takes a
  Postgres advisory lock, verifies checksums, runs each file in a transaction
  unless the file opts out, and is embedded in the `tetsu` binary. Squawk lints
  migration files in CI. Schema changes follow expand and contract, so old and
  new code can run against one schema. When migrations run is decided with the
  [deploy pipeline](https://github.com/heavymedl-labs/tetsu/issues/18).
- **HTML: Askama 0.16.** Templates are Jinja-style files compiled into Rust,
  type-checked against their context structs, and HTML-escaped by default.

Exact versions are pinned in `Cargo.lock`. Moving to a new major or 0.x minor
version needs no new ADR unless it changes a decision above.

## Alternatives considered

- **monoio, compio, or glommio.** Thread-per-core runtimes on io_uring. They
  matched or trailed Tokio on one core, monoio and glommio have not released
  since 2024, and they cut us off from the Tokio ecosystem.
- **actix-web.** Within noise of Axum, with fewer active contributors and its
  own middleware model instead of Tower.
- **Salvo.** Within noise of Axum and actively developed, but much smaller,
  and 1.1 raised its minimum Rust version a day after release.
- **Raw hyper with our own routing.** No faster than Axum, and we would own
  routing, extraction, and middleware.
- **SQLx.** Compile-time checks and a migrator in one crate, but the slowest
  driver measured, no pipelining, and thin maintenance: 781 open issues and a
  lead maintainer with 10 commits in the last year.
- **Diesel with diesel-async.** The healthiest project, but no faster than SQLx
  here, and its query DSL hides the SQL that row-level security and tuning
  need.
- **SeaORM.** Built on SQLx, so it inherits the driver cost, and it hides SQL.
- **`tokio-postgres` without generated code.** Fastest, but a typo in a query
  or a type mismatch surfaces only at runtime.
- **SQLx migrator or refinery.** The SQLx migrator needs SQLx connections.
  refinery has no advisory lock and cannot run `CREATE INDEX CONCURRENTLY`,
  which must run outside a transaction.
- **pgroll or Reshape.** Automate zero-downtime migrations, but run as
  separate tools with their own formats.
- **Sailfish.** About 1.4 µs faster per page, which is noise next to a
  database round trip, with a much smaller user base.
- **Maud.** Templates are Rust macros, harder for designers to edit, and it
  has not released since February 2025.
- **Homegrown templates.** Slower than every library, and escaping depends on
  remembering to call it.

## Consequences

Queries, request handling, and templates are checked before runtime. CI needs a
Postgres service to regenerate Cornucopia code. Dynamic queries, such as
filtered reports, cannot be generated; they use `tokio-postgres` directly and
need their own tests.

We own two pieces of infrastructure: the migration runner and any Cornucopia
fixes. The runner is small but touches every schema, so it gets tests for
locking, checksum drift, and non-transactional files. `deadpool-postgres` has
one main maintainer; it is small and could be replaced or vendored.

Tenant isolation ([#8](https://github.com/heavymedl-labs/tetsu/issues/8)),
identity ([#9](https://github.com/heavymedl-labs/tetsu/issues/9)), and the
public API ([#12](https://github.com/heavymedl-labs/tetsu/issues/12)) build on
Axum and `tokio-postgres`. The benchmarks here were on a laptop over loopback;
ADR-0005's benchmark suite repeats them on production-like hardware once the
walking skeleton runs.

## Sources

- [Server stack research](../research/server-stack.md): benchmarks, code
  reading, and project health, 2026-11-03.
- Tokio documentation, tokio.rs: runtime and scheduler.
- Axum 0.8 documentation, docs.rs/axum: routing, extractors, and Tower
  middleware.
- `tokio-postgres` documentation, docs.rs/tokio-postgres: pipelining.
- Cornucopia book, cornucopia-rs.github.io/cornucopia.
- Askama book, askama.rs: template syntax and automatic escaping.
- PostgreSQL documentation: explicit locking (advisory locks) and
  `CREATE INDEX CONCURRENTLY`.
- OWASP Cross Site Scripting Prevention Cheat Sheet: output encoding.
