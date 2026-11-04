# Server stack research

Research for [ADR-0007](../adr/0007-server-stack.md), run 2026-11-03 by Kurt
Medley with a coding agent. Each layer of the stack was benchmarked against its
competitors and, where feasible, a homegrown baseline. This is research, not a
decision. Where it disagrees with an ADR, the ADR wins.

## Findings

1. **The database driver is the only layer that moves request throughput.**
   End to end, `tokio-postgres` served 50% more requests than SQLx at 30% lower
   median latency, with a much tighter p99.
2. **HTTP frameworks are interchangeable on speed.** Axum, actix-web, Salvo,
   and raw hyper all landed within run-to-run noise. Axum adds no measurable
   cost over hyper.
3. **io_uring and thread-per-core runtimes did not beat Tokio.** Their published
   1.5 to 2x gains did not reproduce on one Linux core.
4. **Templates cost microseconds.** Every engine renders a 7 KB page in 3.5 to
   5 µs, about 0.15% of one database transaction. A naive homegrown renderer
   was the slowest.
5. **No migration library fits as is.** SQLx's migrator needs SQLx
   connections; refinery has no advisory lock and cannot run a migration
   outside a transaction.

## Criteria

| Criterion | Question |
| --- | --- |
| Performance | Throughput, and p99 latency under bursts such as a 6:00 a.m. check-in rush |
| Scalability | Client CPU per request and connections needed per replica |
| Maintainability | Active maintainers, release cadence, open issues, minimum Rust version |
| Composability | Fits hexagonal modules and the composition root ([ADR-0003](../adr/0003-architecture.md)) |
| Compile-time safety | Catches mistakes before runtime, for agent-written code ([ADR-0005](../adr/0005-testing-and-performance.md)) |
| Security defaults | Escaping, parameterized SQL, row-level security support |

## Method

The numbers below come from a first local pass. All runs were interleaved and
repeated at least twice; tables show the range. The same suites now live in
[benchmarks/](../../benchmarks/README.md) and rerun on demand in CI with
averages across runs ([ADR-0008](../adr/0008-benchmark-harness.md)).

- **Machine:** Apple M1 Max, 10 cores, macOS, Rust 1.94 and 1.99, release
  builds with fat LTO. Desktop apps used 2 to 3 cores throughout, so absolute
  numbers are low and tails are noisy. Compare rows, not absolutes.
- **Database:** PostgreSQL 14 on loopback, 400,000 classes across 200 tenants,
  and row-level security enforced for a non-superuser role. Every operation is
  one transaction that sets `app.tenant_id` and then runs its statements, the
  pattern [tenant isolation](https://github.com/heavymedl-labs/tetsu/issues/8)
  is likely to use. 64 concurrent tasks share a pool of 16 connections.
- **Loopback caveat:** with no network latency, driver CPU is exaggerated and
  round trips are understated. Over a real network, round trips dominate, which
  favors pipelining further.
- **Load generator:** `oha` 1.x.

## Async runtime

Each runtime ran the same minimal HTTP/1.1 keep-alive responder on one core of
a 2-CPU Linux VM (kernel 6.6, io_uring available), with `oha` on the other.

| Runtime | Requests/s | p99 |
| --- | --- | --- |
| Tokio, current thread | 155k to 208k | 0.49 to 1.04 ms |
| Tokio, multi-thread, 1 worker | 161k to 223k | 0.48 to 0.80 ms |
| monoio 0.2 (io_uring) | 165k to 188k | 0.56 to 0.60 ms |
| compio 0.19 (io_uring) | 146k to 156k | 0.57 to 0.61 ms |

Thread-per-core designs may pull ahead at higher core counts; a 2-CPU VM cannot
show that. monoio's last release was August 2024 and glommio's March 2024.
Tokio's io_uring support is experimental and covers files only. async-std was
discontinued in March 2025. Every database driver, HTTP framework, and object
storage client we would use is built on Tokio.

## HTTP framework

Four server threads, 128 connections, 8 seconds per endpoint. "JSON" serializes
50 items per request.

| Framework | Plain text, requests/s | JSON, requests/s |
| --- | --- | --- |
| Axum 0.8.9 | 127k to 135k | 95k to 132k |
| actix-web 4.18 | 116k to 123k | 111k to 122k |
| Salvo 1.1 | 123k to 134k | 99k to 124k |
| hyper 1.x, homegrown routing | 120k to 135k | 107k to 128k |
| Axum with trace, timeout, and request ID layers | 123k to 125k | 88k to 102k |

The middleware stack costs about 1 µs per request. Maintenance over the last 52
weeks:

| Framework | Recent downloads | Active contributors | Notes |
| --- | --- | --- | --- |
| Axum | 151M | 25 | Tokio project; Tower middleware shared with hyper, tonic, and others |
| actix-web | 13M | 11 | Own middleware model; per-thread runtimes |
| Salvo | 1M | 25 | 1.1 requires Rust 1.95, a day after release |

## Postgres access

Operations per second, three workloads.

| Driver | Lookup by ID | List 50 rows | Book (update and insert) |
| --- | --- | --- | --- |
| SQLx 0.9 | 11.9k to 13.6k | 12.1k to 13.1k | 10.5k to 11.3k |
| Diesel 2.3 with diesel-async 0.9 | 12.1k to 12.8k | 11.4k to 11.8k | 8.2k to 9.0k |
| `tokio-postgres` 0.7 with `deadpool-postgres` 0.14 | 17.7k to 20.3k | 18.1k to 19.4k | 13.1k to 15.7k |
| `tokio-postgres`, pipelined | 22.7k to 24.2k | 20.3k to 22.5k | 19.0k to 20.4k |
| Cornucopia 1.0, as generated | 12.1k | 11.0k to 11.6k | 8.4k to 8.6k |
| Cornucopia 1.0, statements prepared first | 18.8k to 19.4k | 15.5k to 16.4k | 8.5k to 10.1k |

Client CPU per transaction on the list workload: SQLx 212 µs, Diesel 214 µs,
`tokio-postgres` 133 µs, pipelined 110 µs. Memory was the same for all, about
14 MB.

End to end, Axum served a schedule page: a tenant-scoped transaction, a 50-row
query, and an Askama render.

| Stack | Requests/s | p50 | p99 |
| --- | --- | --- | --- |
| Axum, SQLx, Askama | 9.9k to 10.2k | 6.0 ms | 10.3 to 17.3 ms |
| Axum, `tokio-postgres`, Askama | 14.9k to 15.3k | 4.2 ms | 4.9 to 6.2 ms |
| Axum, `tokio-postgres` pipelined, Askama | 16.3k to 16.4k | 3.8 ms | 5.8 to 7.3 ms |

Notes on each option:

- **SQLx** checks queries at compile time and ships a migrator, but is the
  slowest driver here. Its issue tracker has reported the gap since 2021
  ([#1481](https://github.com/launchbadge/sqlx/issues/1481),
  [#2436](https://github.com/launchbadge/sqlx/issues/2436)). It cannot
  pipeline. It moved from LaunchBadge to the `transact-rs` organization in May
  2026, has 781 open issues, and its lead maintainer made 10 commits in the
  last year.
- **Diesel** has the healthiest project (two maintainers with hundreds of
  commits a year), but diesel-async was no faster than SQLx, and its query DSL
  hides the SQL that row-level security and tuning need.
- **`tokio-postgres`** is the fastest and supports pipelining. It is mature;
  most recent work comes from one maintainer. Alone it has no compile-time
  query checks.
- **Cornucopia** generates typed Rust from `.sql` files by preparing each query
  against a live database. The generated crate depends only on
  `tokio-postgres`, so there is no runtime library to lose. Two defects cost
  speed: inside a transaction it skips the statement cache unless each
  statement is prepared first, and generated `execute` calls never use a
  prepared statement. Both are in generated code and fixable. Two people
  maintain it, and it merged back the Clorinde fork for 1.0.
- **SeaORM** was not benchmarked. It builds on SQLx, so it inherits the driver
  cost, and as an ORM it hides SQL.

## Migrations

Compared by behavior, since speed does not matter.

| Need | SQLx migrator | refinery 0.10 | Homegrown on `tokio-postgres` |
| --- | --- | --- | --- |
| Works with `tokio-postgres` | No, needs an SQLx connection | Yes | Yes |
| Tracking table per module | Yes, `sqlx.toml` | Yes, `set_migration_table_name` | Yes |
| Advisory lock for replicas starting together | Yes | No | Yes, a few lines |
| Run a file outside a transaction (`CREATE INDEX CONCURRENTLY`) | Yes | No | Yes |
| Checksums and divergence checks | Yes | Yes | Yes |
| Embedded in the binary | Yes | Yes | Yes |

A homegrown runner is about 200 lines. Squawk, a Rust linter for Postgres
migrations, catches locking and downtime risks in CI whichever runner is used.
pgroll and Reshape automate zero-downtime expand and contract, but run as
separate tools with their own migration formats.

## HTML templating

A 50-row schedule page, 7 KB, with strings that need escaping. Best of five runs
of 200,000 renders.

| Engine | Render time | Escapes by default | Notes |
| --- | --- | --- | --- |
| Askama 0.16 | 4.8 to 5.0 µs | Yes | Jinja-style files checked at compile time; 16.7M recent downloads |
| Sailfish 0.11 | 3.5 µs | Yes | EJS-style files checked at compile time; 81k recent downloads |
| Maud 0.27 | 4.0 µs | Yes | HTML as Rust macros; last release February 2025 |
| Homegrown string building | 6.0 µs | Only where we remember to call it | Slower than every library |

## Sources

- Benchmarks and code reading described above, 2026-11-03.
- crates.io API and GitHub contributor statistics, read 2026-11-03.
- SQLx 0.9.0 release discussion, github.com/transact-rs/sqlx, discussion 4271.
- SQLx 0.9 source: `sqlx-postgres` `migrate.rs` and `sqlx-core` config
  reference.
- refinery-core 0.10.0 source: `drivers/tokio_postgres.rs`, `traits/async.rs`.
- Cornucopia 1.0, github.com/cornucopia-rs/cornucopia, and its generated code.
- async-std 1.13.1 release notes: discontinuation in favor of smol.
- Squawk, github.com/sbdchd/squawk.
