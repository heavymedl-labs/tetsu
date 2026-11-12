# Tenant isolation research

Research for [#8](https://github.com/heavymedl-labs/tetsu/issues/8), run 2026-11-11 by Kurt Medley with a coding
agent: how Postgres keeps each business's data apart under "shared now, cells later". Four research passes covered
how row-level security fails, production practice and testing, roles and Neon, and restoring or moving one tenant;
their notes, with every source and the container tests' scripts, are in [tenant-isolation/](tenant-isolation/). This
is research, not a decision. Where it disagrees with an ADR, the ADR wins.

The chosen direction holds up: shared tables, `tenant_id` on every tenant-owned row, row-level security (RLS), and
the tenant set per transaction with `set_config(..., true)`. This is the pattern AWS calls required for the pool
model and the one Svix runs in production Rust. RLS rarely fails because its rule is wrong. It fails because a
role skips it, a table lacks it, an object runs as someone else, a whole-table command ignores it, or connection
state carries a tenant into the next request. Each of those holes has a known fix, and most can be checked from the
catalog in CI. Three choices matter most for the coming ADR. Put `tenant_id` first in every primary key, foreign
key, and unique index. Never run the app, or any job that touches tenant data, as a Neon console role, because those
roles hold `BYPASSRLS`. Settle the row ID type now, before real data exists, since it decides how cheaply one tenant
can later be restored or moved to its own cell. Neon restores only whole branches, and only within 6 hours on the
Free plan, so per-tenant restore and per-tenant moves are one copy tool that Tetsu must build itself.

**How to read the citations.** "Container test" marks the researcher's own probes on a throwaway `postgres:18`
Docker container that reported PostgreSQL 18.6 on 2026-11-11. They are first-hand observations, not published
sources. "(vendor)" marks a source that sells a related product. "(secondary)" marks a summary of someone else's
work. "(older)" marks a source from before 2022. Evidence strength is **strong** when official docs agree with a
container test or several primary sources, **moderate** when it rests on primary docs plus reasoning or one
production account, and **weak** when it rests on vendor advice or informal tests alone.

## Recommendations for ADR-0042

| Open choice | Recommendation | Main reason | Evidence |
| --- | --- | --- | --- |
| Table flags | `ENABLE` and `FORCE ROW LEVEL SECURITY` on every tenant-owned table, and on each partition separately | Owners skip RLS without `FORCE`, and the flags do not reach partitions | Strong |
| Policy shape | One permissive policy plus one restrictive policy, both `tenant_id = (select nullif(current_setting('app.tenant_id', true), '')::bigint)` | Permissive policies are OR'd, so a later admin policy would widen access; the `select` wrapper ran 3.8 times faster on a sequential scan | Strong for the restrictive policy, moderate for the speedup |
| Roles | A SQL-created owner role for migrations, a separate app login role with `NOSUPERUSER NOBYPASSRLS` that owns nothing and has no `TRUNCATE`; never `neondb_owner` on any data path | Neon console roles hold `BYPASSRLS`, which `FORCE` cannot override | Strong |
| Keys and indexes | `PRIMARY KEY (tenant_id, id)`, composite foreign keys `(tenant_id, x)`, every unique index led by `tenant_id` | Constraint checks bypass RLS, so single-column keys allow cross-tenant links and leak existence | Strong |
| Row IDs | UUIDv7 row IDs inside the composite key; `tenant_id` stays `bigint`, issued by the global tenant registry | Rows copy verbatim into another cell, and IDs in links survive a move | Moderate |
| Tenant context | A typed tenant transaction that runs `set_config(..., true)` first, inside `BEGIN`, plus explicit `tenant_id` filters in queries; a lint bans session-level `SET` and `set_config(..., false)` | Session state on a reused connection is the documented leak path with `RecyclingMethod::Fast` | Strong |
| Module boundaries | Per-module `NOLOGIN` roles reached by `SET LOCAL ROLE` on the one pool, granted to the login role `WITH INHERIT FALSE, SET TRUE` | Catches a module that queries another schema by mistake, at no connection cost; it does not stop injected SQL | Moderate |
| Tables read before a tenant is known | A narrow `SECURITY DEFINER` lookup function, such as host to tenant, owned by a dedicated `NOLOGIN` role, with a fixed `search_path` and `EXECUTE` revoked from `PUBLIC`, run in its own short transaction | Callers get one answer per input and cannot list the table | Moderate |
| Accounts that span tenants | Global staff identities with no `tenant_id`; staff roles under tenant RLS; a definer function keyed on `app.account_id` for "my businesses"; operators on a separate login role and pool, never on the request path | Matches ADR-0040's realms and Nile's published table shapes | Moderate |
| Isolation tests | Catalog checks in CI, behaviour tests run as the app role, a pool leak test, and a version floor check | Owner or superuser tests give false passes, and missing RLS on one table is the common real leak | Strong |
| Version floor | PostgreSQL 18.6 or later, checked at startup with `server_version_num >= 180006` | 18.6 fixes CVE-2026-14666 in row security caching | Strong |
| Restore and cell moves | One tool that copies one tenant's rows from a Neon branch, with the table list taken from the catalog and checked against a declared list | Neon restores whole branches only | Moderate |
| Timeouts and limits | `statement_timeout` and `lock_timeout` set with `set_config(..., true)` in the same round trip as the tenant; a per-tenant semaphore in front of the pool | Transaction scope resets them at commit; deadpool has one shared pool | Weak |

## RLS fails through five holes, and each closes from the catalog

**Superusers and `BYPASSRLS` roles always skip RLS, and table owners skip it unless the table has `FORCE ROW LEVEL
SECURITY`** ([PG 18 row security](https://www.postgresql.org/docs/18/ddl-rowsecurity.html)). In a container test, the
owner read all 200,000 rows until `FORCE` was set, then read none with no tenant set. A table with RLS enabled and no
policy denies everything, but policies on a table with RLS disabled are silently ignored
([PG 18 ALTER TABLE](https://www.postgresql.org/docs/18/sql-altertable.html)). A policy with only `USING` reuses it as
`WITH CHECK`, so the chosen `FOR ALL` policy already blocks writing another tenant's ID
([PG 18 row security](https://www.postgresql.org/docs/18/ddl-rowsecurity.html)); a container test confirmed the insert
failed. Blocked updates and deletes affect zero rows with no error, which is why tests must assert row counts, not
just the absence of errors. Permissive policies are OR'd and restrictive ones AND'd, and a policy with no `TO` clause
applies to `PUBLIC` ([PG 18 CREATE POLICY](https://www.postgresql.org/docs/18/sql-createpolicy.html)). A later
"support" or "admin" permissive policy therefore widens every query. A restrictive copy of the tenant rule stops that
([Bytebase](https://www.bytebase.com/blog/postgres-row-level-security-footguns/), vendor).

**Objects that run as their owner see what the owner sees.** A container test showed a `SECURITY DEFINER` function,
owned by the table owner, return both tenants' rows while the app had tenant 1 set. Views apply the view owner's
policies unless created with `security_invoker = true`
([PG 18 CREATE VIEW](https://www.postgresql.org/docs/18/sql-createview.html)). With `FORCE` on the base table, both
kinds of view returned only the current tenant in a container test, because the owner's policies read the same
setting. Materialized views have no RLS at all. A container test showed one built over a non-`FORCE` table hold every
tenant's rows, all readable by the app role. No PG 18 docs page states this, so that finding rests on the container
test alone.

**Whole-table commands ignore RLS.** `TRUNCATE` and `REFERENCES` are not subject to row security
([PG 18 row security](https://www.postgresql.org/docs/18/ddl-rowsecurity.html)), and a container test showed the app
role truncate a table when granted the privilege. `COPY FROM` is refused on RLS tables
([PG 18 COPY](https://www.postgresql.org/docs/18/sql-copy.html)), so bulk loads use `INSERT`. `pg_dump` turns
`row_security` off and fails for a role that cannot bypass it
([PG 18 pg_dump](https://www.postgresql.org/docs/18/app-pgdump.html)). Partitions are the sharpest edge. A query that
names a child table directly uses the child's policies and ignores the parent's
([PG 18 inheritance](https://www.postgresql.org/docs/18/ddl-inherit.html)), and the ALTER TABLE page does not say
whether `ENABLE` or `FORCE` recurse. In a container test they did not: after both on a hash-partitioned parent, the
partition showed both flags false, and the app read its rows directly once granted `SELECT` on it.

**Constraint checks bypass RLS and leak.** Unique, primary key, and foreign key checks "always bypass row security,"
and the docs warn of covert channels ([PG 18 row security](https://www.postgresql.org/docs/18/ddl-rowsecurity.html)).
In container tests, a global `unique(email)` told tenant 1 that another tenant's email existed, though PG 18 hid the
key value from the error detail. A single-column foreign key let tenant 1 insert a child row pointing at tenant 2's
parent row, and its failure on a missing ID worked as an existence oracle. A composite foreign key
`(tenant_id, parent_id)` rejected the cross-tenant link with SQLSTATE 23503. Bytebase claims foreign key checks need
a `SELECT` policy on the parent ([Bytebase](https://www.bytebase.com/blog/postgres-row-level-security-footguns/),
vendor). The PG docs and the container test both contradict that.

**Connection state carries a tenant forward.** Custom settings have no privilege check: in a container test the app
role ran `set_config('app.tenant_id', '2', false)` and read tenant 2's rows. SQL injection therefore becomes tenant
escape, which makes parameterised queries part of the isolation boundary. A measured test behind PgBouncer
transaction pooling found that a session-level tenant setting let the next client, which set no tenant, read 267,023
of another tenant's invoices, while `set_config(..., true)` returned zero rows
([now-next.nl](https://now-next.nl/en/insights/postgresql-row-level-security-multi-tenant-saas/), single-author test).
deadpool-postgres's `RecyclingMethod::Fast` "does not check the connection health or perform any clean-up queries"
([docs.rs](https://docs.rs/deadpool-postgres/latest/deadpool_postgres/enum.RecyclingMethod.html)), so Tetsu's pool
would carry any session `SET` to the next checkout. `SET LOCAL` outside a transaction block only warns and has no
effect ([PG 18 SET](https://www.postgresql.org/docs/18/sql-set.html)), so the tenant must be set inside the same
explicit transaction as the queries. After such a transaction ends, `current_setting('app.tenant_id', true)` returns
an empty string, not NULL, in a container test, which is why the chosen policy needs `nullif`.

The published leaks match these holes. OpenAI's 2023 incident came from a client library that returned one
connection's data to another, exposing chat titles and some subscribers' billing details
([OpenAI](https://openai.com/index/march-20-chatgpt-outage)). CVE-2025-48757 covered Lovable-generated apps with
missing or weak RLS, with about 170 of 1,645 showcased apps exposed
([PT Security](https://dbugs.ptsecurity.com/vulnerability/PT-2025-23236);
[Superblocks](https://www.superblocks.com/blog/lovable-rls-breach), vendor). Small projects show the routine case, a
repository method with no tenant predicate ([StellarPOS #207](https://github.com/StellarPOS-App/backend/issues/207)).
No primary postmortem of a cross-tenant leak in a large B2B pooled Postgres database turned up.

PostgreSQL itself leaks now and then. CVE-2026-14666 let row security caching ignore changes to role membership and
attributes through plan reuse; it is fixed in 18.6
([PostgreSQL](https://www.postgresql.org/support/security/CVE-2026-14666/)). CVE-2025-8713 exposed sampled rows
through planner statistics ([PostgreSQL](https://www.postgresql.org/support/security/CVE-2025-8713/)), and
CVE-2023-39418 let `MERGE` skip update policies on 15
([PostgreSQL](https://www.postgresql.org/support/security/)). The plan-reuse CVEs need role-specific policies.
Tetsu's tenant comes from a setting, not a role, so they reach it only weakly, but the 18.6 floor still applies. In a
container test the app role saw no `pg_stats` rows for an RLS table.

## Tenant-leading keys serve the policy, the foreign keys, and future moves at once

**A composite primary key `(tenant_id, id)` is the foreign key target, the policy index, and the shard key in one.**
In a container test, `select * from t where id = 5` planned as an index scan on that key with the policy as part of
the index condition. Supabase notes that only the first column of a btree counts for a policy filter
([Supabase RLS](https://supabase.com/docs/guides/database/postgres/row-level-security), vendor), so `tenant_id` must
lead. Citus requires the distribution column in any composite key
([Citus](https://docs.citusdata.com/en/stable/admin_guide/cluster_management.html)), and Notion's stated regret after
sharding was not introducing "a combined primary key instead of a separate partition key"
([Notion](https://www.notion.com/blog/sharding-postgres-at-notion), older).

**`current_setting` does not stop index use on PG 18.6, but non-leakproof functions in the query do.** One write-up
says the non-leakproof `current_setting` forces sequential scans
([dev.to](https://dev.to/dvoraj75/postgresql-rls-is-fail-closed-but-is-it-fast-making-django-rls-tenants-index-friendly-2j18),
informal, no measured plans). Container tests contradict it: the policy appeared as an `Index Cond`. The real limit
is different. Policy quals run before non-leakproof user functions
([PG 18 CREATE POLICY](https://www.postgresql.org/docs/18/sql-createpolicy.html)), so a query on `lower(email)`
ignored an index on `lower(email)` and filtered after the tenant scan. Expression indexes therefore take the form
`(tenant_id, lower(email))`, which also gives ADR-0040's per-business customer email rule its constraint.

**Wrap the setting in a sub-select.** Wrapping a function as `(select ...)` lets the planner run it once per
statement as an InitPlan ([Supabase RLS](https://supabase.com/docs/guides/database/postgres/row-level-security),
vendor). In a container test on 2 million rows with no `tenant_id` index, the bare policy took about 126 to 130 ms
and the wrapped one about 33 to 35 ms, roughly 3.8 times faster. The wrapper keeps the NULL default deny. Its effect
on index paths was not measured. Adding `tenant_id = $1` to queries as well let the planner turn the policy into a
one-time filter, and Supabase recommends it
([Supabase RLS performance](https://supabase.com/docs/guides/database/postgres/row-level-security-performance),
vendor).

**Prepared statements stay correct across tenants.** With a forced generic plan, a cached statement kept the
`current_setting` expression in its index condition and returned only the newly set tenant's rows after a switch
(container test). Tetsu's statement cache is safe under this design.

Putting `tenant_id` directly on every table also avoids policies that join to a parent, which Svix notes adds a join
to every query ([Svix](https://www.svix.com/blog/row-level-security-defense-in-depth/)). The cost is 8 bytes per index
entry and two-column foreign keys. No benchmark at Tetsu's scale exists.

## Production teams pair RLS with app filters and test from the catalog

**The teams that publish their reasons split by whether the tenant key also routes to a shard.** Svix, on Rust and
Postgres, keeps app-level `account_id` filters and adds RLS "as just one element of a 'defense in depth' approach"
([Svix](https://www.svix.com/blog/row-level-security-defense-in-depth/)). It sets the account with
`set_config(..., true)` at the start of each transaction, inside a `SecureTransaction` type built by an Axum
extractor from the authenticated request. That is the closest precedent for Tetsu. AWS calls RLS required for the
pool model ([AWS](https://docs.aws.amazon.com/prescriptive-guidance/latest/saas-multitenant-managed-postgresql/best-practices.html))
and prefers one shared role plus a session variable over a role per tenant
([AWS blog](https://aws.amazon.com/blogs/database/multi-tenant-data-isolation-with-postgresql-row-level-security/),
older). Notion, PostHog, Citus users, and Salesforce scope by tenant key in a layer they own
([Notion](https://notion.com/blog/sharding-postgres-at-notion);
[PostHog](https://posthog.com/handbook/engineering/databases/hogql-python);
[Citus](https://docs.citusdata.com/en/v12.1/develop/migration_mt_schema.html);
[Salesforce](https://architect.salesforce.com/fundamentals/platform-multitenant-architecture)). PostHog warns that
raw SQL skips its guard. ClickHouse also treats RLS as a safety net behind app filters
([ClickHouse](https://clickhouse.com/resources/engineering/multi-tenant-saas-postgres-architecture), vendor).
GitLab, moving to cells, requires every new cell-local table to declare a `sharding_key`, checked in CI
([GitLab](https://docs.gitlab.com/development/organization/sharding);
[GitLab MR 136682](https://gitlab.com/gitlab-org/gitlab/-/merge_requests/136682)).

**Isolation tests come in three layers.** First, schema lints: Atlas proposes rules that every table has RLS enabled
and forced, at least one policy, `WITH CHECK` on writes, restrictive tenant policies, and an allowlist of
`BYPASSRLS` roles ([Atlas](https://www.atlasgo.io/guides/rls-policy), vendor). Supabase's linter errors on a table
without RLS ([Supabase lint 0013](https://supabase.github.io/splinter/0013_rls_disabled_in_public), vendor). Second,
policy tests seed data as a privileged role and then switch to the app role, because tests as an owner or superuser
make broken policies look correct ([Atlas](https://www.atlasgo.io/guides/rls-policy), vendor;
[Bytebase](https://www.bytebase.com/blog/postgres-row-level-security-footguns/), vendor). Third, negative tests:
with no tenant set, a query through the pool returns zero rows
([now-next.nl](https://now-next.nl/en/insights/postgresql-row-level-security-multi-tenant-saas/)).

Tetsu can write these as plain SQL tests without adopting a tool. The catalog checks, all designed by the researcher
from the docs above, assert that every table in a module schema either has a `tenant_id` column or sits on a declared
global list; that every tenant table, and every partition, has `relrowsecurity` and `relforcerowsecurity`; that each
has a restrictive policy reading `app.tenant_id`; that every unique index and foreign key on a tenant table includes
`tenant_id`; that every view has `security_invoker=true`; that there are no materialized views and no `SECURITY
DEFINER` functions outside an allowlist; and that the app role is neither superuser nor `BYPASSRLS`, owns nothing,
and holds no `TRUNCATE`, `REFERENCES`, or `TRIGGER`. Behaviour tests then loop over the same table list: as the app
role with tenant A set, reads of tenant B's rows return nothing, updates and deletes affect zero rows, and inserts or
updates that write B fail with SQLSTATE 42501. A pool test sets a tenant at session level on one checkout and
asserts the next checkout sees an empty setting, which proves the lint is needed. The Lee Park case from #8, one
person who is a customer of both reference businesses with the same email, is the natural fixture.

## Per-module roles catch mistakes, not attackers

**`SET LOCAL ROLE` on one pool works, and it guards against bugs only.** `SET LOCAL` ends with the transaction,
committed or not ([PG 18 SET](https://www.postgresql.org/docs/18/sql-set.html)), so it fits `RecyclingMethod::Fast`
and transaction pooling. The plan cache records the role and discards an RLS-dependent plan built under another role
([plancache.c, REL_18_STABLE](https://raw.githubusercontent.com/postgres/postgres/REL_18_STABLE/src/backend/utils/cache/plancache.c)),
so the statement cache stays correct, at the price of replanning when one connection alternates between module roles.
Nobody has measured that price. But `RESET ROLE` "can be executed by any user"
([PG 18 SET ROLE](https://www.postgresql.org/docs/18/sql-set-role.html)), and pgsql-hackers notes that any injection
can then switch to any granted role
([pgsql-hackers](https://www.postgresql.org/message-id/CACA6kxh4MfRCHuY%2BuC2ZvXRQUP63LqumNtxtLsDF-mJswAJR5w%40mail.gmail.com)).
Granting module roles to the login role `WITH INHERIT FALSE, SET TRUE` means a reset lands on a role that can touch
nothing ([PG 18 SET ROLE](https://www.postgresql.org/docs/18/sql-set-role.html)), but an attacker can still choose
another module's role. `SET ROLE` also does not apply a role's `search_path`, so every name must be
schema-qualified.

**One pool per module role holds against injection and costs connections.** Milan Jovanovic's modular-monolith guide
gives each module its own login and connection string
([Milan Jovanovic](https://milanjovanovic.tech/blog/how-to-keep-your-data-boundaries-intact-in-a-modular-monolith),
secondary). On Neon Free at 0.25 CU there are 112 connections with 7 reserved
([Neon pooling](https://neon.com/docs/connect/connection-pooling)), so five modules with eight connections each would
take 40 of the 105 usable. Through Neon's pooler each role also gets its own server-side pool. Since injection is
already tenant escape through `set_config`, a per-module pool would not close the bigger hole. The middle path is one
pool per trust class, the request path and the operator path, with `SET LOCAL ROLE` per module inside the request
pool.

**Grants follow the owner, so one role must create every table.** `ALTER DEFAULT PRIVILEGES` applies only to objects
the named role creates later ([PG 18](https://www.postgresql.org/docs/18/sql-alterdefaultprivileges.html);
[Neon roles](https://neon.com/docs/manage/roles)). If a migration ever runs as a different role, new tables silently
miss their grants. Functions get `EXECUTE` for `PUBLIC` by default, and databases give `PUBLIC` the `TEMPORARY`
privilege ([PG 18 privileges](https://www.postgresql.org/docs/18/ddl-priv.html)); revoke both. A CI check that
compares `information_schema.role_table_grants` with the expected matrix catches drift.

## Guarded lookups handle tables read before or across tenants

**Resolve the tenant in its own short transaction, through a function that answers one question.** Before a tenant
is known, the app must map a host to a tenant. A plain grant on the routing table lets any code, or any injection,
list every host and tenant. A `SECURITY DEFINER` function such as `tenancy.resolve_host(text)` returns one answer per
input and cannot be used to enumerate. The PG docs set its rules: fix `search_path` with `pg_temp` last, and revoke
`EXECUTE` from `PUBLIC` in the same transaction that creates it
([PG 18 CREATE FUNCTION](https://www.postgresql.org/docs/18/sql-createfunction.html)). Supabase puts such helpers
in a non-exposed schema with `search_path = ''`
([Supabase RLS](https://supabase.com/docs/guides/database/postgres/row-level-security), vendor). The owner should be a
dedicated `NOLOGIN` role that owns only that table, not a Neon console role. A policy keyed on another setting, such
as `app.host`, only narrows accidental reads, because the caller can set any value. No benchmark compares these
options.

**Staff span tenants by design, and customers do not.** ADR-0040 already makes a staff member "one identity with a
staff role at each business," and a customer account belongs to one business
([ADR-0040](../adr/0040-identity-and-sign-in.md)). Customer accounts are therefore ordinary tenant rows. Staff
identities fit Nile's published shape: a global users table with no `tenant_id`, and a membership table keyed
`(tenant_id, user_id)` ([Nile](https://thenile.dev/docs/auth/concepts/builtintables.md), vendor). The staff role
table sits under tenant RLS. The app checks the role for the requested business before it sets `app.tenant_id`. A
"my businesses" list crosses tenants on purpose, so it goes through a definer function keyed on an `app.account_id`
setting that only trusted code sets after sign-in. Supabase uses the same device to stop policies that read each
other from looping, error 42P17 ([Supabase RLS](https://supabase.com/docs/guides/database/postgres/row-level-security),
vendor). Policies that read other tables can race under `READ COMMITTED`, so a removed role may still pass in a
transaction already running ([PG 18 row security](https://www.postgresql.org/docs/18/ddl-rowsecurity.html)); short
transactions shrink that window.

**Operators get their own login role and pool, never the request path.** Whether operators may see inside a business
is #24's decision ([ADR-0040](../adr/0040-identity-and-sign-in.md)). Mechanically, either policies `TO
operator_role` or the `BYPASSRLS` attribute would serve. Supabase's `service_role` has `bypassrls` and must never
reach the browser ([Supabase RLS](https://supabase.com/docs/guides/database/postgres/row-level-security), vendor).
Policies `TO operator_role` are narrower and auditable, and they do not depend on whether Neon lets a non-superuser
grant `BYPASSRLS`, which is unconfirmed.

## Neon hands every console role the power to skip RLS

**Roles created in the console, API, or CLI join `neon_superuser`, which reads and writes all data and, in projects
created after 2023-08-15, has `BYPASSRLS`** ([Neon roles](https://neon.com/docs/manage/roles);
[Neon changelog](https://neon.com/docs/changelog/2023-08-15)). Neon's own guide says admin connection strings such as
`neondb_owner` "ignore RLS policies entirely" and recommends a separate login role without `BYPASSRLS` for the app
([Neon RLS](https://neon.com/docs/guides/rls-query-execution)). Roles created in SQL get only basic `public`
privileges ([Neon roles](https://neon.com/docs/manage/roles)), which suits least privilege. `FORCE` does not help
against `BYPASSRLS`. The fix is for tables to be owned by a SQL-created owner role that lacks the attribute, so
`FORCE` binds it, and for migrations to run as that role. Whether that works end to end on Neon is a check still to
run.

**The pooler and the free plan shape the connection budget.** Neon's pooler is PgBouncer in transaction mode, where
session `SET` and `RESET` are unsupported and protocol-level prepared statements work, up to 1,000 per connection
([Neon pooling](https://neon.com/docs/connect/connection-pooling)). Transaction-scoped settings fit that mode. Since
deadpool already pools, a direct connection with pool sizes summing below 105 is the simpler setup at 0.25 CU. The
Free plan autoscales to 2 CU, scales to zero after 5 minutes, and keeps 6 hours of history
([Neon plans](https://neon.com/docs/introduction/plans);
[Neon history window](https://neon.com/docs/postgres/backup-restore/history-window)). Roles copy to child branches,
so preview branches inherit the grant setup ([Neon roles](https://neon.com/docs/manage/roles)).

## One copy tool serves single-tenant restore and moves to a cell

**No managed host restores one tenant, Neon included.** Neon's instant restore "applies to _all_ databases on the
branch" and is "a _complete_ overwrite" ([Neon restore](https://neon.com/docs/guides/branch-restore)). AWS's method
for pooled databases restores a whole snapshot to a side copy and loads one tenant back, and AWS says "We prefer
segregation during recovery." Its example loops over a hard-coded table list and warns that the list must track the
schema ([AWS blog](https://aws.amazon.com/blogs/database/managed-database-backup-and-recovery-in-a-multi-tenant-saas-application)).
On Neon the path is to branch at a time before the damage, check it with Time Travel, then copy one tenant's rows
back with `WHERE tenant_id = $1` in foreign key order inside one transaction. The table list comes from the catalog
and is checked against a declared list, so a new table cannot be skipped. On Free, a mistake older than 6 hours cannot
be recovered this way. Neon's restore also leaves object storage untouched, so files and rows can diverge
([Neon restore](https://neon.com/docs/guides/branch-restore)).

**Every published tenant move has the same shape.** Shopify attaches `shop_id` to every shop-owned table, copies one
shop's rows, tails the binlog for that shop, takes a short write lock, and flips a routing table that lives in a
separate unsharded database ([Shopify](https://shopify.engineering/mysql-database-shard-balancing-terabyte-scale)).
Stripe's switch takes under two seconds
([Stripe](https://stripe.dev/blog/how-stripes-document-databases-supported-99.999-uptime-with-zero-downtime-data-migrations)).
Citus isolates a tenant to its own shard with logical replication and a brief metadata lock
([Citus](https://docs.citusdata.com/en/stable/admin_guide/cluster_management.html)). AWS names a tenant that needs "a
dedicated cell" as a reason to migrate, in four phases: clone, flip, redirect, forget
([AWS cells](https://docs.aws.amazon.com/wellarchitected/latest/reducing-scope-of-impact-with-cell-based-architecture/cell-migration.html)).
At Tetsu's size, a few minutes of read-only time, a copy, and a route flip are enough. A restore and a move are the
same filtered copy, so one tool serves both.

**Row IDs decide whether that copy is verbatim.** Per-database `bigint` sequences collide when a tenant lands in a
cell that already has rows. PostgreSQL 18 adds native `uuidv7()`
([PG 18 UUID functions](https://www.postgresql.org/docs/18/functions-uuid.html)). Salesforce pairs a GUID with OrgID
and enforces uniqueness on both
([Salesforce](https://architect.salesforce.com/fundamentals/platform-multitenant-architecture)), which is the shape
recommended here. Figma kept auto-increment IDs, routed on hashes, and listed globally unique IDs as future work
([Figma](https://www.figma.com/blog/how-figmas-databases-team-lived-to-tell-the-scale/)). The size cost looks modest:
a 50 million row test on PG 18 found a 1,504 MB UUIDv7 index against 1,981 MB for UUIDv4
([credativ](https://www.credativ.de/en/blog/postgresql-en/a-deeper-look-at-old-uuidv4-vs-new-uuidv7-in-postgresql-18/),
vendor), and an informal test put UUIDv7 inserts within about 15% of `bigint`
([stevenpg.com](https://stevenpg.com/posts/uuidv7-in-spring-boot-and-postgres/), informal). No rigorous study
compares `bigint` and UUIDv7 at Tetsu's size. `tenant_id` itself can stay `bigint`, because tenants are created in
the global registry and their IDs never come from a per-cell sequence. The alternative, `bigint` row IDs with
disjoint ranges per cell, keeps 8-byte keys but is easy to get wrong. Changing later means rewriting every key, so
the choice is cheapest now.

The hard parts of a move are rows that belong to no single tenant: global identities, the routing table, plan and
billing catalogs, queues, caches, and file paths. Classify each table as tenant-owned, global, or forbidden. Put
`tenant_id` on every job, key caches by tenant and rebuild them after a move, and prefix files with the tenant ID.

## Per-tenant limits ride in the same transaction as the tenant

**Set timeouts with the tenant, and cap concurrency in the app.** `set_config` "provides equivalent functionality"
to `SET` ([PG 18 SET](https://www.postgresql.org/docs/18/sql-set.html)), so `statement_timeout` and `lock_timeout`
can be set with `set_config(..., true)` in the same round trip as `app.tenant_id` and reset at commit. Under a
transaction pooler, session-level timeouts can land on another client's connection
([Lightrun](https://lightrun.com/answers/coleifer-peewee-set-statement_timeout-per-connection-or-per-transaction),
secondary). Neon suggests timeouts by tier, such as 60 s and 15 s, per-tenant connection caps, and tenant-aware rate
limiting that queues or asks clients to retry later, though its main advice is a database per tenant
([Neon blog](https://neon.com/blog/noisy-neighbor-multitenant), vendor). AWS says noisy neighbours cannot be fully
removed in the pool model
([AWS](https://docs.aws.amazon.com/prescriptive-guidance/latest/saas-multitenant-managed-postgresql/best-practices.html)).
deadpool has one pool, so a per-tenant cap is a semaphore per tenant in front of it. Tagging each transaction with
its tenant in logs supports per-tenant monitoring of the kind Citus built in Landlord
([Citus](https://www.citusdata.com/blog/2018/07/31/introducing-landlord-per-tenant-stats/), older). No named company
has published a per-tenant `SET LOCAL statement_timeout` implementation, so this section is the weakest.

## Conclusion

The research moves the question from "does RLS isolate tenants" to "can Tetsu prove, on every commit, that no path
skips it." The policy text is the easy part. The proof is a set of catalog queries plus tests run as the app role,
and that set doubles as the enumeration a restore or a cell move needs. One list of tenant tables, checked in CI,
serves isolation, restore, and moves at once, which is the lesson GitLab and AWS reached from opposite ends.

The other shift is that the riskiest roles on Neon are the ones Neon hands out by default. Isolation therefore
depends as much on who owns and runs things as on the policies. The decisions that are cheap now and costly later
are the composite keys and the row ID type, because no copy tool can repair data that lacks a tenant key or IDs that
collide.

## Gaps and checks still to run

- Confirm Neon's PostgreSQL 18 minor version is 18.6 or later.
- Check whether a `neon_superuser` member can grant `BYPASSRLS` to a SQL-created role.
- Run the hosted check of the app role on the demo's Neon project (hosted test 8, now in #8): not superuser, not
  `BYPASSRLS`, owns nothing, and sees no rows with no tenant set.
- Check that a SQL-created owner role can own all tables and run migrations on Neon, so `FORCE` binds it.
- Measure the `(select ...)` wrapper on index paths, and replanning cost when one connection alternates module roles.
- Confirm `SET LOCAL ROLE` passes through Neon's pooler unchanged, if the pooler is ever used.
- Source trigger and rule behaviour under RLS on PG 18, and whether `ATTACH PARTITION` copies policies.
- Check whether Neon supports row-filtered logical replication as a source for tenant moves.
- Check whether Neon sets `max_connections` from the minimum or maximum of the autoscaling range.
- Benchmark `bigint` against UUIDv7 keys at Tetsu's expected size, and confirm `uuidv7()` ordering within a session.
- Read the English AWS RLS pages, which returned 404; the research fell back on the Spanish edition and search snippets.
- Find a primary postmortem of a cross-tenant leak in a large B2B pooled Postgres database; none turned up.
