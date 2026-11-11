# PostgreSQL RLS failure modes for pooled multi-tenancy

Scope: PostgreSQL 18 (Neon), pooled shared tables, `tenant_id bigint`, policy
`tenant_id = nullif(current_setting('app.tenant_id', true), '')::bigint`, tenant set per transaction with
`set_config('app.tenant_id', $1, true)`, migrations as table owner, app as non-owner role.

Evidence key. "Probe" means I ran it on a throwaway `postgres:18` Docker container that reported
`PostgreSQL 18.6 (Debian 18.6-1.pgdg13+2)` on 2026-11-11. Probe scripts are in
[probes/](probes/). Probe results are first-hand observations, not published sources, and the container
has been stopped.

## Bypasses: owners, superusers, BYPASSRLS, FORCE, command coverage, permissive vs restrictive, default deny

**Takeaway:**

Superusers and BYPASSRLS roles always skip RLS, and owners skip it unless the table has FORCE. Every policy must cover
reads and writes, since `FOR ALL` with only `USING` reuses it as `WITH CHECK`. The app role must own nothing and hold
neither attribute. Use permissive policies for the tenant rule, and treat any extra permissive policy as a widening.

**Cited Findings:**

- "Superusers and roles with the BYPASSRLS attribute always bypass the row security system when accessing a table. Table owners normally bypass row security as well, though a table owner can choose to be subject to row security with ALTER TABLE ... FORCE ROW LEVEL SECURITY." (PG 18) [PG18 docs 5.9](https://www.postgresql.org/docs/18/ddl-rowsecurity.html)
- FORCE "control[s] the application of row security policies belonging to the table when the user is the table owner ... If disabled (the default) then row-level security will not be applied when the user is the table owner." [ALTER TABLE PG18](https://www.postgresql.org/docs/18/sql-altertable.html)
- Probe: as owner without FORCE, `select count(*)` returned all 200000 rows. After `FORCE ROW LEVEL SECURITY` with no tenant set, the owner saw 0 rows. (Probe, PG 18.6.)
- Default deny: "If no policy exists for the table, a default-deny policy is used, meaning that no rows are visible or can be modified." [PG18 docs 5.9](https://www.postgresql.org/docs/18/ddl-rowsecurity.html)
- Policies on a table with RLS disabled are silently ignored: "policies can exist for a table even if row-level security is disabled. In this case, the policies will not be applied." [ALTER TABLE PG18](https://www.postgresql.org/docs/18/sql-altertable.html)
- A policy written with only `USING` "implicitly provides a WITH CHECK clause identical to its USING clause", so rows for another owner cannot be created by INSERT or UPDATE. [PG18 docs 5.9](https://www.postgresql.org/docs/18/ddl-rowsecurity.html)
- Probe: with tenant 1 set, `insert ... values (2, ...)` failed with `new row violates row-level security policy for table "t"`. (Probe, PG 18.6, `FOR ALL` policy with only USING.)
- Per command matrix (PG 18): INSERT checks the new row against INSERT/ALL WITH CHECK; UPDATE filters by UPDATE USING and checks the new row by UPDATE WITH CHECK; DELETE filters by DELETE USING; `INSERT ... ON CONFLICT DO UPDATE` checks existing and new rows; `MERGE` has no policy of its own and applies SELECT, INSERT, UPDATE, DELETE policies per action. [CREATE POLICY PG18](https://www.postgresql.org/docs/18/sql-createpolicy.html)
- UPDATE and DELETE that read columns (WHERE, RETURNING, SET expressions) also apply SELECT policies. RETURNING rows that fail SELECT policies raise an error, never a silent drop. [CREATE POLICY PG18](https://www.postgresql.org/docs/18/sql-createpolicy.html)
- Blocked UPDATE or DELETE affects 0 rows with no error, for example `UPDATE 0`. [PG18 docs 5.9](https://www.postgresql.org/docs/18/ddl-rowsecurity.html); [Bytebase footguns](https://www.bytebase.com/blog/postgres-row-level-security-footguns/)
- Combination: permissive policies are OR'd; restrictive policies are AND'd. "there must be at least one PERMISSIVE policy granting access ... If there are no PERMISSIVE policies, then access is denied." [CREATE POLICY PG18](https://www.postgresql.org/docs/18/sql-createpolicy.html)
- An extra permissive policy (for example an admin or support policy) widens access because it is OR'd. Fix: put the tenant rule in a `RESTRICTIVE` policy or keep one policy. [Bytebase footguns](https://www.bytebase.com/blog/postgres-row-level-security-footguns/)
- The `TO` clause defaults to `PUBLIC`, so a policy applies to all roles unless narrowed. [CREATE POLICY PG18](https://www.postgresql.org/docs/18/sql-createpolicy.html)
- Only the owner can enable RLS or create policies. [PG18 docs 5.9](https://www.postgresql.org/docs/18/ddl-rowsecurity.html)
- `row_security = off` "does not in itself bypass row security; what it does is throw an error if any query's results would get filtered by a policy." [PG18 docs 5.9](https://www.postgresql.org/docs/18/ddl-rowsecurity.html)
- Probe: the app role running `set row_security = off; select count(*) from m.t` got `ERROR: query would be affected by row-level security policy for table "t"`. (Probe, PG 18.6.)
- Testing as a superuser or owner gives false passes. Test as the real app role. [Bytebase footguns](https://www.bytebase.com/blog/postgres-row-level-security-footguns/)

**Inferences:**

- Fix set for this design: `ENABLE` and `FORCE ROW LEVEL SECURITY` on every tenant-owned table; app role `NOSUPERUSER NOBYPASSRLS`, owns no tables, schemas, views, or functions; migrations as a separate owner role.
- Keep `FORCE` even though the app is not the owner. It catches migrations, backfills, and owner-run jobs that would otherwise touch every tenant.
- A permissive tenant policy plus a restrictive tenant policy is defence in depth. A later permissive "admin" policy then cannot widen past the restrictive tenant check.
- Catalogue tests to write, each run as the app role: (1) query `pg_class` for every table in tenant schemas with a `tenant_id` column and assert `relrowsecurity and relforcerowsecurity`; (2) assert each such table has a policy (`pg_policies`) whose expression mentions `app.tenant_id`, and that no permissive policy lacks it; (3) assert the app role has `rolbypassrls = false`, `rolsuper = false`, and owns nothing (`pg_class.relowner`, `pg_namespace.nspowner`, `pg_proc.proowner`); (4) behaviour tests per table: with tenant A set, SELECT, UPDATE, DELETE of a tenant B row affect 0 rows, INSERT and UPDATE that set `tenant_id = B` fail with SQLSTATE 42501; (5) with no tenant set, every table returns 0 rows.

**Gaps:**

- No primary source gives a canned catalogue query for "policy missing for a command". The tests above are my design.

## Objects that run with someone else's rights: SECURITY DEFINER, views, materialized views, triggers, rules

**Takeaway:**

Anything that runs as the owner (SECURITY DEFINER functions, default views, materialized views) sees what the owner sees.
FORCE on the base table makes the owner subject to policies, which closes most of this. Materialized views have no RLS at all.

**Cited Findings:**

- Policy expressions run with the querying user's privileges, "although security-definer functions can be used to access data not available to the calling user." [PG18 docs 5.9](https://www.postgresql.org/docs/18/ddl-rowsecurity.html)
- Probe: a `SECURITY DEFINER` SQL function owned by the table owner, on a table with RLS but not FORCE, returned the count of both tenants' rows (2) while the app had tenant 1 set. (Probe, PG 18.6.)
- Views by default: "the row-level security policies of the view owner are applied, and access to any additional relations referred to by those policies is determined by the permissions of the view owner. However, if the view has security_invoker set to true, then the policies and permissions of the invoking user are used instead." [CREATE VIEW PG18](https://www.postgresql.org/docs/18/sql-createview.html)
- `security_invoker` is available from Postgres 15. On older versions, revoke access or hide views in an unexposed schema. [Supabase RLS docs](https://supabase.com/docs/guides/database/postgres/row-level-security)
- A `security_invoker = false` view "is not equivalent to a SECURITY DEFINER function"; functions in a view still run as invoker or definer per their own definition. [CREATE VIEW PG18](https://www.postgresql.org/docs/18/sql-createview.html)
- Probe: with FORCE on the base table, both a default view and a `security_invoker` view owned by the table owner returned only the current tenant's 400 rows, because the owner's policies use the same `current_setting`. (Probe, PG 18.6.)
- Probe: a materialized view built by the owner over a non-FORCE RLS table held all tenants' rows, and the app role read all of them (2 rows across 2 tenants). A materialized view built while FORCE was on and no tenant set held 0 rows. (Probe, PG 18.6.)
- Data copied out of a protected table (materialized views, exports, jobs) is not covered by its policy. [Bytebase footguns](https://www.bytebase.com/blog/postgres-row-level-security-footguns/)
- Before PG 15 workaround for definer functions: check `row_security_active()` and raise. [Bytebase footguns](https://www.bytebase.com/blog/postgres-row-level-security-footguns/)
- SECURITY DEFINER functions should set `search_path = ''` and schema-qualify every name. [Supabase RLS docs](https://supabase.com/docs/guides/database/postgres/row-level-security)
- Policy expressions are applied before user quals so untrusted functions cannot see filtered rows, except `LEAKPROOF` functions, which may run first. [CREATE POLICY PG18](https://www.postgresql.org/docs/18/sql-createpolicy.html)

**Inferences:**

- Always create views `WITH (security_invoker = true)` in tenant schemas, and assert it in a catalogue test (`pg_class.reloptions` contains `security_invoker=true` for every view).
- Ban materialized views over tenant data, or include `tenant_id` and expose them only through a security_invoker view over an RLS table. A test can assert no `relkind = 'm'` in tenant schemas, or that the app has no SELECT grant on them.
- Ban SECURITY DEFINER functions in tenant schemas unless reviewed. Catalogue test: `pg_proc.prosecdef` is false for functions in tenant schemas, or listed in an allowlist.
- Triggers and rules: trigger functions run as the caller unless SECURITY DEFINER, so RLS applies inside them. I did not find a primary source stating this for PG 18, so treat it as unverified (see Gaps).

**Gaps:**

- I did not fetch the PG18 trigger or rules pages, so the trigger and rule behaviour under RLS is not sourced here.
- No primary statement found on materialized views and RLS in PG18 docs; the finding rests on the probe.

## Whole-table operations and other paths: TRUNCATE, COPY, pg_dump, sequences, partitions, inheritance, logical replication

**Takeaway:**

TRUNCATE ignores RLS, so never grant it. COPY FROM is refused on RLS tables. pg_dump dumps everything or fails.
Partitions are the sharpest edge: ENABLE and FORCE on the parent do not reach partitions, and a query naming a partition
directly ignores the parent's policies.

**Cited Findings:**

- "Operations that apply to the whole table, such as TRUNCATE and REFERENCES, are not subject to row security." [PG18 docs 5.9](https://www.postgresql.org/docs/18/ddl-rowsecurity.html)
- Probe: the app role with tenant 1 set and a TRUNCATE grant ran `TRUNCATE m.c` successfully. (Probe, PG 18.6.)
- COPY: "the relevant SELECT policies will apply to COPY table TO statements. Currently, COPY FROM is not supported for tables with row-level security. Use equivalent INSERT statements instead." [COPY PG18](https://www.postgresql.org/docs/18/sql-copy.html)
- pg_dump: "By default, pg_dump will set row_security to off, to ensure that all data is dumped from the table. If the user does not have sufficient privileges to bypass row security, then an error is thrown." `--enable-row-security` dumps only visible rows and needs INSERT format because COPY FROM does not support row security. [pg_dump PG18](https://www.postgresql.org/docs/18/app-pgdump.html)
- Inheritance: "the parent table's row security policies ... are applied to rows coming from child tables during an inherited query. A child table's policies, if any, are applied only when it is the table explicitly named in the query; and in that case, any policies attached to its parent(s) are ignored." Permissions are checked on the parent only for inherited queries, but direct child access needs its own grant. [Inheritance PG18](https://www.postgresql.org/docs/18/ddl-inherit.html)
- ALTER TABLE docs do not say whether ENABLE or FORCE recurse to partitions. [ALTER TABLE PG18](https://www.postgresql.org/docs/18/sql-altertable.html)
- Probe: after `ENABLE` then `FORCE ROW LEVEL SECURITY` on a hash-partitioned parent, `pg_class` showed `relrowsecurity = t, relforcerowsecurity = t` on the parent and `f, f` on partition `pt0`. With no tenant set, the app read 0 rows via the parent and 2 rows via `SELECT ... FROM m.pt0` after being granted SELECT on the partition. (Probe, PG 18.6.)
- RLS support for declarative partitioned tables was added in 2017 by applying policies to the partitioned relkind in `fireRIRrules()`. [pgsql-hackers commit thread](https://www.postgresql.org/message-id/E1dK59q-0005XI-2U%40gemulon.postgresql.org)
- Probe: the app role got `permission denied for sequence t_id_seq` when reading an identity sequence it had no grant on. (Probe, PG 18.6.)
- Logical replication: if the replication role lacks SUPERUSER and BYPASSRLS, publisher row security policies can execute; use `options=-crow_security=off` so a new policy halts replication rather than filtering. The apply worker switches to the table owner for writes. [Logical replication security PG18](https://www.postgresql.org/docs/18/logical-replication-security.html)

**Inferences:**

- Never grant TRUNCATE (or REFERENCES, TRIGGER) to the app role. Catalogue test: `has_table_privilege('app', oid, 'TRUNCATE')` is false for every tenant table.
- Bulk loads by the app must use INSERT (or `INSERT ... SELECT FROM unnest(...)`), not COPY FROM.
- Backups must run as an owner or BYPASSRLS role, never the app role. On Neon, backups are platform-level, so this mainly affects ad hoc pg_dump.
- Partitioned tables: enable and force RLS on each partition too, add the same policy to each partition, and grant the app role nothing on partitions. Run these in the migration that creates a partition. Test: every `relispartition` table in tenant schemas has `relrowsecurity and relforcerowsecurity` and a tenant policy, and the app has no direct privilege on it.
- Shared identity sequences across tenants leak global insert volume through id gaps the tenant sees. This is low risk; per-tenant counters or UUIDs remove it. Not sourced; my inference.

**Gaps:**

- No primary source on whether `ATTACH PARTITION` copies policies; probe shows partitions created with `PARTITION OF` did not inherit the RLS flags.
- Logical replication subscriber-side RLS behaviour is not documented on the security page.

## Side channels: unique and FK errors, leakproof functions, error messages, EXPLAIN, statistics

**Takeaway:**

Constraint checks ignore RLS, so a global unique index or a single-column foreign key across tenants is an existence
oracle and, for FKs, a cross-tenant reference. Scope every unique constraint and FK by `tenant_id`. PG 18 hides key
values in error details on RLS tables and hides `pg_stats` rows while RLS is active.

**Cited Findings:**

- "Referential integrity checks, such as unique or primary key constraints and foreign key references, always bypass row security to ensure that data integrity is maintained. Care must be taken ... to avoid 'covert channel' leaks." [PG18 docs 5.9](https://www.postgresql.org/docs/18/ddl-rowsecurity.html)
- Inserting a duplicate into a unique column reveals the value exists; inserting into a referencing table reveals the referenced value exists. Fix by restricting such writes or "using generated values (e.g., surrogate keys) instead of keys with external meanings." [CREATE POLICY PG18](https://www.postgresql.org/docs/18/sql-createpolicy.html)
- Fix for global unique index leakage: scope the index, for example `(tenant_id, lower(email))`. [Bytebase footguns](https://www.bytebase.com/blog/postgres-row-level-security-footguns/)
- Probe: with a global `unique(email)`, tenant 1 inserting another tenant's email got `duplicate key value violates unique constraint "t_email_key"`. Even with `VERBOSITY verbose`, no `DETAIL: Key (email)=...` line appeared for the app role. (Probe, PG 18.6.)
- Probe: a single-column FK `sc.s_id references s(id)` let tenant 1 insert a child row pointing at tenant 2's parent row. A missing id failed with `Key is not present in table "s"`, so success versus failure is an existence oracle. (Probe, PG 18.6.)
- Probe: a composite FK `(tenant_id, parent_id) references t(tenant_id, id)` rejected tenant 1 pointing at a tenant 2 row with SQLSTATE 23503. (Probe, PG 18.6.)
- Bytebase says FK checks need a SELECT policy on the parent table. [Bytebase footguns](https://www.bytebase.com/blog/postgres-row-level-security-footguns/); contradicted by [PG18 docs 5.9](https://www.postgresql.org/docs/18/ddl-rowsecurity.html), which says FK checks always bypass row security, and by the probe above.
- Race conditions in policies that look up other tables can leak; mitigations include `SELECT ... FOR SHARE` in policy sub-selects. [PG18 docs 5.9](https://www.postgresql.org/docs/18/ddl-rowsecurity.html)
- LEAKPROOF functions and operators "may be evaluated before policy expressions". [CREATE POLICY PG18](https://www.postgresql.org/docs/18/sql-createpolicy.html)
- Probe: in `pg_proc`, `int8eq` and `texteq` are leakproof; `current_setting`, `lower`, and `textlike` are not. (Probe, PG 18.6.)
- Timing can reveal hidden rows; there is no single clean fix. [Bytebase footguns](https://www.bytebase.com/blog/postgres-row-level-security-footguns/)
- Probe: the app role, with SELECT on an RLS table, saw 0 rows for that table in `pg_stats`. (Probe, PG 18.6.)
- CVE-2025-8713: planner statistics sampled from a view, partition, or child table could be exposed through a crafted leaky operator, bypassing view ACLs and RLS. Fixed in 17.6, 16.10, 15.14, 14.19, 13.22 on 2025-08-14. It names CVE-2017-7484 and CVE-2019-10130 as earlier fixes for the same class. [CVE-2025-8713](https://www.postgresql.org/support/security/CVE-2025-8713/)
- CVE-2024-4317 restricted `pg_stats_ext` and `pg_stats_ext_exprs` to the table owner (fixed 16.3, 15.7, 14.12). [PG security page](https://www.postgresql.org/support/security/)
- Probe: `EXPLAIN` as the app role shows the policy expression (for example `Index Cond: ((tenant_id = (NULLIF(current_setting('app.tenant_id'...`) but no other tenant's values. (Probe, PG 18.6.)

**Inferences:**

- Rule: every UNIQUE and PRIMARY KEY on a tenant table leads with `tenant_id`, except truly global identifiers that are random (UUIDs), where existence is not meaningful. Catalogue test: for each unique index on a tenant table, the first key column is `tenant_id`.
- Rule: every FK between tenant tables is composite `(tenant_id, x) references parent(tenant_id, id)`. This blocks cross-tenant references, the existence oracle, and cascades that cross tenants. Catalogue test: every `pg_constraint` with `contype = 'f'` between tenant tables includes the `tenant_id` column on both sides.
- I observed cross-tenant references with a single-column FK but did not check a cascade delete in the probe. Since RI actions bypass RLS per the docs, a cascade from tenant B's parent would delete tenant A's child rows that point at it. Composite FKs prevent that state from existing.
- Do not create user-defined LEAKPROOF functions; only superusers can, and on Neon the app cannot anyway. Unverified for Neon.

**Gaps:**

- I did not test whether the SQLSTATE or constraint name in errors leaks schema detail across tenants; constraint names are schema-level, not tenant data.
- No source quantifies the timing side channel for this design.

## Composite keys and index design

**Takeaway:**

Use `PRIMARY KEY (tenant_id, id)`, composite FKs, and indexes that lead with `tenant_id`. The policy's
`current_setting` expression is usable as an index condition, so these indexes serve the policy and the app's own
filters together.

**Cited Findings:**

- "A column counts as indexed only when it comes first in a btree index. A composite primary key indexes only its first column." Add an index on every column the policy filters on. [Supabase RLS docs](https://supabase.com/docs/guides/database/postgres/row-level-security)
- Probe: with `PRIMARY KEY (tenant_id, id)`, `select * from m.t where id = 5` planned as `Index Scan using t_pkey` with `Index Cond: ((tenant_id = (NULLIF(current_setting(...))::bigint) AND (id = 5))`. (Probe, PG 18.6.)
- Probe: a user qual `lower(email) = 'u1@x'` did not use an index on `lower(email)`; the plan used the tenant index and applied `lower(email)` as a Filter. `email = 'u1@x'` (leakproof `texteq`) used the global `t_email_key` index with the policy as a Filter. (Probe, PG 18.6.)
- One write-up claims `current_setting()` not being leakproof stops the policy from becoming an index condition and forces a sequential scan; it gives illustrative, not measured, plans and no version. [dev.to django-rls-tenants](https://dev.to/dvoraj75/postgresql-rls-is-fail-closed-but-is-it-fast-making-django-rls-tenants-index-friendly-2j18); contradicted by the probe above on PG 18.6, where the policy was an `Index Cond`.
- Bytebase says a non-leakproof function in a policy can force a full scan. [Bytebase footguns](https://www.bytebase.com/blog/postgres-row-level-security-footguns/); the probe shows this applies to non-leakproof functions in the user's query (like `lower`), not to `current_setting` in the policy.
- Adding the tenant filter explicitly in the app query helps the planner, even though it duplicates the policy. [Supabase RLS performance](https://supabase.com/docs/guides/database/postgres/row-level-security-performance)

**Inferences:**

- Leading the PK with `tenant_id` makes the PK both the composite FK target and the policy index. It costs 8 bytes per index entry.
- Indexes on expressions with non-leakproof functions (like `lower(email)`) are not usable under RLS unless `tenant_id` leads. Write them as `(tenant_id, lower(email))` so the tenant prefix carries the index.
- A separate single-column `UNIQUE (id)` is not needed when ids come from an identity sequence; uniqueness of `(tenant_id, id)` is enough for FKs.

**Gaps:**

- No measured benchmark of index size or write cost for tenant-leading PKs at Tetsu's scale.

## Performance: current_setting cost, initplan wrapping, plan caching, PG 16 to 18

**Takeaway:**

On index paths the policy costs little. On sequential or wide scans, `current_setting` runs per row; wrapping it as
`(select ...)` makes it an InitPlan evaluated once, which ran about 3.8 times faster in a 2M row probe. Generic plans keep
the policy expression and re-evaluate it at run time, so prepared statement caching is safe for tenant switching.

**Cited Findings:**

- Wrapping a function like `auth.uid()` as `(select auth.uid())` lets the optimizer run it as an initPlan and cache the result per statement, but only when the result does not depend on row data. [Supabase RLS docs](https://supabase.com/docs/guides/database/postgres/row-level-security)
- Supabase's performance page shows only one unindexed diagnostic run (171.033 ms, sequential scan, 100000 rows removed) and no before/after benchmark. [Supabase RLS performance](https://supabase.com/docs/guides/database/postgres/row-level-security-performance)
- Probe: 2M rows, 10 tenants, no index on `tenant_id`, parallel off, warm cache, `count(*)` for one tenant: bare policy 125.7 to 129.5 ms; `(select nullif(current_setting(...), '')::bigint)` policy 33.3 to 34.6 ms over three runs each. The wrapped plan shows `InitPlan 1` and `Filter: (tenant_id = (InitPlan 1).col1)`. (Probe, PG 18.6.)
- Probe: with an explicit `WHERE tenant_id = 3`, the planner hoisted the policy to a `One-Time Filter` on the constant comparison, and the scan filter was the cheap `tenant_id = 3`. (Probe, PG 18.6.)
- Probe: with `plan_cache_mode = force_generic_plan`, a prepared `select * from m.t where id = $1` kept `tenant_id = (NULLIF(current_setting(...)))::bigint` in the Index Cond. Changing `app.tenant_id` to 4 and executing the cached plan returned only tenant 4's view (0 rows for id 4, owned by another tenant). (Probe, PG 18.6.)
- Probe: after a transaction that used `set_config(..., true)` ends, `current_setting('app.tenant_id', true)` returns `''`, not NULL, which is why the `nullif` is needed. (Probe, PG 18.6.)
- Probe: the app role can call `set_config('app.tenant_id', '2', false)` and then read tenant 2's rows. Custom GUCs have no privilege check. (Probe, PG 18.6.)
- Clients must not be able to set `app.*` variables. [Bytebase footguns](https://www.bytebase.com/blog/postgres-row-level-security-footguns/)
- CVE-2026-14666: row security caching ignores changes to role membership, role attributes, and database ownership through plan reuse; stale policies persist until invalidation or disconnect. Fixed in 18.6, 17.11, 16.15, 15.19, 14.24 on 2026-08-13. CVSS 4.2. [CVE-2026-14666](https://www.postgresql.org/support/security/CVE-2026-14666/)

**Inferences:**

- Use the wrapped form in every policy: `tenant_id = (select nullif(current_setting('app.tenant_id', true), '')::bigint)`. It keeps the NULL default deny (NULL never equals) and turns per-row calls into one per statement. Check that index plans still use it; I only measured the sequential case.
- Because the tenant comes from a GUC, not from a role, Tetsu's policies are not role-specific. The plan-reuse CVEs (2023-2455, 2024-10976, 2026-14666) require role-specific policies, so their impact on this design is low, but pin to 18.6 or later anyway.
- SQL injection becomes tenant escape: any injected `set_config` or `SET app.tenant_id` switches tenant. Parameterised queries are therefore part of the isolation boundary.
- deadpool `RecyclingMethod::Fast` does not reset session state. A session-level `set_config(..., false)` or plain `SET` would leak a tenant to the next checkout. Only `is_local = true` inside an explicit transaction is safe. Test: run one statement that sets the tenant at session level, return the connection, check out again, and assert `current_setting('app.tenant_id', true)` is empty. Better: a grep or lint that forbids `set_config(..., false)` and `SET app.` in code. Pool behaviour is from deadpool's design, not verified against its docs here.
- Statements run outside a transaction in autocommit mode get their own implicit transaction, so a `set_config(..., true)` sent as a separate statement does not carry over. The app must issue it inside `BEGIN ... COMMIT` with the queries.

**Gaps:**

- I found no source tying PG 16, 17, or 18 release notes to RLS planner changes. I did not fetch release notes.
- No source measured initplan wrapping against an index path on PG 18.
- Neon's current minor version for PG 18 was not checked; confirm it is 18.6 or later.

## Known RLS CVEs and version pinning

**Takeaway:**

RLS CVEs recur about once a year, mostly around plan reuse across role changes and planner statistics. Pin to the
latest minor of 18, at least 18.6, and track the PostgreSQL security page.

**Cited Findings:**

- CVE-2026-14666: row security caching disregards role modifications. Affects 14 to 18; fixed 18.6, 17.11, 16.15, 15.19, 14.24. [CVE-2026-14666](https://www.postgresql.org/support/security/CVE-2026-14666/)
- CVE-2024-10976: row security below subqueries, WITH queries, security invoker views, or SQL functions disregards user ID changes; requires role-specific policies and a plan reused across `SET ROLE` or SECURITY DEFINER. Fixed 17.1, 16.5, 15.9, 14.14, 13.17, 12.21 on 2024-11-14. CVSS 4.2. It completes CVE-2023-2455 and CVE-2016-2193. [CVE-2024-10976](https://www.postgresql.org/support/security/CVE-2024-10976/)
- CVE-2023-39418: MERGE fails to enforce UPDATE or SELECT row security policies. Affects 15; fixed 15.4. [PG security page](https://www.postgresql.org/support/security/)
- CVE-2023-2455: row security policies disregard user ID changes after inlining. Affects 14, 15; fixed 15.3, 14.8. [PG security page](https://www.postgresql.org/support/security/)
- CVE-2025-8713: optimizer statistics expose sampled data within a view, partition, or child table. Fixed 17.6, 16.10, 15.14, 14.19, 13.22. [CVE-2025-8713](https://www.postgresql.org/support/security/CVE-2025-8713/)
- CVE-2024-4317: `pg_stats_ext` and `pg_stats_ext_exprs` restricted to table owner. Fixed 16.3, 15.7, 14.12. [PG security page](https://www.postgresql.org/support/security/)
- CVE-2019-10130 let crafted operators read planner statistics holding hidden data; affected 9.5 to 11, fixed 9.5.17, 9.6.13, 10.8, 11.3. [Bytebase footguns](https://www.bytebase.com/blog/postgres-row-level-security-footguns/)
- CVE-2026-14668 (ctid type confusion in a selectivity estimator) and CVE-2026-6575 (`pg_restore_attribute_stats` out-of-bounds read, 18 only, fixed 18.4) are planner or stats issues, not RLS-specific. [PG security page](https://www.postgresql.org/support/security/)

**Inferences:**

- 18.6 is the floor for this design, since it fixes CVE-2026-14666 and CVE-2026-14668. Add a startup or CI check that `server_version_num >= 180006`.
- The app does not use role-specific policies or `SET ROLE`, which removes the precondition of the plan-reuse CVEs. Keep it that way: one app role, tenant in a GUC.

**Gaps:**

- The CVE-2016-2193 and CVE-2017-7484 detail pages were not fetched; they appear only as references in the pages above.
- The CVE-2023-2455 detail page was not fetched; its one-line summary comes from the index.
