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

- **Async runtime: Tokio 1.x.** io_uring runtimes used more CPU per request and
  served fewer requests, and every other choice here builds on Tokio. Domain
  code stays runtime-agnostic.
- **HTTP: Axum 0.8.** It uses 11 to 13% more server CPU per request than raw
  hyper on trivial handlers, about 1 to 3 µs, and that buys routing, extractors,
  and Tower middleware. A list transaction alone costs about 120 µs of client
  CPU. Each module exposes its inbound adapters as an Axum `Router`, and the
  composition root merges them.
- **JSON responses: our own response type on `serde_json::to_vec`.**
  `axum::Json` uses about 30% more server CPU per JSON request than this small
  type with identical output. A lint rejects `axum::Json` once code exists.
- **Postgres driver: `tokio-postgres` 0.7 with a `deadpool-postgres` 0.14
  pool.** It served 27% more requests than SQLx end to end. Statements are
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

- **Technology stays at the edges.** Axum, `tokio-postgres`, Cornucopia, and
  Askama types appear only in adapters and the composition root, never in the
  domain model, use cases, or driven ports. Handlers parse the request, call a
  use case, and shape the response. We do not wrap these crates in abstractions
  of our own: replacing one means rewriting the adapters that use it. Adapters
  stay in their module's crate, so the compiler cannot enforce this rule; lints
  and review do, with the checks chosen when the walking skeleton is built.

Exact versions are pinned in `Cargo.lock`. Moving to a new major or 0.x minor
version needs no new ADR unless it changes a decision above.

## Alternatives considered

- **A separate crate for each module's HTTP adapters.** The compiler would keep
  Axum out of the domain, but the number of crates would about double, and
  [ADR-0003](0003-architecture.md) makes each module one crate.
- **monoio or compio.** Thread-per-core runtimes on io_uring. On one core they
  used about 12% more CPU per request than Tokio and served 7% fewer requests.
  monoio has not released since 2024, and both cut us off from the Tokio
  ecosystem. glommio, not benchmarked, has the same drawbacks.
- **actix-web.** The closest to raw hyper, using 7 to 8% less CPU than Axum with
  our JSON type, about 1 to 2 µs per request. That does not outweigh fewer
  active contributors and its own middleware model instead of Tower.
- **Salvo.** Uses 21% more CPU than raw hyper on plain text and nearly twice as
  much with its built-in JSON response. It is also much smaller, and 1.1 raised
  its minimum Rust version a day after release.
- **Raw hyper with our own routing.** The cheapest measured, but we would own
  routing, extraction, and middleware to save a few microseconds. Its benchmark
  only matches paths, so a real router would give back part of that saving.
- **SQLx.** Compile-time checks and a migrator in one crate, but slower than
  `tokio-postgres` on every workload, no pipelining, and thin maintenance: 781
  open issues and a lead maintainer with 10 commits in the last year.
- **Diesel with diesel-async.** The healthiest project, but slower than SQLx
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
- **Homegrown templates.** Slower than Askama and Sailfish, and escaping depends
  on remembering to call it.

## Consequences

Queries, request handling, and templates are checked before runtime. CI needs a
Postgres service to regenerate Cornucopia code. Dynamic queries, such as
filtered reports, cannot be generated; they use `tokio-postgres` directly and
need their own tests.

We own two pieces of infrastructure: the migration runner and any Cornucopia
fixes. The runner is small but touches every schema, so it gets tests for
locking, checksum drift, and non-transactional files. `deadpool-postgres` has
one main maintainer; it is small and could be replaced or vendored.

Replacing a choice costs different amounts at each layer. These are estimates;
no swap has been tried.

| Layer | What a swap changes | What keeps it cheap |
| --- | --- | --- |
| Domain and use cases | Nothing | No framework or driver types |
| HTTP | Inbound adapters and custom middleware | Thin handlers, request and response types separate from domain types, Tower layers before custom middleware, tests over real HTTP |
| Templates | Template files | Templates take plain view structs and hold no business logic |
| Query checking | Generated code and outbound adapters | SQL stays in `.sql` files, and ports return domain types |
| Postgres driver | Outbound adapters and the pool | Ports name no driver types, including for transactions and tenant context, which is the hardest part |
| Migrations | The runner | Migrations stay plain SQL files |
| Tokio | Nearly everything; not planned | Spawning and timers stay out of domain code |
| PostgreSQL | Schemas, queries, and row-level security; not planned | Nothing: we use its features directly |

Tenant isolation ([#8](https://github.com/heavymedl-labs/tetsu/issues/8)),
identity ([#9](https://github.com/heavymedl-labs/tetsu/issues/9)), and the
public API ([#12](https://github.com/heavymedl-labs/tetsu/issues/12)) build on
Axum and `tokio-postgres`. The benchmarks here ran over loopback on a laptop and
2-CPU CI runners ([ADR-0008](0008-benchmark-harness.md)), so they show neither
real network latency nor many-core behavior. ADR-0005's benchmark suite repeats
them on production-like hardware once the walking skeleton runs.

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
