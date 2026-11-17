# PostgreSQL roles for a modular-monolith multi-tenant app: per-module roles, tables above any tenant, and Neon specifics

Researched 2026-11-11. Context: Rust modular monolith, one Postgres schema per module, tokio-postgres + deadpool-postgres (one pool, `RecyclingMethod::Fast`, statement cache), PostgreSQL 18 on Neon Free, pooled tenancy with `tenant_id` + RLS set per transaction with `set_config(..., true)`.

## Per-module roles: SET LOCAL ROLE on one pool vs one pool per module role

**Takeaway:**

`SET LOCAL ROLE` per transaction on one pool works with prepared statements and with transaction pooling, because it reverts at transaction end and the plan cache revalidates role-dependent plans. It guards against bugs, not against injected SQL: any SQL that runs on the connection can `RESET ROLE` or `SET ROLE` to another granted role. One pool per module role is the only option that makes the module boundary hold against injection, at the cost of more connections.

**Cited Findings:**

- `SET [ SESSION | LOCAL ] ROLE`: "The `SESSION` and `LOCAL` modifiers act the same as for the regular SET command." — [PG 18 SET ROLE](https://www.postgresql.org/docs/18/sql-set-role.html)
- "The effects of `SET LOCAL` last only till the end of the current transaction, whether committed or not." `SET LOCAL` outside a transaction block "emits a warning and otherwise has no effect." Rolling back to an earlier savepoint also cancels it. — [PG 18 SET](https://www.postgresql.org/docs/18/sql-set.html)
- `set_config` "provides equivalent functionality" to `SET`. — [PG 18 SET](https://www.postgresql.org/docs/18/sql-set.html)
- `RESET ROLE` and `SET ROLE NONE` return to the session user (or a connection-time role setting), and "These forms can be executed by any user." — [PG 18 SET ROLE](https://www.postgresql.org/docs/18/sql-set-role.html)
- To switch, "The current session user must have the `SET` option for the specified role_name." Memberships granted `WITH INHERIT FALSE` give no privileges by default, but `WITH SET TRUE` still allows `SET ROLE` to that role. `WITH INHERIT FALSE, SET FALSE` makes the role's privileges unusable. — [PG 18 SET ROLE](https://www.postgresql.org/docs/18/sql-set-role.html)
- "After `SET ROLE`, permissions checking for SQL commands is carried out as though the named role were the one that had logged in originally." `SET ROLE` does not apply the target role's `ALTER ROLE ... SET` settings (for example a per-role `search_path`); that happens only at login. — [PG 18 SET ROLE](https://www.postgresql.org/docs/18/sql-set-role.html)
- "`SET ROLE` cannot be used within a `SECURITY DEFINER` function." — [PG 18 SET ROLE](https://www.postgresql.org/docs/18/sql-set-role.html)
- Plan cache and role changes: the plan cache records `rewriteRoleId = GetUserId()` and marks a cached query invalid "if either the role or the row_security setting has changed" when the rewrite had an RLS dependency. A generic plan with `dependsOnRole` built under another role is discarded ("If plan isn't valid for current role, we can't use it"). Role-dependent plans never take the fast validity path. — [plancache.c, REL_18_STABLE](https://raw.githubusercontent.com/postgres/postgres/REL_18_STABLE/src/backend/utils/cache/plancache.c)
- Prepared statements are re-planned after DDL on referenced objects or a `search_path` change; an unqualified name does not re-resolve when a same-named table appears earlier in `search_path` unless something else forces a re-parse. — [PG 18 PREPARE](https://www.postgresql.org/docs/18/sql-prepare.html)
- deadpool-postgres `RecyclingMethod::Fast` "does not check the connection health or perform any clean-up queries." `Clean` runs `CLOSE ALL; SET SESSION AUTHORIZATION DEFAULT; RESET ALL; UNLISTEN *; SELECT pg_advisory_unlock_all(); DISCARD TEMP; DISCARD SEQUENCES;` and keeps the statement cache (omits `DEALLOCATE ALL` and `DISCARD PLAN`). Version 0.14.2. — [docs.rs deadpool-postgres RecyclingMethod](https://docs.rs/deadpool-postgres/latest/deadpool_postgres/enum.RecyclingMethod.html)
- Injection risk with role switching on pooled connections: "Any SQL injection vulnerability or anything else that allows arbitrary SQL allows the client to issue a `RESET ROLE; SET ROLE anybody_else;`, bypassing authentication." A proposed "role sandboxing" feature (non-resettable `SET ROLE`) was discussed on pgsql-hackers; it is a proposal, not a shipped feature. — [pgsql-hackers, Role Sandboxing proposal](https://www.postgresql.org/message-id/CACA6kxh4MfRCHuY%2BuC2ZvXRQUP63LqumNtxtLsDF-mJswAJR5w%40mail.gmail.com); [reply by Joe Conway](https://www.postgresql.org/message-id/3c61faea-1dc3-486a-bd0b-f2be4656b45b%40joeconway.com)
- An older pgsql-general thread makes the same point about pools that `SET ROLE`: each user "could use RESET ROLE and become the super user." — [pgsql-general, RESET ROLE and connection pool](https://www.postgresql.org/message-id/AANLkTikbHNZWU_4WvRE3VTEJZQk8L8RAqUhz8F%3D7Y7n0%40mail.gmail.com)
- EDB: with pooler-issued `SET ROLE`, "the user must not be able to execute custom SQL on the connection (because then it's just a matter of RESET ROLE to gain the full privileges, or SET ROLE to switch to another role)." (Quote via search snippet; page not fetched in full.) — [EDB, Auditing users and roles](https://www.enterprisedb.com/index.php/blog/auditing-users-and-roles-postgresql)
- Precedent for one login role per module: Milan Jovanovic's modular-monolith guide creates `orders_role LOGIN`, `CREATE SCHEMA orders AUTHORIZATION orders_role`, grants on that schema only, `ALTER ROLE orders_role SET search_path = orders`, and one connection string per module. It asserts "separate credentials ensure that a misconfigured context cannot access another schema." It does not discuss pool cost. — [Milan Jovanovic, data boundaries in a modular monolith](https://milanjovanovic.tech/blog/how-to-keep-your-data-boundaries-intact-in-a-modular-monolith)
- Neon PgBouncer `default_pool_size` is 0.9 × `max_connections` and is "the maximum active connections per user per database." — [Neon connection pooling](https://neon.com/docs/connect/connection-pooling)
- Neon `max_connections`: 112 at 0.25 CU, 450 at 1 CU, 901 at 2 CU; "Seven connections are reserved for the Neon superuser account." — [Neon connection pooling](https://neon.com/docs/connect/connection-pooling)

**Inferences:**

- With `RecyclingMethod::Fast`, a plain `SET ROLE` would leak into the next checkout. `SET LOCAL ROLE` (or `set_config('role', ..., true)`) inside the same transaction as the tenant `set_config` avoids that. A safety net is a `Custom` recycle query or `Clean`, but that adds a round trip per checkout.
- Prepared statements stay correct under `SET LOCAL ROLE`: tokio-postgres uses protocol-level statements, and the plan cache replans when the role differs for RLS-dependent plans. The cost is replanning when one connection alternates between module roles, since a generic plan built under role A is thrown away under role B. This is a performance note, not a correctness one.
- Unqualified names resolve through `search_path` at parse time. Since `SET ROLE` does not apply a role's `ALTER ROLE ... SET search_path`, a single-pool design should schema-qualify every name (or set `search_path` locally too). Schema-qualified SQL also sidesteps the PREPARE re-parse caveat above.
- Hardening the single-pool design: give the login role no table privileges itself and grant module roles to it `WITH INHERIT FALSE, SET TRUE`. Then `RESET ROLE` lands on a role that can touch nothing. Injected SQL can still `SET ROLE` to any other module role, so the boundary still fails against injection. It reliably catches a module that forgets its own role or queries another schema by mistake.
- Pool-per-role cost on Neon Free: each module pool holds its own connections. Five modules with eight connections each is 40 of the 105 usable direct connections at 0.25 CU. Through Neon's pooler, each role gets its own server-side pool (per user per database), so total backend connections scale with the number of roles too. deadpool pools are lazy, so idle modules hold few connections.
- A middle option: one pool per trust class (for example an app pool and a separate lookup pool, see below) rather than per module, and `SET LOCAL ROLE` within the app pool for bug-catching. The DB then enforces the trust classes against injection and module separation against mistakes.

**Gaps:**

- No primary source found that measures replanning cost from alternating `SET LOCAL ROLE` on one connection.
- No first-party Neon statement found on whether `SET LOCAL ROLE` passes through the pooler unchanged. The PgBouncer feature table lists only `SET/RESET` as unsupported in transaction mode and does not mention `SET LOCAL` ([PgBouncer features](https://www.pgbouncer.org/features.html)). By PG semantics a transaction-scoped setting cannot outlive the transaction, so it should be safe, but this is reasoning, not a cited test.
- No Crunchy Data or Cybertec post was found that covers this exact pattern in the time available.

## Schema-level privileges, default privileges, PUBLIC, and search_path hardening

**Takeaway:**

Grant `USAGE` on each module schema only to that module's role, grant table privileges, and use `ALTER DEFAULT PRIVILEGES FOR ROLE <migration owner> IN SCHEMA <module>` so future tables get the same grants. Revoke the remaining PUBLIC defaults that matter (function `EXECUTE`, database `TEMPORARY`), keep `public` non-writable, and set a fixed `search_path` on every `SECURITY DEFINER` function.

**Cited Findings:**

- "No privileges are granted to `PUBLIC` by default on tables, table columns, sequences, ... schemas ..." PUBLIC does get `CONNECT` and `TEMPORARY` on databases, `EXECUTE` on functions and procedures, and `USAGE` on languages and types. — [PG 18 Privileges](https://www.postgresql.org/docs/18/ddl-priv.html)
- "The right to modify or destroy an object is inherent in being the object's owner, and cannot be granted or revoked in itself." Owners "can always re-grant their own privileges." — [PG 18 Privileges](https://www.postgresql.org/docs/18/ddl-priv.html)
- Column-level grants exist for `SELECT`, `INSERT`, `UPDATE`, and `REFERENCES`, e.g. `GRANT SELECT (col1), UPDATE (col1) ON mytable TO miriam_rw;`. — [PG 18 Privileges](https://www.postgresql.org/docs/18/ddl-priv.html)
- PG15+: in new databases, ordinary users cannot create in `public`. Upgraded databases keep the old default and need `REVOKE CREATE ON SCHEMA public FROM PUBLIC`. — [PG 18 Schemas](https://www.postgresql.org/docs/18/ddl-schemas.html)
- "adding a schema to `search_path` effectively trusts all users having `CREATE` privilege on that schema." — [PG 18 Schemas](https://www.postgresql.org/docs/18/ddl-schemas.html)
- `pg_catalog` is implicitly searched first unless named explicitly in the path. A shared writable `public` schema "is never a secure pattern." — [PG 18 Schemas](https://www.postgresql.org/docs/18/ddl-schemas.html)
- `ALTER DEFAULT PRIVILEGES FOR ROLE target_role`: "Change default privileges for objects created by the target_role." At creation time "new object permissions are only affected by the default privileges of the current role, and are not inherited from any roles in which the current role is a member." — [PG 18 ALTER DEFAULT PRIVILEGES](https://www.postgresql.org/docs/18/sql-alterdefaultprivileges.html)
- Per-schema defaults "are added to whatever the global default privileges are." You cannot revoke per schema something granted globally, so `ALTER DEFAULT PRIVILEGES IN SCHEMA public REVOKE EXECUTE ON FUNCTIONS FROM PUBLIC` does nothing. Use the global form: `ALTER DEFAULT PRIVILEGES FOR ROLE admin REVOKE EXECUTE ON FUNCTIONS FROM PUBLIC;`. Existing objects are not affected. — [PG 18 ALTER DEFAULT PRIVILEGES](https://www.postgresql.org/docs/18/sql-alterdefaultprivileges.html)
- Neon repeats the trap: "`ALTER DEFAULT PRIVILEGES` applies to tables created later by the role that runs it." — [Neon roles](https://neon.com/docs/manage/roles)
- RLS owner bypass: "Table owners normally bypass row security as well, though a table owner can choose to be subject to row security with ALTER TABLE ... FORCE ROW LEVEL SECURITY." Superusers and `BYPASSRLS` roles "always bypass." — [PG 18 Row Security](https://www.postgresql.org/docs/18/ddl-rowsecurity.html)

**Inferences:**

- Because default privileges key on the creating role, the migration runner must create every table as one known owner role (not as whichever Neon console role happens to run it). If migrations ever run as a different role, new tables silently miss their grants. A check in CI that compares `information_schema.role_table_grants` with the expected matrix would catch this.
- Sequences need grants too (`USAGE` for `nextval`) if any table uses serial columns; identity columns created by the owner need the same default-privilege line `ON SEQUENCES`.
- Revoke `TEMPORARY` on the database from PUBLIC so the app role cannot create temp objects that shadow names via `pg_temp` (relevant to the `SECURITY DEFINER` hazard below).
- Since the app role is not the owner, RLS applies without `FORCE`. `FORCE ROW LEVEL SECURITY` is still cheap insurance in case a job runs as the owner, though Neon owner roles have `BYPASSRLS` (see Neon section), which `FORCE` does not override.

**Gaps:**

- No source compared per-module default-privilege setups at scale; the guidance above is assembled from the PG reference pages.

## Guarding tables read before a tenant is known

**Takeaway:**

The options are (1) a plain grant on a table with no RLS, (2) a separate lookup role or pool, (3) a `SECURITY DEFINER` function that returns only the needed row, (4) column-level grants, or (5) an RLS policy keyed on a different setting. A narrow `SECURITY DEFINER` lookup function, owned by a role that is not the table owner of tenant data, with `SET search_path` and `EXECUTE` revoked from PUBLIC, gives the tightest surface; a plain grant is simplest but exposes the whole mapping to any code (or injection) on that role.

**Cited Findings:**

- `SECURITY DEFINER` hazards: "search_path should be set to exclude any schemas writable by untrusted users ... Particularly important in this regard is the temporary-table schema, which is searched first by default ... write `pg_temp` as the last entry in `search_path`." Example uses `SET search_path = admin, pg_temp`. — [PG 18 CREATE FUNCTION](https://www.postgresql.org/docs/18/sql-createfunction.html)
- "by default, execute privilege is granted to `PUBLIC` for newly created functions ... you must revoke the default PUBLIC privileges and then grant execute privilege selectively. To avoid having a window where the new function is accessible to all, create it and set the privileges within a single transaction." — [PG 18 CREATE FUNCTION](https://www.postgresql.org/docs/18/sql-createfunction.html)
- A function `SET` clause restores the prior value on exit; `SET LOCAL` inside it stays local to the function, but a plain `SET` inside persists after exit. — [PG 18 CREATE FUNCTION](https://www.postgresql.org/docs/18/sql-createfunction.html)
- Supabase's pattern: put security definer helpers in a non-exposed `private` schema, `set search_path = ''`, and schema-qualify every name. "Never create one in a schema listed under 'Exposed schemas'." — [Supabase RLS](https://supabase.com/docs/guides/database/postgres/row-level-security)
- Column grants can restrict a role to specific columns (e.g. read only `host` and `tenant_id`). — [PG 18 Privileges](https://www.postgresql.org/docs/18/ddl-priv.html)
- With RLS enabled and no policy, "a default-deny policy is used." Policies can be limited to roles with `TO`, and "If no role is specified, or the special user name `PUBLIC` is used, then the policy applies to all users." — [PG 18 Row Security](https://www.postgresql.org/docs/18/ddl-rowsecurity.html)
- "Referential integrity checks ... always bypass row security ... Care must be taken ... to avoid 'covert channel' leaks of information through such referential integrity checks." — [PG 18 Row Security](https://www.postgresql.org/docs/18/ddl-rowsecurity.html)
- Policy sub-selects on other tables can race: under `READ COMMITTED` a user saw a row after losing access because the sub-select read the old snapshot. Mitigations: `FOR SHARE` in the sub-select (needs `UPDATE` on that table), lock the referenced table on update, or wait for concurrent transactions. — [PG 18 Row Security](https://www.postgresql.org/docs/18/ddl-rowsecurity.html)
- Nile's model: with no tenant context set, "the connection can access across all the tenant DBs," and shared tables such as `tenants` are readable in either mode. — [Nile tenant isolation](https://www.thenile.dev/docs/tenant-virtualization/tenant-isolation)

**Inferences:**

Hazards per option (reasoned from the sources above):

- **Plain grant, no RLS.** Simple and fast. Any query on the app role, including injected SQL, can list every web address and its tenant. Acceptable if the mapping is public information anyway (custom domains are visible in DNS), but tenant names or plan data in the same table would leak too.
- **Separate lookup role or pool.** The role can `SELECT` only the mapping table (possibly only some columns). Strong against injection on the main pool if it is a separate login. If reached by `SET LOCAL ROLE` from the main pool, it adds nothing against injection. Costs a few extra connections.
- **`SECURITY DEFINER` function** such as `tenancy.resolve_host(text) returns uuid`. The caller gets one answer per known input and cannot enumerate. Hazards: forgetting `SET search_path` (temp-table shadowing), forgetting to revoke `EXECUTE` from PUBLIC, and the function owner having `BYPASSRLS` or broad rights. Owner should be a dedicated `NOLOGIN` role that owns only the lookup table, not the Neon owner role.
- **Column-level grants.** Cheap and declarative. `SELECT *` fails, so code must name columns. Does not stop enumeration of the granted columns.
- **RLS keyed on a different setting** (e.g. `current_setting('app.host', true)`). The caller can set any value, so the setting is not a secret. It narrows accidental reads but does not stop a deliberate one. Also note `current_setting(..., true)` returns NULL when unset, which fails closed in an equality check.
- The tenant-unknown phase should be one short transaction that resolves the tenant, then the request's real transaction sets `app.tenant_id`. Keeping the lookup separate avoids running tenant queries with no tenant set.

**Gaps:**

- No source found that benchmarks `SECURITY DEFINER` lookups against plain grants for this pattern.

## Accounts that span tenants (global users with per-tenant memberships)

**Takeaway:**

The common pattern is a global users table with no `tenant_id` plus a membership table keyed `(tenant_id, user_id)`. Supabase puts users in `auth.users` and writes policies that read membership tables through `SECURITY DEFINER` helpers. Nile ships `users.users` (global) and `users.tenant_users` (tenant-aware). Platform operators are usually a separate role or path that bypasses tenant RLS, kept off the request path.

**Cited Findings:**

- Nile `users.users`: `id uuid` primary key, `email`, names, timestamps, soft-delete `deleted`; unique index on `email` where `deleted IS NULL`; no `tenant_id`. — [Nile built-in tables](https://thenile.dev/docs/auth/concepts/builtintables.md)
- Nile `users.tenant_users`: `tenant_id`, `user_id`, `roles text[]`, `email`, timestamps, `deleted`; primary key `(tenant_id, user_id)`; foreign key to `tenants(id)`. — [Nile built-in tables](https://thenile.dev/docs/auth/concepts/builtintables.md)
- Nile sets tenant context with `set nile.tenant_id = '<uuid>'`, which "is equivalent to pointing the connection to a specific tenant DB." — [Nile tenant isolation](https://www.thenile.dev/docs/tenant-virtualization/tenant-isolation)
- Supabase links app tables to `auth.users` (e.g. `user_id uuid references auth.users`), uses `auth.uid()`, and says "Always name the role a policy applies to, using the `to` clause." — [Supabase RLS](https://supabase.com/docs/guides/database/postgres/row-level-security)
- Supabase warns "Two tables whose policies read each other never resolve" (error `42P17`), and fixes it with a `security definer` function in `private` that returns the user's membership IDs, used as `id in (select private.user_list_ids())`. — [Supabase RLS](https://supabase.com/docs/guides/database/postgres/row-level-security)
- Supabase: wrap functions as `(select auth.uid())` so the planner runs an initPlan once per statement; index every column a policy filters on. — [Supabase RLS](https://supabase.com/docs/guides/database/postgres/row-level-security)
- Supabase's `service_role` has `bypassrls`; a secret key "bypasses RLS only when the request carries no user access token," and must never reach the browser. — [Supabase RLS](https://supabase.com/docs/guides/database/postgres/row-level-security)
- `auth.uid()` returns NULL when unauthenticated; Supabase recommends `auth.uid() IS NOT NULL AND auth.uid() = user_id`. — [Supabase RLS](https://supabase.com/docs/guides/database/postgres/row-level-security)
- Policies that consult other tables can leak through race conditions under `READ COMMITTED`. — [PG 18 Row Security](https://www.postgresql.org/docs/18/ddl-rowsecurity.html)

**Inferences:**

- For Tetsu, staff who work at several businesses map to Nile's shape: a global account table in `identity` with no `tenant_id`, and a membership table with `tenant_id` under the usual tenant RLS. The current tenant comes from the request (web address), and the app checks membership for that tenant before setting `app.tenant_id`.
- The global account table is read before or across tenants (login, "which businesses do I work at"). It needs one of the guards from the previous section, keyed on the logged-in account (e.g. `app.account_id`) rather than on tenant.
- A "my businesses" list crosses tenants by design. Options: a `SECURITY DEFINER` function that returns memberships for `current_setting('app.account_id')`, or an RLS policy on the membership table that allows rows where `account_id = app.account_id` in addition to the tenant rule. Either way the account setting must be set only by trusted code after authentication.
- Platform operators: a separate login role with `BYPASSRLS` (or a policy `TO operator_role`) on a separate pool, used only by admin paths, keeps the request-path role unable to cross tenants. On Neon, `BYPASSRLS` comes from `neon_superuser` membership (see Neon section), so a narrow operator role made in SQL would need policies `TO operator_role` rather than the attribute, unless Neon lets you grant `BYPASSRLS` directly (not confirmed, see Gaps).
- Revoking access: because of the documented policy race, a removed membership may still pass checks in transactions already running. Short transactions limit the window.

**Gaps:**

- Nile's user-management page on how `tenant_users` is enforced (RLS vs app) was not fetched; only the table definitions were confirmed.
- No primary source on how Supabase models platform-operator access beyond `service_role`.

## Neon specifics: roles, neon_superuser, BYPASSRLS, connections, pooler, RLS guidance

**Takeaway:**

On Neon you can create non-owner roles in SQL and grant precisely; SQL-created roles get only basic `public` privileges and no `neon_superuser` membership. Console, API, and CLI roles join `neon_superuser`, which has `BYPASSRLS` (projects after 2023-08-15) and read/write on all data, so the owner role bypasses RLS. Free plan allows up to 2 CU; at 0.25 CU there are 112 direct connections (105 usable). The pooler is PgBouncer in transaction mode, where session `SET` is unsupported but protocol-level prepared statements work.

**Cited Findings:**

- `neon_superuser` can create databases (`CREATEDB`), create/alter/drop roles (`CREATEROLE`), read and write all data (`pg_read_all_data`, `pg_write_all_data`), replicate, run maintenance, and cancel non-member sessions. "Users can't log in as `neon_superuser`, but they inherit the privileges assigned to this role." — [Neon roles](https://neon.com/docs/manage/roles)
- `BYPASSRLS` is part of `neon_superuser` only in projects created after the 2023-08-15 release. — [Neon roles](https://neon.com/docs/manage/roles); [Neon changelog 2023-08-15](https://neon.com/docs/changelog/2023-08-15)
- "Roles created with SQL receive only basic public schema privileges and must be granted permissions explicitly." Console and API roles get `neon_superuser` membership; `neon roles create` does too unless given `--access read_only` or `--access read_write`. — [Neon roles](https://neon.com/docs/manage/roles)
- Limits: 500 roles per branch; role names up to 63 bytes; names starting `pg_` reserved; roles belong to a branch and copy to child branches; SQL passwords need at least 60 bits of entropy. Neon recommends `NOLOGIN` roles to group permissions. — [Neon roles](https://neon.com/docs/manage/roles)
- "Admin connection strings (e.g., `neondb_owner`) include the `BYPASSRLS` attribute, meaning they ignore RLS policies entirely." Neon recommends a separate `LOGIN` role without `BYPASSRLS` for the app, e.g. `CREATE ROLE authenticated_backend WITH LOGIN ...; GRANT USAGE ON SCHEMA public ...; GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE todos ...`, and shows `select set_config('request.jwt.claims', ..., true)` inside a transaction for pooled connections. — [Neon RLS query execution](https://neon.com/docs/guides/rls-query-execution)
- Neon's multi-tenant RLS guide: "Keep 'owner' for migrations only"; "your connection string for app requests should always use the least-privileged role"; sets tenant with `SET LOCAL app.tenant_id`. — [Neon guide, RLS for multi-tenant apps](https://neon.com/guides/rls-multi-tenant-apps)
- Neon RLS / Data API: "Enable RLS on every table you expose through the Neon Data API"; the Data API validates JWTs and provides `auth.user_id()`. Page last updated 2026-06-05. — [RLS in Neon](https://neon.com/docs/guides/neon-rls)
- Data API roles `authenticated` and `anonymous` are `NOLOGIN` and cannot be used for direct backend connections. (Via search summary of the page.) — [Neon Data API access control](https://neon.com/docs/data-api/access-control)
- Free plan (page updated 2026-11-11): autoscaling up to 2 CU; 100 CU-hours per project per month; scale to zero after 5 minutes, not disableable; 10 branches per project; 1 GB storage per project; 100 projects. The page states no connection or role limits. — [Neon plans](https://neon.com/docs/introduction/plans)
- `max_connections` by compute: 112 at 0.25 CU, 450 at 1 CU, 901 at 2 CU; seven reserved for Neon's superuser. — [Neon connection pooling](https://neon.com/docs/connect/connection-pooling)
- Pooler: PgBouncer `pool_mode=transaction`, `max_client_conn=10000`, `default_pool_size = 0.9 × max_connections` per user per database, settings not user-configurable. Unsupported in transaction mode: `SET`/`RESET`, `LISTEN`/`NOTIFY`, `WITH HOLD` cursors, SQL-level `PREPARE`/`DEALLOCATE`, temp tables with `PRESERVE`/`DELETE ROWS`, `LOAD`, session advisory locks. Protocol-level prepared statements are supported "as of PgBouncer 1.22.0" with `max_prepared_statements=1000`. Workarounds for `search_path`: direct connection, schema-qualified names, or `ALTER ROLE ... SET search_path`. — [Neon connection pooling](https://neon.com/docs/connect/connection-pooling)
- PgBouncer feature map: `SET/RESET` "Never" in transaction pooling; protocol-level prepared plans "Yes" with `max_prepared_statements` non-zero; "Cached plan reset" Yes. — [PgBouncer features](https://www.pgbouncer.org/features.html)

**Inferences:**

- Tetsu can create the module roles, lookup role, and app login role in a migration run as the owner, since the owner inherits `CREATEROLE` from `neon_superuser`. SQL-created roles start clean, which suits least privilege.
- Do not run the app as `neondb_owner` or any console-created role: they bypass RLS and can read all data. Running migrations as the owner is fine, but jobs that touch tenant data should use the app role.
- `FORCE ROW LEVEL SECURITY` does not help against the Neon owner, because `BYPASSRLS` always bypasses. Only role separation does.
- Through Neon's pooler, `SET LOCAL ROLE` and `set_config(..., true)` fit transaction mode because they end with the transaction. Session `SET ROLE` and session `set_config(..., false)` must not be used. With deadpool `Fast` recycling on a direct connection, the same rule prevents leaks between checkouts.
- Each extra login role creates a separate server-side pool at Neon's PgBouncer, each capped at 0.9 × `max_connections`. On a direct connection, the deadpool pools are the only cap, so their maximum sizes must sum below 105 at 0.25 CU. Autoscaling can raise `max_connections`, but planning against the minimum is safer.
- Roles copy to child branches, so preview branches inherit the grant setup. Roles created on a branch after a point-in-time are missing if a branch is restored to before that point.

**Gaps:**

- Not confirmed whether a non-superuser on Neon can `ALTER ROLE ... BYPASSRLS` on a SQL-created role (PG requires the granting role to have `BYPASSRLS` itself; whether `neon_superuser` membership satisfies that was not tested or documented on the pages read).
- Not confirmed whether Neon sets `max_connections` from the minimum or the maximum of the autoscaling range.
- The Neon Authorize product name did not appear on the current pages; the current docs describe RLS with the Data API. Whether Neon Authorize was renamed or retired was not confirmed from a primary source.
- Neon's pooling page does not mention `SET LOCAL` explicitly.
