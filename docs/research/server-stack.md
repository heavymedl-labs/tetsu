# Server stack research

Research for [ADR-0007](../adr/0007-server-stack.md), run 2026-11-03 by Kurt
Medley with a coding agent. Each layer of the stack was benchmarked against its
competitors and, where feasible, a homegrown baseline. This is research, not a
decision. Where it disagrees with an ADR, the ADR wins.

## Findings

1. **The database driver moves request throughput most.** End to end,
   `tokio-postgres` served 27% more requests than SQLx, with 20% lower median
   latency and 25% lower p99.
2. **HTTP frameworks differ by microseconds.** Axum uses 11 to 13% more server
   CPU per request than raw hyper on trivial handlers, about 1 to 3 µs. A
   list transaction alone costs about 120 µs of client CPU.
3. **`axum::Json` is the one easy HTTP saving.** A small response type on
   `serde_json::to_vec` uses 24% less CPU and serves 30% more JSON requests.
4. **io_uring runtimes did not beat Tokio.** On one core they used about 12%
   more CPU per request and served 7% fewer requests.
5. **Templates cost microseconds.** Askama renders a 7 KB page in 3.9 µs, and
   Sailfish saves about 1.4 µs of that.
6. **No migration library fits as is.** SQLx's migrator needs SQLx
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

Numbers come from 10-run CI benchmarks unless marked local. Each option is
compared with its baseline in the same run, then averaged, with a 95% confidence
interval ([ADR-0008](../adr/0008-benchmark-harness.md),
[benchmarks/](../../benchmarks/README.md)). `http` and `rt` come from run
41773162601; `db`, `e2e`, and `tpl` come from run 41751597568. No run had request
errors.

- **Machine:** GitHub standard runners, 2 vCPUs, Rust 1.99.1, release builds with
  fat LTO. The server and load generator get one core each. Raw throughput
  varies 20 to 45% between runners, so compare ratios, not absolutes.
- **Database:** PostgreSQL 18.6 on loopback, 400,000 classes across 200 tenants,
  and row-level security enforced for a non-superuser role. Every operation is
  one transaction that sets `app.tenant_id` and then runs its statements, the
  pattern [tenant isolation](https://github.com/heavymedl-labs/tetsu/issues/8)
  is likely to use. 64 concurrent tasks share a pool of 16 connections.
- **Loopback caveat:** with no network latency, driver CPU is exaggerated and
  round trips are understated. Over a real network, round trips dominate, which
  favors pipelining further.
- **Load generator:** `oha` 1.16 on one core. It reaches its limit near 70 to
  80k requests/s, before the server does on plain text and in the runtime suite.
  There, without middleware, server cores busy stays below 0.90 and CPU per
  request is the fair comparison. Throughput ratios on those endpoints understate real differences.
- **Local pass:** an earlier pass on an Apple M1 Max with PostgreSQL 14 is in this
  file's git history. It showed a larger driver gap, 50% end to end, and a false
  tie between HTTP frameworks.

## Async runtime

The same minimal HTTP/1.1 keep-alive responder on one core, 64 connections. `oha`
set the throughput for all four: server cores busy was 0.69 to 0.73.

| Runtime | Median CPU per request | CPU vs Tokio multi-thread | Throughput vs Tokio multi-thread |
| --- | --- | --- | --- |
| Tokio, current thread | 8.5 µs | 0.96 ± 0.01 | 1.01 ± 0.01 |
| Tokio, multi-thread, 1 worker | 8.7 µs | 1.00 | 1.00 |
| monoio 0.2 (io_uring) | 9.5 µs | 1.12 ± 0.03 | 0.93 ± 0.02 |
| compio 0.19 (io_uring) | 9.6 µs | 1.13 ± 0.03 | 0.93 ± 0.02 |

These CPU numbers were taken below saturation, which can change how much work
each wakeup batches, so treat small differences with care.

Thread-per-core designs may pull ahead at higher core counts; one core cannot
show that. monoio's last release was August 2024 and glommio's March 2024.
Tokio's io_uring support is experimental and covers files only. async-std was
discontinued in March 2025. Every database driver, HTTP framework, and object
storage client we would use is built on Tokio.

## HTTP framework

One server core, 128 connections, 8 seconds per endpoint. The JSON endpoint
serializes 50 items into 3,591 bytes per request. "Axum with `to_vec`" is Axum
with a small response type on `serde_json::to_vec`.

| Option | Plain, median CPU | Plain, CPU vs hyper | JSON, median CPU | JSON, CPU vs hyper | JSON, throughput vs hyper |
| --- | --- | --- | --- | --- | --- |
| hyper 1.x, path match only | 10.9 µs | 1.00 | 19.0 µs | 1.00 | 1.00 |
| actix-web 4 | 11.0 µs | 1.02 ± 0.02 | 19.9 µs | 1.05 ± 0.02 | 0.95 ± 0.02 |
| Axum with `to_vec` | 12.0 µs | 1.11 ± 0.02 | 21.9 µs | 1.13 ± 0.03 | 0.89 ± 0.02 |
| Axum 0.8 with `axum::Json` | 11.9 µs | 1.11 ± 0.02 | 29.9 µs | 1.48 ± 0.06 | 0.68 ± 0.03 |
| Salvo 1.1 | 13.1 µs | 1.21 ± 0.03 | 36.7 µs | 1.93 ± 0.06 | 0.53 ± 0.02 |

On plain text `oha` set the throughput: server cores busy was 0.80 to 0.89. On
JSON the server did, at 0.97 to 0.99.

Trace, timeout, and request ID layers on Axum add 3 to 4 µs per request, a median
of 14.9 µs on plain text and 33.8 µs on JSON.

To find where Axum's cost goes, a local test on a 2-CPU Linux container removed
one layer at a time, over 8 rounds:

| JSON server | Extra CPU vs hyper | Allocations per request |
| --- | --- | --- |
| hyper with Axum's boxed body | +0.15 µs | 9 (hyper: 8) |
| hyper serving an Axum `Router` | +0.80 µs | 19 |
| Axum with `to_vec` and `TCP_NODELAY` | +0.93 µs | 18 |
| Axum with `to_vec` | +0.95 µs | not counted |

The router and handler layers add about nine allocations per request, on plain
text as well as JSON. `TCP_NODELAY` and the boxed body cost nothing measurable.
Allocation counts were not repeated in CI, so CI shows the overhead but not its
cause.

Axum's extra CPU over hyper is about 1 µs on plain text and 3 µs on JSON in CI.
About 2 µs of that is unexplained. It is too small to affect the choice.
`axum::Json` is a separate cost: it makes about as many allocations as `to_vec`
but serializes through a `BytesMut` writer.

Maintenance over the last 52 weeks:

| Framework | Recent downloads | Active contributors | Notes |
| --- | --- | --- | --- |
| Axum | 151M | 25 | Tokio project; Tower middleware shared with hyper, tonic, and others |
| actix-web | 13M | 11 | Own middleware model; per-thread runtimes |
| Salvo | 1M | 25 | 1.1 requires Rust 1.95, a day after release |

## Postgres access

Throughput relative to SQLx, three workloads, with client CPU per transaction in
parentheses. SQLx's CPU is 113, 187, and 142 µs.

| Driver | Lookup by ID | List 50 rows | Book (update and insert) |
| --- | --- | --- | --- |
| SQLx 0.9 | 1.00 | 1.00 | 1.00 |
| Diesel 2.3 with diesel-async 0.9 | 0.81 ± 0.03 (135 µs) | 0.87 ± 0.02 (192 µs) | 0.65 ± 0.01 (215 µs) |
| `tokio-postgres` 0.7 with `deadpool-postgres` 0.14 | 1.40 ± 0.04 (74 µs) | 1.33 ± 0.03 (120 µs) | 1.31 ± 0.06 (101 µs) |
| `tokio-postgres`, pipelined | 1.55 ± 0.09 (65 µs) | 1.39 ± 0.03 (115 µs) | 1.44 ± 0.06 (85 µs) |
| Cornucopia 1.0, as generated | 0.65 ± 0.03 (142 µs) | 0.82 ± 0.01 (180 µs) | 0.60 ± 0.02 (204 µs) |
| Cornucopia 1.0, statements prepared first | 1.40 ± 0.06 (75 µs) | 1.33 ± 0.03 (118 µs) | 0.71 ± 0.02 (175 µs) |

Within a run, pipelining beat sequential `tokio-postgres` by 1.11 ± 0.04 on
lookup, 1.05 ± 0.04 on list, and 1.10 ± 0.02 on book. Prepared Cornucopia matched
it on reads (1.00) and reached only 0.54 ± 0.02 on writes, the second defect
below. Local memory use was the same for all, about 14 MB.

End to end, Axum served a schedule page: a tenant-scoped transaction, a 50-row
query, and an Askama render.

| Stack | Throughput vs SQLx | p50 | p99 |
| --- | --- | --- | --- |
| Axum, SQLx, Askama | 1.00 | 33.6 ms | 53.5 ms |
| Axum, `tokio-postgres`, Askama | 1.27 ± 0.02 | 26.9 ms | 40.0 ms |
| Axum, `tokio-postgres` pipelined, Askama | 1.32 ± 0.04 | 25.4 ms | 39.1 ms |

Notes on each option:

- **SQLx** checks queries at compile time and ships a migrator, but is slower
  than `tokio-postgres` on every workload. Its issue tracker has reported the gap since 2021
  ([#1481](https://github.com/launchbadge/sqlx/issues/1481),
  [#2436](https://github.com/launchbadge/sqlx/issues/2436)). It cannot
  pipeline. It moved from LaunchBadge to the `transact-rs` organization in May
  2026, has 781 open issues, and its lead maintainer made 10 commits in the
  last year.
- **Diesel** has the healthiest project (two maintainers with hundreds of
  commits a year), but diesel-async was slower than SQLx, and its query DSL
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

A 50-row schedule page, 7 KB, with strings that need escaping, 100,000 renders.

| Engine | Render time | Speed vs Askama | Escapes by default | Notes |
| --- | --- | --- | --- | --- |
| Sailfish 0.11 | 2.5 µs | 1.54 ± 0.06 | Yes | EJS-style files checked at compile time; 81k recent downloads |
| Askama 0.16 | 3.9 µs | 1.00 | Yes | Jinja-style files checked at compile time; 16.7M recent downloads |
| Homegrown string building | 4.9 µs | 0.81 ± 0.01 | Only where we remember to call it | |
| Maud 0.27 | 6.3 µs | 0.69 ± 0.13 | Yes | HTML as Rust macros; last release February 2025 |

Maud's result is noisy, and it beat Askama locally, so its rank is not settled.

## Community review

On 2026-11-04 we compared the stack with common practice. No survey of what
teams run turned up, so this rests on blog posts, project docs, source code, and
crates.io downloads. Most of the stack matches common practice, and no choice
needs reversing. Recent downloads favor Axum (151M) over actix-web (13M). Askama
is the usual pick for type-checked templates. [Svix](https://www.svix.com/blog/row-level-security-defense-in-depth/)
runs the same tenant pattern on Axum: `set_config(..., true)` inside a
transaction.

### Departures from common practice

- **`tokio-postgres` instead of SQLx.** SQLx is the usual default. The throughput
  gap above was measured on loopback, which exaggerates driver CPU, so speed is
  a weaker argument than the table suggests.
- **No build-time query checks.** ADR-0007 asks for mistakes to be caught at
  compile time, and [ADR-0009](../adr/0009-defer-generated-queries.md) gives that
  up for queries through the walking skeleton. SQLx would provide it today.
- **Our own migration runner.** Teams usually adopt a tool. The Rust libraries
  still fall short: refinery closed its
  [request to run outside a transaction](https://github.com/rust-db/refinery/issues/277)
  as not planned. We did not evaluate dbmate, goose, or Atlas, which run as
  separate tools. [Rust on Nails](https://cloak.pages.dev/blog/rust-on-nails)
  pairs dbmate with Axum and Cornucopia.

### Failure tests

Run 2026-11-04 on PostgreSQL 18.6 in a local container, through `tokio-postgres`
0.7 and `deadpool-postgres` 0.14. The test code was throwaway and is not in this
repository. Each case ran once and none ran in CI. An earlier `psql` pass on
PostgreSQL 14.24 agreed on the cached statement and lock results. The rules these
tests lead to are in [ADR-0011](../adr/0011-server-stack-guardrails.md).

**Cached statements across schema changes.** The failure is
`0A000 cached plan must not change result type`.

| Schema change | `SELECT *` | Explicit columns |
| --- | --- | --- |
| Add a column | Fails | Works |
| Drop a column the query does not use | Fails | Works |
| Change the type of a selected column | Fails | Fails |

The failure does not clear. Repeating the statement, calling `prepare_cached`
again, and returning the connection to the pool all still failed. Only
`statement_caches.clear()` recovered.

**Migration lock.** A runner blocked in `pg_advisory_lock` deadlocks with another
runner's `CREATE INDEX CONCURRENTLY`, as
[golang-migrate](https://github.com/golang-migrate/migrate/issues/960) reports.
Which side loses depends on timing:

| When the build starts | Build | Second runner | Index |
| --- | --- | --- | --- |
| Under `deadlock_timeout` after the other runner starts waiting | Succeeds | `40P01` deadlock error | Valid |
| Over `deadlock_timeout` after the other runner starts waiting | `40P01` deadlock error | Gets the lock | Invalid, left behind |
| Other runner polls `pg_try_advisory_lock` | Succeeds | Gets the lock | Valid |

**Pool and connection behavior.**

| Case | Result |
| --- | --- |
| Pool full, default config | Still waiting after 2 s; no timeout by default |
| Pool full, `wait` of 100 ms | Timeout error after 102 ms |
| Database accepts TCP but never answers, default config | Still waiting after 2 s |
| Same, `create` of 100 ms | Timeout error |
| Same, driver `connect_timeout` of 1 s | Still waiting after 3 s, so it does not cover this case |
| Backends killed, connection reused after 50 ms | Works; the pool replaced it |
| Backends killed, connection reused immediately | `connection closed` error |
| Request future dropped during a 2 s query | Query kept running; the next user of that connection waited 1.8 s |
| Same query with a `statement_timeout` of 300 ms | `57014` after 313 ms; the next user waited 1.5 ms |

**Tenant context on a reused connection.** The tenant setting did not survive a
commit, a panic inside a transaction, or a dropped request future. The panic came
from `Row::get` on a type mismatch; `Row::try_get` returned an error instead.

A reused connection differs from a fresh one, though. After `SET LOCAL` or
`set_config(..., true)` ends, `current_setting` returns an empty string, not
null. With no tenant set, a policy casting it with `::bigint` returned no rows on
a fresh connection and failed with `22P02` on a reused one. `nullif(..., '')`
returned no rows on both. Both fail closed, but the error would surface as an
outage. This is an input to
[tenant isolation](https://github.com/heavymedl-labs/tetsu/issues/8).

**Statement cache growth.** 5,000 distinct statements gave a client cache of 5,000
and 5,001 prepared statements on the server, so we saw no limit. Backend memory
rose from about 1 MB to 18 MB, roughly 3.4 KB per trivial statement.

### Project health and security

Read 2026-11-04 from crates.io, GitHub, and RustSec. Contributors and the top
contributor's share cover the last 52 weeks of commits to the whole repository,
including bots.

| Project | Latest release | License | Minimum Rust | Recent downloads | Contributors | Top share |
| --- | --- | --- | --- | --- | --- | --- |
| tokio | 1.54.1, Oct 2026 | MIT | 1.85 | 290M | 35 | 32% |
| axum | 0.8.9, Apr 2026 | MIT | 1.80 | 152M | 25 | 20% |
| hyper | 1.12.0, Oct 2026 | MIT | 1.63 | 266M | 12 | 58% |
| tower | 0.5.3, Jan 2026 | MIT | 1.64 | 225M | 6 | 46% of 13 commits |
| tower-http | 0.7.1, Aug 2026 | MIT | 1.65 | 204M | Not read | Not read |
| tokio-postgres | 0.7.18, Jun 2026 | MIT or Apache-2.0 | 1.85 | 25M | 9 | 72% |
| deadpool-postgres | 0.14.2, Aug 2026 | MIT or Apache-2.0 | 1.85 | 8M | 13 | 69% |
| askama | 0.16.1, Sep 2026 | MIT or Apache-2.0 | 1.88 | 17M | 17 | 61% |
| Squawk | 2.71.0, Oct 2026 | Apache-2.0 | Not read | Not read | 16 | 91% |

All licenses are permissive. The driver and the pool lean most on one person, and
tower has the least recent activity, though it is widely used.

Every RustSec advisory for these crates is patched in the versions our benchmark
lockfile uses:

| Advisory | Crate | Severity | Issue | Patched in | Locked |
| --- | --- | --- | --- | --- | --- |
| RUSTSEC-2026-0179 | `postgres-protocol` | High | Unbounded SCRAM iterations from a malicious server | 0.6.12 | 0.6.12 |
| RUSTSEC-2026-0180 | `postgres-protocol` | Medium | Panic on a malformed `hstore` value | 0.6.12 | 0.6.12 |
| RUSTSEC-2026-0178 | `tokio-postgres` | Medium | Panic on a `DataRow` with too few fields | 0.7.18 | 0.7.18 |
| RUSTSEC-2026-0384 | `tokio` | None listed | Race in `time::Interval` on 1.54.0 | 1.54.1 | 1.54.1 |
| RUSTSEC-2026-0386 | `tokio` | Informational | `AsyncFd::new` unsoundness | 1.53.3 | 1.54.1 |

Older advisories exist for tokio, hyper, h2, http, axum-core, and tower-http, all
patched well below our versions. One exception sits outside the chosen stack: the
lockfile's `h2` 0.3.27 comes through actix-web, and RUSTSEC-2026-0258 is patched in
0.4.16. Axum's HTTP/2 support would need `h2` 0.4, so the skeleton should check
that version.

### Options not evaluated earlier

**Templates.** A local run on an Apple M1 Max rendered the same 7 KB page with the
same template file, over three rounds. These runs did not use the CI setup, and
Askama took 6.1 µs here against 3.9 µs on CI.

| Engine | Render time | Time vs Askama | Checked at compile time | Notes |
| --- | --- | --- | --- | --- |
| Askama 0.16 | 6.1 µs | 1.0 | Yes | Rebuild on each template change |
| MiniJinja 3.0 | 28.5 µs | 4.6 | No | Reloads without a rebuild; Apache-2.0; 15M recent downloads; 79% one contributor |
| Tera 2.4 | 38.9 µs | 6.4 | No | MIT; 8M recent downloads; 92% one contributor |

Both runtime engines fail ADR-0007's requirement to catch mistakes at compile
time, so they lose whatever the speed. MiniJinja's reload is the real attraction,
and it may matter once designers edit templates.

**Connection pool.** bb8 is healthy enough (10M recent downloads, 0.9.1 in
November 2025) and sets a connection timeout by default, which deadpool does not.
But `bb8-postgres` last released in December 2024 and has no statement cache, so we
would write and own one. We did not benchmark it, since that missing cache decides
the matter. `deadpool-postgres` stays.

**Migration tools.** These run as separate Go binaries, so a job can use them but
the `tetsu` binary cannot embed them.

| Tool | License | Outside a transaction | Per-module table | Lock |
| --- | --- | --- | --- | --- |
| goose | MIT | `-- +goose NO TRANSACTION` | `-table` | Polls `pg_try_advisory_lock`, as a Go library option |
| dbmate | MIT | `transaction:false` | `--migrations-table` | Blocks in `pg_advisory_lock`, so it looks exposed to the deadlock above; not tested |
| Atlas | Apache-2.0 for a minimal build | `txmode none` | Not read | Not read |

goose is the strongest alternative to our runner. Atlas's community build leaves
out row-level security and migration linting, and the default binary is proprietary
with a paid tier, so it is out. We keep our runner for now and decide with
[#18](https://github.com/heavymedl-labs/tetsu/issues/18), which sets when migrations
run. Choosing a tool there would need a new ADR.

### Open items

- **Postgres TLS.** ADRs 0003 to 0009 do not mention it, and `tokio-postgres`
  needs a separate TLS crate, which needs its own ADR. Raised on
  [#18](https://github.com/heavymedl-labs/tetsu/issues/18).
- **Axum server timeouts.** Axum's unreleased changelog adds a default header read
  timeout, which suggests 0.8.9 applies none. We have not tested this.
- **Advisory checks in CI.** Nothing checks advisories automatically yet. A tool
  for it needs its own ADR once code exists.
- **When migrations run.** Practitioner guides favor a pre-deploy job over running
  at startup. A `tetsu migrate` command would fit that and keep migrations in the
  binary. Raised on #18.
- **Connection poolers.** PgBouncer's `max_prepared_statements` tracks
  protocol-level prepared statements in transaction mode, which should cover our
  cached statements, but we have not tested it. The runner's session-level
  advisory lock needs a direct connection. Raised on #18.
- **The query checking revisit.** It should weigh SQLx as well as Cornucopia and
  rerun the driver comparison with real network latency. Raised on
  [#28](https://github.com/heavymedl-labs/tetsu/issues/28).

## Sources

- Benchmarks and code reading described above, 2026-11-03. Raw results are in
  the `benchmark-report` artifact of each run.
- crates.io API and GitHub contributor statistics, read 2026-11-03.
- RustSec advisory database, rustsec.org, and project pages for goose, dbmate,
  bb8, MiniJinja, and Tera, read 2026-11-04.
- [Atlas community edition](https://atlasgo.io/community-edition), read 2026-11-04.
- Community review sources, read 2026-11-04: `deadpool` source, the Axum changelog,
  [`tokio-postgres` `Row` docs](https://docs.rs/tokio-postgres/latest/tokio_postgres/row/struct.Row.html),
  [PgBouncer configuration](https://www.pgbouncer.org/config.html),
  [Encore's Axum and Postgres guide](https://encore.dev/articles/add-postgres-to-axum),
  and [a guide to running migrations on deploy](https://pandastack.io/blog/run-database-migrations-on-deploy).
- SQLx 0.9.0 release discussion, github.com/transact-rs/sqlx, discussion 4271.
- SQLx 0.9 source: `sqlx-postgres` `migrate.rs` and `sqlx-core` config
  reference.
- refinery-core 0.10.0 source: `drivers/tokio_postgres.rs`, `traits/async.rs`.
- Cornucopia 1.0, github.com/cornucopia-rs/cornucopia, and its generated code.
- async-std 1.13.1 release notes: discontinuation in favor of smol.
- Squawk, github.com/sbdchd/squawk.
