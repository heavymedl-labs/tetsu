# Postgres tenant isolation: production practice, incidents, testing, and noisy-neighbour controls

Research date: 2026-11-11. Sources older than 2022 are marked (older). Several sources are vendor blogs; they are flagged where it matters.

## 1. Which teams use RLS, and which chose app-level scoping, and why

**Takeaway:**

Named teams with public write-ups split into two camps. Svix (Rust, Postgres) uses RLS on top of app-level filtering as defence in depth, and AWS recommends RLS as required for the pool model. Teams at very large scale (Notion, GitLab, Salesforce, PostHog, Citus users) scope by a tenant key in application or query-layer code, often because the same key also drives sharding or cell placement.

**Cited Findings:**

- Svix (Rust, Axum, Postgres) uses RLS as "just one element of a 'defense in depth' approach". Without it, "it's up to us to make sure that every application query appropriately passes the `account_id` filter." Post dated 2025-11-20. — [Svix blog](https://www.svix.com/blog/row-level-security-defense-in-depth/)
- Svix sets the tenant with `set_config(..., true)` at the start of each transaction and reads it through a `current_account()` helper. If the setting is missing, no rows are visible (fails closed). — [Svix blog](https://www.svix.com/blog/row-level-security-defense-in-depth/)
- Svix wraps this in a Rust `SecureTransaction` type that implements Axum's `FromRequestParts`, so tenant context comes from authentication automatically. This is a typed tenant context in production Rust. — [Svix blog](https://www.svix.com/blog/row-level-security-defense-in-depth/)
- Svix applies the same idea in ClickHouse: it sets a custom `SQL_account_id` setting, then sets `readonly = 1` so the request cannot change it. — [Svix blog](https://www.svix.com/blog/row-level-security-defense-in-depth/)
- AWS Prescriptive Guidance says RLS is required to isolate tenant data in the pool model, since there is no other logical separation. — [AWS best practices](https://docs.aws.amazon.com/prescriptive-guidance/latest/saas-multitenant-managed-postgresql/best-practices.html); [AWS decision matrix](https://docs.aws.amazon.com/prescriptive-guidance/latest/saas-multitenant-managed-postgresql/matrix.html)
- AWS argues for RLS because it centralizes isolation in the database, acting as "an automated WHERE clause that the database engine manages itself", and makes enforcement "less dependent on the everyday variability of your source code" (2020, older). — [AWS Database Blog](https://aws.amazon.com/blogs/database/multi-tenant-data-isolation-with-postgresql-row-level-security/)
- Notion partitions by workspace ID because every block belongs to exactly one workspace. Routing from workspace ID to physical database happens in application code. This is app-level scoping driven by sharding (2021, older). — [Notion: Sharding Postgres at Notion](https://notion.com/blog/sharding-postgres-at-notion)
- Figma routes queries to shards through a query proxy (DBProxy) and uses different keys per table (user ID, file ID, or organization ID), not one tenant key. This is a secondary summary. — [pganalyze summary](https://pganalyze.com/blog/5mins-postgres-figma-dbproxy-sharding-postgres)
- GitLab is moving to Cells and wants `organization_id` reachable from all rows of all tables, so organizations can move between cells. Each cell-local table must declare a `sharding_key` (preferably `project_id` or `namespace_id`, with `organization_id` as fallback) in its `db/docs/` file. — [GitLab sharding guidelines](https://docs.gitlab.com/development/organization/sharding); [GitLab Cells guidelines](https://docs.gitlab.com/development/cells)
- GitLab enforces that newly created tables define a `sharding_key` with a merge request that added a check. — [GitLab MR 136682](https://gitlab.com/gitlab-org/gitlab/-/merge_requests/136682)
- GitLab allows exceptions for reference data that is identical in every cell and replicated per cell. — [GitLab sharding guidelines](https://docs.gitlab.com/development/organization/sharding)
- PostHog's HogQL layer injects a `team_id` filter on every table access. The handbook warns that hand-written raw SQL skips this guard, and cross-team queries must stay raw SQL with manual scoping. — [PostHog handbook: HogQL in Python](https://posthog.com/handbook/engineering/databases/hogql-python); [PostHog handbook: ClickHouse queries](https://posthog.com/handbook/engineering/databases/clickhouse-queries-new-products)
- Citus (Microsoft) tells multi-tenant apps to store the tenant ID on every table, use it as the distribution column, filter every query by it, and include it in primary and foreign keys. — [Citus docs: choosing distribution column](https://docs.citusdata.com/en/latest/sharding/colocation.html); [Citus docs: multi-tenant migration](https://docs.citusdata.com/en/v12.1/develop/migration_mt_schema.html)
- Salesforce stores an OrgID on every org-specific row in shared tables, including its metadata tables. — [Salesforce architect: platform multitenant architecture](https://architect.salesforce.com/fundamentals/platform-multitenant-architecture)
- Salesforce's 2008 design hash-partitions tables by OrgId and has a "no runaway queries" rule (older). — [Weissman, QCon 2008 slides](https://qconsf.com/sf2008/dl/qcon-sanfran-2008/slides_/CraigWeissmann_The_Magic_Behind_Multi_Tenancy.pdf)
- Crunchy Data publishes a tutorial on RLS with session variables for tenant isolation. — [Crunchy Data blog](https://crunchydata.com/blog/row-level-security-for-tenants-in-postgres)
- Supabase's database linter treats a table without RLS in an API-exposed schema as an ERROR (lint 0013). Neon's Data API advisor carries the same lints. — [Supabase splinter 0013](https://supabase.github.io/splinter/0013_rls_disabled_in_public); [Neon Data API advisors](https://neon.com/docs/data-api/database-advisor)

**Inferences:**

- The teams that skip RLS are those whose tenant key is also a shard or cell routing key, enforced in a query layer they own. That layer gives them one choke point, which plays the role RLS plays for others.
- Svix is the closest public precedent for Tetsu: Rust, Postgres, transaction-scoped `set_config`, a typed request extractor, and RLS as a second layer behind app-level filters.
- GitLab's "every table declares its sharding key, enforced in CI" is direct precedent for Tetsu's later move to cells.

**Gaps:**

- I found no primary public statement on the RLS versus app-level choice from Linear, Retool, Clerk, WorkOS, Atlassian, Shopify, or Basecamp in this pass. Do not claim their choices without a source.
- Nile docs were not fetched in this pass.
- Figma's own engineering post was not fetched; only a secondary summary was.

## 2. Published cross-tenant leaks and root causes

**Takeaway:**

Public, primary postmortems of cross-tenant leaks in pooled SQL databases are rare. The best documented cases come from connection-state reuse (OpenAI 2023) and missing or misconfigured RLS (Lovable apps on Supabase, CVE-2025-48757). Issue trackers show the routine cause: a query or lookup that forgot the tenant predicate.

**Cited Findings:**

- OpenAI, March 2023: a bug in the asyncio redis-py client for Redis Cluster, triggered by a spike in cancelled requests, returned data meant for one connection to another. Some users saw other users' chat titles, and billing details of about 1.2% of ChatGPT Plus subscribers may have been visible to others during a nine-hour window. — [OpenAI postmortem](https://openai.com/index/march-20-chatgpt-outage)
- Sonatype reported that the redis-py fix was initially incomplete. The issue was tracked as CVE-2023-28858 and CVE-2023-28859. — [Sonatype](https://sonatype.com/openai-data-leak-and-redis-race-condition-vulnerability-that-remains-unfixed)
- CVE-2025-48757: insufficient RLS policies in Lovable-generated apps through 2025-04-15 let unauthenticated attackers read or write arbitrary records, CWE-863. A scan found about 170 of 1,645 showcased apps exposed. Figures come mostly from security vendor write-ups. — [PT Security dbugs](https://dbugs.ptsecurity.com/vulnerability/PT-2025-23236); [Superblocks write-up](https://www.superblocks.com/blog/lovable-rls-breach)
- A small-project issue shows the routine bug: repository methods without a `tenant_id` predicate returned other merchants' records. — [StellarPOS issue #207](https://github.com/StellarPOS-App/backend/issues/207)
- Another issue shows unscoped lookups by numeric ID (IDOR) across tenants. The fix returns 404 on tenant mismatch so existence is not revealed. — [Elder issue #237](https://github.com/penguintechinc/elder/issues/237)
- A measured test found that a session-level tenant setting behind PgBouncer transaction pooling let the next client, which had set no tenant, see 267,023 invoices of another tenant. Transaction-scoped `set_config(..., true)` returned 0 rows. Single-author test. — [now-next.nl](https://now-next.nl/en/insights/postgresql-row-level-security-multi-tenant-saas/)
- Postgres itself has had RLS leaks through planner statistics: CVE-2019-10130 (older) let crafted operators read sampled values from hidden rows. — [Bytebase footguns](https://www.bytebase.com/blog/postgres-row-level-security-footguns/)
- A later CVE, CVE-2025-8713, is reported as optimizer statistics again exposing sampled data from rows RLS should hide. Secondary source only. — [Medium (InstaTunnel)](https://medium.com/@instatunnel/multi-tenant-leakage-when-row-level-security-fails-in-saas-da25f40c788c)

**Inferences:**

- The OpenAI case and the PgBouncer test point at the same class: state attached to a reused connection. Tetsu's choice of `set_config(..., true)` inside a transaction is the documented fix for the Postgres half of that class.
- Lovable shows that RLS only helps when every table has it. A catalog check in CI addresses that directly.
- Keep Postgres on current minor releases, since RLS has had statistics-based leaks.

**Gaps:**

- I found no primary postmortem from a large B2B SaaS company describing a cross-tenant leak in a pooled Postgres database.
- I did not verify CVE-2025-8713 against the PostgreSQL security page.

## 3. AWS guidance on pool isolation with RLS

**Takeaway:**

AWS recommends one shared application role and a session variable read by `current_setting()` in the policy, rather than a role per tenant. The main pitfalls are owner and superuser bypass, missing `WITH CHECK`, silent zero-row writes, and pooler interaction.

**Cited Findings:**

- AWS rates the pool model as cheapest and weakest on isolation, and says the noisy neighbour problem cannot be fully eliminated in it. — [AWS decision matrix](https://docs.aws.amazon.com/prescriptive-guidance/latest/saas-multitenant-managed-postgresql/matrix.html); [AWS best practices](https://docs.aws.amazon.com/prescriptive-guidance/latest/saas-multitenant-managed-postgresql/best-practices.html)
- AWS's example policy is `USING (tenant_id = current_setting('app.current_tenant')::UUID)`. — [AWS RLS recommendations (Spanish edition)](https://docs.aws.amazon.com/es_es/prescriptive-guidance/latest/saas-multitenant-managed-postgresql/rls.html)
- Per-tenant database roles, with policies comparing to `current_user`, are hard to maintain and scale poorly. AWS prefers a shared login plus a session variable (2020, older). — [AWS Database Blog](https://aws.amazon.com/blogs/database/multi-tenant-data-isolation-with-postgresql-row-level-security/)
- Superusers and `BYPASSRLS` roles ignore policies. The table owner does too unless the table has `FORCE ROW LEVEL SECURITY`. The app should connect as a role that is neither. — [AWS Database Blog](https://aws.amazon.com/blogs/database/multi-tenant-data-isolation-with-postgresql-row-level-security/)
- Session variables may conflict with server-side poolers such as PgBouncer, so the pooling setup must be tested. — [AWS Database Blog](https://aws.amazon.com/blogs/database/multi-tenant-data-isolation-with-postgresql-row-level-security/)
- Blocked SELECTs return fewer rows, blocked UPDATE and DELETE affect zero rows silently, and blocked INSERT raises an error. Write policies need `WITH CHECK`. — [AWS Database Blog](https://aws.amazon.com/blogs/database/multi-tenant-data-isolation-with-postgresql-row-level-security/)
- Further footguns: `SECURITY DEFINER` views bypass RLS (use `security_invoker` on Postgres 15 and later), non-`LEAKPROOF` functions in policies can block index use, permissive policies combine with OR (use `RESTRICTIVE`), global unique indexes reveal values across tenants, FK checks need the parent visible, and materialized views or exports lose protection. — [Bytebase footguns, 2025-09-05](https://www.bytebase.com/blog/postgres-row-level-security-footguns/)
- A policy that checks ownership through a joined table adds a join to every query. Svix's alternative is denormalizing the account ID onto the child table. — [Svix blog](https://www.svix.com/blog/row-level-security-defense-in-depth/)

**Inferences:**

- Tetsu's planned design (shared role, `set_config(..., true)`, `tenant_id` on every row) matches AWS's preferred pattern. The 2020 blog's "set it each time you get a connection" wording predates the common advice to scope it to the transaction; transaction scope is stricter.
- `tenant_id` directly on every table, as Citus and GitLab require for routing, also avoids the join-in-policy cost Svix measured.

**Gaps:**

- The English AWS RLS recommendations and pool model pages returned 404 to the fetcher; content came from search snippets and the Spanish edition. The PDF is at [AWS guide PDF](https://docs.aws.amazon.com/pdfs/prescriptive-guidance/latest/saas-multitenant-managed-postgresql/saas-multitenant-managed-postgresql.pdf).
- AWS SaaS Lens text was not fetched in this pass.

## 4. How teams test tenant isolation

**Takeaway:**

The precedent is three layers: a schema check that every tenant table has RLS enabled and forced with a policy (Atlas lint, Supabase lint, GitLab's sharding-key check), policy tests run as the non-owner app role, and negative tests that tenant A cannot see or write tenant B's rows.

**Cited Findings:**

- Atlas proposes lint rules that require RLS enabled on every table, `FORCE` on every RLS table, at least one policy per RLS table, `WITH CHECK` on write policies, `RESTRICTIVE` tenant policies, and an allowlist for `BYPASSRLS` roles. It runs these as required PR checks plus scheduled drift detection. Vendor guide. — [Atlas RLS guide](https://www.atlasgo.io/guides/rls-policy)
- Atlas's policy tests seed data as a superuser, then `SET ROLE app_tester;` so policies apply, and assert results such as `SELECT bool_and(tenant_id = 1) FROM orders`. — [Atlas RLS guide](https://www.atlasgo.io/guides/rls-policy)
- Supabase's linter flags tables without RLS (0013), policies on tables where RLS is off (0007), and RLS with no policy (0008). It also has a lint for always-true write policies (0024). — [Supabase splinter 0013](https://supabase.github.io/splinter/0013_rls_disabled_in_public); [GuardLayer explainer](https://www.guardlayer.io/blog/supabase-security-advisor)
- GitLab fails CI when a new cell-local table lacks a declared `sharding_key`. — [GitLab MR 136682](https://gitlab.com/gitlab-org/gitlab/-/merge_requests/136682); [GitLab Cells guidelines](https://docs.gitlab.com/development/cells)
- Testing as a superuser or owner makes broken policies look correct, so tests must run as the app role. — [Bytebase footguns](https://www.bytebase.com/blog/postgres-row-level-security-footguns/); [Svix blog](https://www.svix.com/blog/row-level-security-defense-in-depth/)
- PostHog relies on code review and tests for raw queries that bypass its HogQL guard. The claim comes from a third-party mirror of its docs. — [Mintlify mirror of PostHog data model](https://www.mintlify.com/PostHog/posthog/architecture/data-model)
- A practical negative test: run the same query with no tenant set behind a transaction pooler and assert zero rows. — [now-next.nl](https://now-next.nl/en/insights/postgresql-row-level-security-multi-tenant-saas/)

**Inferences:**

- A catalog query can implement the Atlas rules without adopting Atlas, for example: every table in the app schema has a `tenant_id` column, `pg_class.relrowsecurity` and `relforcerowsecurity` are true, and at least one row exists in `pg_policy` for it, minus an explicit allowlist of global tables (GitLab's reference-data exception is precedent). This is my inference, not a cited implementation.
- Also check that the app role has neither `rolsuper` nor `rolbypassrls` in `pg_roles`, mirroring Atlas's role rules.
- A generic negative test can loop over every tenant table from the catalog, insert rows for tenants A and B as a privileged role, then as the app role with tenant A assert no B rows are visible, and that updates and deletes of B rows affect zero rows and inserts labelled B fail.

**Gaps:**

- I found no public write-up of property-based or fuzz testing for tenant isolation from a named company.
- I did not confirm whether squawk has an RLS rule; I found no evidence that it does.

## 5. Defence in depth and typed tenant contexts

**Takeaway:**

Precedent favours both layers: app-level scoping as the primary path and RLS as a fail-closed backstop. Typed contexts (Svix's `SecureTransaction`, PostHog's team-scoped query function) make the scoped path the easy one.

**Cited Findings:**

- Svix keeps app-level `account_id` filters and adds RLS as a second layer, with a Rust type that opens a transaction already scoped to the authenticated account. — [Svix blog](https://www.svix.com/blog/row-level-security-defense-in-depth/)
- ClickHouse's engineering guide treats RLS as an infrastructure safety net, not the primary authorization gate, and says application filters should still target the right tenant. — [ClickHouse: multi-tenant SaaS on Postgres](https://clickhouse.com/resources/engineering/multi-tenant-saas-postgres-architecture)
- PostHog's `execute_hogql_query` takes one team and injects the guard; raw SQL skips it. — [PostHog handbook](https://posthog.com/handbook/engineering/databases/hogql-python)
- Citus still requires the application to filter every query by tenant ID even though data placement is by tenant. — [Citus docs](https://docs.citusdata.com/en/v12.1/develop/migration_mt_schema.html)
- RLS filters rows, not columns. Sensitive columns need column privileges or masking views. — [Bytebase footguns](https://www.bytebase.com/blog/postgres-row-level-security-footguns/)

**Inferences:**

- For Tetsu, a tenant-scoped transaction type that can only be built from an authenticated tenant, and that runs `set_config(..., true)` before handing out the client, follows Svix directly. Repository functions that take that type, and still filter by `tenant_id`, give the second layer and help the planner use tenant-leading indexes.

**Gaps:**

- No public measurements compare leak rates for RLS-only, app-only, and combined designs.

## 6. Per-tenant timeouts, rate limits, and noisy neighbours

**Takeaway:**

The documented controls are per-tier `statement_timeout` scoped to the transaction, per-tenant connection caps, tenant-aware rate limiting in the app, moving heavy work to queues, and per-tenant monitoring. Sources are mostly vendor guidance, not named-company postmortems.

**Cited Findings:**

- Neon recommends `statement_timeout` with thresholds per tenant tier (example: 60 s Enterprise, 15 s Basic), per-tenant PgBouncer connection caps (example: 50, 20, 5 by tier), tenant-aware rate limiting with queueing or retry-later responses, throttling non-critical queries from heavy tenants, and running bulk work off-peak or async. Neon's main recommendation is database per tenant. Post dated 2025-04-03. — [Neon blog](https://neon.com/blog/noisy-neighbor-multitenant)
- ClickHouse recommends strict per-tenant statement timeouts and prioritizing critical writes over heavy reads. — [ClickHouse guide](https://clickhouse.com/resources/engineering/multi-tenant-saas-postgres-architecture)
- Under PgBouncer transaction pooling a session-level setting can land on a different connection, so `SET LOCAL statement_timeout` is preferred. — [Lightrun / peewee discussion](https://lightrun.com/answers/coleifer-peewee-set-statement_timeout-per-connection-or-per-transaction)
- Salesforce's platform rule against runaway queries is long-standing precedent for hard per-tenant limits in a pooled database (2008, older). — [Weissman, QCon 2008 slides](https://qconsf.com/sf2008/dl/qcon-sanfran-2008/slides_/CraigWeissmann_The_Magic_Behind_Multi_Tenancy.pdf)
- Citus built "Landlord" to gather per-tenant query stats in Postgres (2018, older). — [Citus blog](https://www.citusdata.com/blog/2018/07/31/introducing-landlord-per-tenant-stats/)
- PostHog has a handbook page on query attribution for ClickHouse, which ties queries to teams. — [PostHog handbook: query attribution](https://posthog.com/handbook/engineering/clickhouse/query-attribution.md)
- AWS says noisy neighbours cannot be fully removed in the pool model. — [AWS best practices](https://docs.aws.amazon.com/prescriptive-guidance/latest/saas-multitenant-managed-postgresql/best-practices.html)

**Inferences:**

- Tetsu can set `statement_timeout` and `lock_timeout` with `set_config(..., true)` in the same round trip as the tenant ID, so they share transaction scope and reset at commit. Svix and the pgx benchmark show this batching is cheap. ([daily.dev pgx summary](https://daily.dev/posts/go-pgx-and-postgresql-row-level-security-measured-alrbtv31s))
- With deadpool-postgres, a per-tenant cap is a semaphore per tenant in front of the shared pool, since deadpool has one pool. This is my inference.
- Tagging each transaction with the tenant (for example in `application_name` or a log field) supports per-tenant monitoring in the Landlord and PostHog style.

**Gaps:**

- No named company published a per-tenant `SET LOCAL statement_timeout` implementation in this pass.
- I did not research HTTP edge rate limiting products or Neon's own per-endpoint limits.
