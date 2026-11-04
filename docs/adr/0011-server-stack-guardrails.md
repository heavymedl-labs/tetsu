# 0011. Server stack guardrails

- Status: Accepted
- Date: 2026-11-04

## Context

[ADR-0007](0007-server-stack.md) chose the server stack before the full process
in [ADR-0010](0010-evaluating-technology.md) existed. Checking the stack against
that process kept every choice, but found failures the choices can cause and left
most layers with no stated reason to revisit them. The evidence is in
[Server stack research](../research/server-stack.md#community-review).

## Decision

Keep the stack. Build on it under these rules, and revisit a choice when a trigger
below fires.

### Rules

- **Queries name their columns.** No `SELECT *` or `RETURNING *`. A cached statement
  using them fails after any column change.
- **Column types change by expand and contract.** Never in place, because cached
  statements then fail on every connection until the cache is cleared. If this
  error (`0A000`) still occurs, the adapter clears the statement caches and fails
  the request.
- **Row mapping uses `try_get`.** `Row::get` panics on a type mismatch.
- **Dynamic queries skip the statement cache.** It has no size limit, here or on
  the server.
- **Every pool sets `wait` and `create` timeouts.** The defaults wait forever, and
  the driver's `connect_timeout` does not cover a server that accepts and then
  stalls. A full pool fails the request fast.
- **Every connection sets `statement_timeout`.** Dropping a request does not stop
  its query, so a slow query would otherwise hold its connection for the next
  request.
- **`connection closed` is an expected error.** The pool can hand out a connection
  that has just died. Adapters report it as temporary and retry only work that is
  safe to repeat.
- **The migration runner polls for its lock.** It uses `pg_try_advisory_lock` on a
  direct connection, never a blocking `pg_advisory_lock`, which can deadlock with
  `CREATE INDEX CONCURRENTLY` and leave an invalid index.
- **Our JSON type handles both directions.** ADR-0007's lint on `axum::Json` also
  rejects the request extractor, so our type replaces both.

### Revisit triggers

Any of these on a dependency we chose opens a **Decision needed** issue:

- A security advisory stays unpatched for 30 days.
- No release in 18 months, or the main maintainer steps away with no successor.
- A license change away from permissive terms.

Each layer also has its own trigger:

| Layer | Revisit when |
| --- | --- |
| Tokio | A many-core benchmark on production-like hardware shows a thread-per-core runtime using at least 25% less CPU per request |
| Axum | Framework overhead exceeds 10% of server CPU on a real hot path in the [ADR-0005](0005-testing-and-performance.md) suite |
| `tokio-postgres` and `deadpool-postgres` | The revisit in [ADR-0009](0009-defer-generated-queries.md), which should also weigh SQLx and measure over a real network |
| Migration runner | [#18](https://github.com/heavymedl-labs/tetsu/issues/18) picks a pre-deploy job, or a runner bug fails a deploy |
| Askama | Rebuilding after a template change slows front-end work enough that people ask for live reload |

## Alternatives considered

- **Write these only in the research page.** Cheaper, but research loses to an ADR
  and binds nobody.
- **Supersede ADR-0007.** Would restate a stack that has not changed. These rules
  add to it.
- **Replace `deadpool-postgres` with bb8.** bb8 has a default connection timeout,
  but no statement cache for Postgres, so we would own one.
- **Adopt goose now.** It handles these migration cases well, but whether a separate
  tool fits depends on when migrations run, which #18 decides.
- **Move to MiniJinja for live reload.** Render time was 4.6 times Askama's in a
  local test, and it gives up compile-time checks.

## Consequences

Known failure modes become rules before any code exists, and each choice has a stated
reason to reopen it. Nothing enforces these yet: checks are chosen with the walking
skeleton, as ADR-0007 says, and advisory scanning in CI needs its own ADR. The numeric
triggers are judgment calls, not measurements.

The failure tests ran once on a local container, not in CI, and ADR-0005's suite
should cover the ones that can regress. Some ADR-0010 steps ran after ADR-0007 was
accepted, which cannot be undone: this ADR records the result, not a clean process.

## Sources

- [Server stack research](../research/server-stack.md): failure tests, project health,
  advisories, and added options, 2026-11-04.
- `tokio-postgres` documentation, docs.rs/tokio-postgres: `Row::get` and
  `Row::try_get`.
- `deadpool-postgres` and `deadpool` source, github.com/deadpool-rs/deadpool: pool
  timeouts and the statement cache.
- PostgreSQL documentation: `CREATE INDEX CONCURRENTLY`, advisory locks, and
  `statement_timeout`.
- RustSec advisory database, rustsec.org.
