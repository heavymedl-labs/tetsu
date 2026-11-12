# 0042. Tenant isolation

- Status: Proposed
- Date: 2026-11-11
- Settles the schema boundary enforcement that [ADR-0003](0003-architecture.md) left to this decision
- Supersedes the rule in [ADR-0040](0040-identity-and-sign-in.md) that the demo's hosts sit under a domain of its
  own: until Tetsu has domains, each demo realm is its own free host

## Context

A tenant is one business: a brand and all its locations. Isolation is Tier 1, every module that stores data waits
on it, and the invariant is plain: a tenant sees only its own data. The maintainer chose the direction on #8 on
2026-11-04: **shared now, cells later.** One shared deployment and database now, built so a business can later
move to a cell of its own, a complete deployment serving some tenants. Database per business would cost about
$2,900 a month for 100 businesses at Render's list prices, and every deploy and migration would run 100 times.

Two more forces shape it. Tetsu stays **portable**: nothing may depend on one host's features, so a move from
Render or Neon is a configuration change. And Tetsu runs on **free hosting** until a client showcase is close,
with no domain of its own yet.

The [tenant isolation research](../research/tenant-isolation.md) found that row-level security (RLS) rarely fails
because a policy is wrong. It fails because a role skips it, a table lacks it, an object runs with another role's
rights, a whole-table command ignores it, or a pooled connection carries one tenant into the next request.
Container tests on PostgreSQL 18.6 confirmed each of these, and each has a fix the catalog can check.

## Decision

Tenants share tables. Every tenant-owned row carries `tenant_id`, and Postgres row-level security enforces it, with
the application's own filters as a second layer.

### Portability

- **Only standard PostgreSQL 18 features,** so any host that runs it can run Tetsu. No host's roles, branching,
  extensions, or APIs are on the path that guards tenant data.
- **Tetsu creates its own roles** with SQL, as below. A host's built-in administrative roles, such as Neon's
  `neondb_owner` or members of `neon_superuser`, often skip RLS, so they never run migrations, serve requests,
  or run jobs that touch tenant data. They only create Tetsu's roles.
- **Tetsu refuses to start on a server older than 18.6,** checked through `server_version_num`, since 18.6 fixes
  CVE-2026-14666 in row security.

### Rows and keys

- **Every table is declared tenant-owned or global** in its module's migrations. A test compares the declaration
  with the catalog, so no table goes unclassified. Global tables are few, and each says why it is global.
- **A tenant-owned table's primary key is `(tenant_id, id)`,** every foreign key to it includes `tenant_id`, and
  every unique index starts with `tenant_id`. Foreign key and unique checks bypass RLS, so a single-column key let
  one tenant's row point at another's in testing, and a global unique index tells one tenant that a value exists
  in another.
- **Row IDs are UUIDv7,** from Postgres 18's `uuidv7()`. They never collide when one tenant's rows are restored or
  moved into another database. `tenant_id` stays a `bigint`, issued by the global tenant registry.
- **A partitioned table gets RLS on every partition,** since the parent's flags and policies do not reach them,
  and the application role gets no rights on partitions directly.

### Policies

- **Every tenant-owned table has RLS enabled and forced,** so its owner is bound too.
- **Each has two policies with the same rule:** a permissive one and a restrictive one, both
  `tenant_id = (select nullif(current_setting('app.tenant_id', true), '')::bigint)`. The restrictive copy stops a
  later, wider policy from widening every query, since permissive policies combine with OR. The `select` makes
  Postgres read the setting once per query, and `nullif` makes a missing tenant return no rows rather than an
  error.
- **No view, materialized view, or `SECURITY DEFINER` function reads tenant-owned tables** except as listed below.
  Views use `security_invoker = true`.

### Roles

- **An owner role** owns every schema and table and runs migrations. Nothing else logs in as it.
- **An application role** logs in for requests. It owns nothing, holds neither `BYPASSRLS` nor `TRUNCATE`, and
  reaches data only through module roles.
- **A `NOLOGIN` role per module** holds that module's grants on its own schema alone. The application role is
  granted each `WITH INHERIT FALSE, SET TRUE`, so it has no rights of its own and uses one module's rights at a
  time through `SET LOCAL ROLE`. A module that queries another's schema by mistake gets an error. This catches
  mistakes, not injected SQL, which could already change the tenant setting; Tetsu guards against injection with
  bound parameters ([ADR-0009](0009-defer-generated-queries.md)). Default privileges grant each new table to its
  module role, which works only because the owner role creates every table.
- **An operator role** logs in through its own pool, never the request path's. Its policies name it with `TO`, so
  what it may read is written down. How far it reaches into businesses stays with
  [#24](https://github.com/heavymedl-labs/tetsu/issues/24); during development the maintainer keeps full access to
  demo businesses ([ADR-0040](0040-identity-and-sign-in.md)).

### The tenant transaction

- **Code reaches tenant-owned tables only through a typed tenant transaction.** It opens a transaction, then sets
  the module role, `app.tenant_id`, `statement_timeout`, and `lock_timeout` in one round trip, all with
  transaction scope (`SET LOCAL` or `set_config(..., true)`). Commit or rollback clears them, which matters
  because the pool keeps session settings between checkouts ([#53](https://github.com/heavymedl-labs/tetsu/issues/53)).
- **Queries also filter by `tenant_id`,** so a missing policy is caught twice.
- **A check rejects session-level `SET`, `RESET`, and `set_config(..., false)`** anywhere in Rust or SQL.
- **The Postgres adapter tells constraints apart,** so a taken address is never reported as a taken slug.

### Tables read before a tenant is known

- **Lookups that run before a tenant is known go through one narrow `SECURITY DEFINER` function each,** such as
  web address to tenant, or session token to session. Each answers one input at a time, so no caller can list the
  table. Each is owned by its own `NOLOGIN` role, sets a fixed `search_path`, has `EXECUTE` revoked from `PUBLIC`
  in the migration that creates it, and runs in its own short transaction.
- **Staff identities and their passkeys are global,** as ADR-0040's realms require, while staff roles are
  tenant-owned. "Which businesses can I work at" is one such function, keyed on the signed-in identity.
  Customer accounts belong to one business, so they are ordinary tenant-owned rows.

### Addresses and hosts

- **A business is reached by path,** such as `/anvil-row`, on Tetsu's host, as today. Sessions stay apart because
  each is bound to its business on the server ([ADR-0040](0040-identity-and-sign-in.md)).
- **A business's own domain comes later.** Before one is served, the business proves it owns the domain, and each
  host maps to exactly one tenant. Sign-in stays on Tetsu's hosts, as ADR-0040 decides. The rest waits for the
  first business that asks.
- **While Tetsu has no domain, each demo realm runs as its own free Render service** from the one image, such as
  `tetsu-demo` for customers, `tetsu-demo-staff` for staff, and `tetsu-demo-ops` for operators. `onrender.com` is
  on the Public Suffix List, so browsers treat each service as its own site, with its own cookies and passkey
  domain, which is the separation ADR-0040 sets. Which realm a process serves, and its hosts, are configuration,
  so Tetsu's domains ([#102](https://github.com/heavymedl-labs/tetsu/issues/102)) change settings, not code. The
  three services share Render's 750 free instance hours a month, used only while awake; if they run out, every
  free service stops until the month ends.

### Tests

- **Catalog checks** fail the build when a tenant-owned table lacks forced RLS, either policy, a `tenant_id`-led
  primary key, or composite foreign keys; when any role but the owner owns a table; or when the application role
  holds `BYPASSRLS`, `TRUNCATE`, or rights on a partition.
- **Isolation tests run as the application role,** never the owner, since owner tests pass when RLS is broken.
  They assert row counts, since a blocked update or delete returns zero rows without an error.
- **A pool leak test** runs two tenants' transactions on one pooled connection and checks nothing crosses.
- **Lee Park's case is a fixture:** two businesses each hold a customer with the same email, and neither can read,
  find, or learn of the other's record.

### Limits, restore, and moves

- **Every tenant transaction sets `statement_timeout` and `lock_timeout`,** within
  [ADR-0011](0011-server-stack-guardrails.md)'s limits. Rate limits per business come with real traffic.
- **No restore or move tool is built now.** Restoring one business, or moving it to a cell, is one future tool: it
  copies one tenant's rows from any point-in-time copy of the database, using the declared table list. It works
  on any host, and arrives with operations ([#27](https://github.com/heavymedl-labs/tetsu/issues/27)) or the first
  real need.

## Alternatives considered

- **Schema per tenant.** Strong separation, but migrations run once per tenant, and the catalog grows with every
  business.
- **Database per tenant.** The strongest isolation, at about $2,900 a month for 100 businesses and 100 runs of every
  deploy. It stays open as a paid cell of one.
- **Application filters alone.** Notion, PostHog, Citus users, and GitLab enforce the tenant key in code they own.
  One missed filter leaks, and Tetsu has one maintainer, so the database is the backstop.
- **A pool per module role.** It would hold against injected SQL, but injected SQL can already change the tenant
  setting, so it buys little for many more connections.
- **Business subdomains.** Each gets its own cookies, but the parent domain then belongs on the Public Suffix List,
  which stops it being customers' passkey domain, and it needs a domain Tetsu does not have yet.
- **`bigint` row IDs.** Smaller indexes, but rows collide when one business is restored or moved into another
  database. Notion's published regret is not keying for this from the start.
- **An operator role with `BYPASSRLS`.** Simple, but it reads everything and records nothing about what it may
  read.

## Consequences

- Tenancy's tables gain their class: `tenants` and `addresses` are global, and `locations` is tenant-owned and gets
  forced RLS and both policies. Tenancy moves to the Tier 1 bar in
  [#63](https://github.com/heavymedl-labs/tetsu/issues/63), and the address lookup becomes a definer function.
- The tenant transaction, the module roles, the version check, and the catalog checks are new work in the
  Postgres crate. The catalog checks belong with [#25](https://github.com/heavymedl-labs/tetsu/issues/25).
- Hosted test 8 runs on the demo's database: the application role under these rules, and the server's version.
- [ARCHITECTURE.md](../../ARCHITECTURE.md) drops "provisional" from its tenancy section.
- Sign-in ([#20](https://github.com/heavymedl-labs/tetsu/issues/20)) builds on this: customer accounts are
  tenant-owned, staff identities global, and sessions found through a lookup function.
- UUID keys make indexes about 10% larger than `bigint` in informal benchmarks, which is acceptable here.
- Every module adds a role and default privileges in its first migration.

## Sources

- PostgreSQL 18 documentation: [Row Security Policies](https://www.postgresql.org/docs/18/ddl-rowsecurity.html),
  [CREATE POLICY](https://www.postgresql.org/docs/18/sql-createpolicy.html),
  [CREATE FUNCTION, writing `SECURITY DEFINER` functions safely](https://www.postgresql.org/docs/18/sql-createfunction.html),
  [SET ROLE](https://www.postgresql.org/docs/18/sql-set-role.html), and
  [ALTER DEFAULT PRIVILEGES](https://www.postgresql.org/docs/18/sql-alterdefaultprivileges.html).
- [PostgreSQL security advisories](https://www.postgresql.org/support/security/).
- AWS, [*SaaS Tenant Isolation Strategies*, pooled storage](https://docs.aws.amazon.com/whitepapers/latest/saas-tenant-isolation-strategies/pooled-storage-isolation-strategies.html).
- Svix, ["Row-level security as defense in depth"](https://www.svix.com/blog/row-level-security-defense-in-depth/), 2025.
- Notion, ["Herding elephants: lessons learned from sharding Postgres at Notion"](https://www.notion.com/blog/sharding-postgres-at-notion), 2021.
- [Tenant isolation research](../research/tenant-isolation.md), with every source and the container tests.
