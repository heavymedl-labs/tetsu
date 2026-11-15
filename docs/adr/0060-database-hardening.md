# 0060. Database hardening after the design review

- Status: Accepted
- Date: 2026-11-15
- Amends [ADR-0042](0042-tenant-isolation.md) (the server floor, keys, and grants) and
  [ADR-0011](0011-server-stack-guardrails.md) (connection timeouts)

## Context

The [database design review](../research/database-design-review.md) of 2026-11-14 found Tetsu's isolation and
migration runner ahead of published practice, and its weak points where a rule lives only in Rust, in a per-file
convention, or nowhere:

- PostgreSQL 18.7, released 2026-11-12, fixes four flaws in row security and definer functions that 18.6 leaves
  open, among them CVE-2026-116311 (`DROP OWNED` strips restrictive policies) and CVE-2026-116316 (types planted in
  `pg_temp`, CVSS 8.8). Neon applied 18.7 on 2026-11-13, on each compute's next restart.
- `tenancy.locations` is keyed `(tenant_id, sort_order)`, against ADR-0042's `(tenant_id, id)`, and the catalog check
  missed it.
- Slugs, colors, names, addresses, and event payloads are bounded only in Rust.
- Nothing bounds a query once it is sent: a stalled network holds a request until TCP keepalive, two hours by
  default, and no setting bounds a whole transaction.
- The catalog check allows `TEMPORARY` on the database to PUBLIC.

Only tenancy's and audit's migrations have run anywhere, on the demo's fictional data, so these are cheap now.

## Decision

- **Tetsu refuses servers older than 18.7,** and the floor moves with every PostgreSQL advisory on row security,
  roles, or definer functions. CI and benchmark images move with it.
- **Keys follow one rule, which the catalog check enforces.** A tenant-owned table's primary key is exactly
  `(tenant_id, id)`, with `id uuid DEFAULT uuidv7()`. A global registry, such as tenants or operators, keys on a
  `bigint` identity. A value that must reveal neither time nor order, such as a session's public id or a WebAuthn
  user handle, is a random UUID (RFC 9562, section 8). `tenancy.locations` gets an `id`, and its order becomes a
  deferrable unique `(tenant_id, sort_order)`, so a reorder can swap two in one transaction.
- **The database enforces every bound and format Rust does.** Every `text`, `bytea`, and `jsonb` column has a
  `CHECK` on its size, and fixed formats, such as slugs and colors, a `CHECK` on their shape. The catalog check
  flags any such column without one. Tables copied with `LIKE` include constraints. Rust still checks first, so
  people get a clear message; the database is the backstop. Time zones wait for
  [#21](https://github.com/heavymedl-labs/tetsu/issues/21), since a `CHECK` cannot consult the zone list.
- **A stalled network ends the connection within about half a minute,** since every connection sets
  `tcp_user_timeout` and keepalives of seconds, not hours, and the pool never hands out a closed connection. A timer
  around each query would bound it closer to `statement_timeout`, but every adapter's queries would have to go
  through it; the half minute is enough until load says otherwise.
- **Every scoped transaction sets `transaction_timeout` and `idle_in_transaction_session_timeout`,** 15 and 10
  seconds for a module, and enough for the dispatcher's longest pass, with its other limits. They are set inside the
  transaction because Postgres cannot tighten a session-level one there. Their errors sort as temporary
  ([ADR-0011](0011-server-stack-guardrails.md)).
- **The catalog check refuses `TEMPORARY` on the database for PUBLIC,** as the demo runbook already revokes it. The
  review also suggested a login of its own for the dispatcher, which [ADR-0061](0061-one-service-until-domains.md) defers
  for the first real business's data.
- **The sessions table keeps room for its updates,** with `fillfactor = 80`, so each request's write to
  `last_seen_at`, which no index names, stays on its page.

## Alternatives considered

- **Keep 18.6 until a flaw is shown to reach Tetsu.** Three of the four need preconditions Tetsu meets today:
  restrictive policies, `security_barrier` quals, and unqualified casts.
- **Bounds in Rust alone.** One forgotten check in a new adapter, an import, or a manual fix stores what Rust would
  refuse. ADR-0042 already rests isolation on the database for the same reason.
- **Reset each connection on checkout** (`RecyclingMethod::Clean`). Every scoped transaction already sets its own
  role, tenant, and timeouts, so a reset adds a round trip and covers nothing.
- **`ON DELETE CASCADE` from sessions to operators.** Removing an operator must end its sessions in the same
  transaction (ADR-0059). The plain foreign key refuses a removal that forgets, which a test can show; a cascade
  would hide it.

## Consequences

- One migration for tenancy (locations' `id`, the bounds), one for audit (its bounds), and edits to identity's and
  the outbox's unapplied files. The demo reseeds after the tenancy migration.
- Audit's retention, and with it whether `records` is partitioned by time, which its unique `event_id` key blocks, is
  raised on [#22](https://github.com/heavymedl-labs/tetsu/issues/22).
- The review's later items wait for the slice that touches each: the dispatcher's short claim with a lease, the
  session sweep, lock-timeout retries in the runner, a conflict error class, an ADR on `btree_gist` before bookings,
  and shipping audit records to a separate store before the Level 2 review.

## Sources

- [Postgres database design: a practice review](../research/database-design-review.md), 2026-11-14, and its sources.
- PostgreSQL, ["PostgreSQL 18.7, 17.12, 16.16, 15.20, 14.25, and 19 RC 2 Released!"](https://www.postgresql.org/about/news/postgresql-187-1712-1616-1520-1425-and-19-rc-2-released-3442/),
  2026-11-12, and [security information](https://www.postgresql.org/support/security/).
- Neon, [changelog, 2026-11-13](https://neon.com/docs/changelog/2026-11-13).
