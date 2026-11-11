# Restoring one tenant and moving one tenant to its own cell (pooled PostgreSQL)

Research date: 2026-11-11. Each source's date is given where the page showed one.

## Per-tenant restore: approaches and pitfalls

**Takeaway::**

No managed Postgres host, Neon included, restores a single tenant. The common pattern is to restore the whole database to a side copy (a PITR copy or Neon branch), then copy one tenant's rows back with a tenant filter. That only works if every tenant-owned row carries the tenant key and the restore code knows the full list of tenant tables.

**Cited Findings::**

- AWS describes two ways to restore one tenant in a pooled database. You split tenant data at backup time ("extract"), or you split it at recovery time ("load") — [AWS Database Blog, "Managed database backup and recovery in a multi-tenant SaaS application" (date not shown on fetched page)](https://aws.amazon.com/blogs/database/managed-database-backup-and-recovery-in-a-multi-tenant-saas-application).
- AWS's recovery-time method restores the whole multi-tenant snapshot to a temporary database, extracts the tenant's data, loads it into the live database, then deletes the temporary database. AWS says "We prefer segregation during recovery." — [AWS Database Blog](https://aws.amazon.com/blogs/database/managed-database-backup-and-recovery-in-a-multi-tenant-saas-application).
- AWS says splitting tenant data at backup time "adds operational overhead to each backup cycle" and can be "unsuitable for SaaS providers with a large number of tenants." The benefits are per-tenant retention and per-tenant backup cost — [AWS Database Blog](https://aws.amazon.com/blogs/database/managed-database-backup-and-recovery-in-a-multi-tenant-saas-application).
- AWS's example tooling: `\copy (select * from table where tenant_id = '...')` to export, then `DELETE` plus `\copy ... from` to restore. It also uses a PL/pgSQL function that loops over a hard-coded table list. AWS warns that the list must be kept in step with the schema, and that "Data consistency is critical when exporting multiple tables." — [AWS Database Blog](https://aws.amazon.com/blogs/database/managed-database-backup-and-recovery-in-a-multi-tenant-saas-application).
- AWS notes that selective recovery must "overwrite any existing data that is being restored." — [AWS Database Blog](https://aws.amazon.com/blogs/database/managed-database-backup-and-recovery-in-a-multi-tenant-saas-application).
- AWS's older whitepaper says that in pooled storage "the challenge of isolating the data becomes much more complex." AWS now marks this whitepaper as historical — [AWS SaaS Tenant Isolation Strategies, pooled storage](https://docs.aws.amazon.com/whitepapers/latest/saas-tenant-isolation-strategies/pooled-storage-isolation-strategies.html).
- Neon instant restore rolls a whole root branch back. "the operation applies to _all_ databases on the branch, not just the one you are troubleshooting." It is "a _complete_ overwrite of the database timeline, not a merge or refresh." — [Neon docs, Branch restore (updated 2026-09-25)](https://neon.com/docs/guides/branch-restore).
- Neon can also create branches from past states within the history window, and Time Travel runs read-only queries against historical points — [Neon docs, History window (updated 2026-08-24)](https://neon.com/docs/postgres/backup-restore/history-window). Neon recommends Time Travel Assist to confirm the right restore point first — [Neon docs, Branch restore](https://neon.com/docs/guides/branch-restore).
- Salesforce partitions data, metadata, and pivot tables "physically ... by OrgID," and "every org-specific record in shared database tables has an _OrgID_." — [Salesforce Architects, Platform Multitenant Architecture (no date shown)](https://architect.salesforce.com/fundamentals/platform-multitenant-architecture).

**Inferences::**

- For Tetsu on Neon, the workable path is: create a branch at the timestamp before the damage (or use Time Travel to read it), then copy one tenant's rows from that branch into the live branch with `WHERE tenant_id = $1`, table by table in foreign key order, inside one transaction. Never use instant restore for a one-tenant problem, because it rewinds every tenant.
- Foreign keys: copy parents before children, or defer constraints for the transaction. Rows in the side copy may reference rows the live database has since deleted, or vice versa, so the copy step needs a merge policy (replace the tenant's rows entirely, or upsert by id).
- Conflicting IDs: with one shared database and bigint identity columns, restored rows keep their original ids, which cannot collide with new rows because the sequence only moves forward. Collisions appear only when rows move between databases with separate sequences (see the ID section). Inserting explicit ids into a `GENERATED ALWAYS` identity column needs `OVERRIDING SYSTEM VALUE` (PostgreSQL INSERT syntax; not re-fetched in this session).
- Rows that reference other tenants, and global tables (accounts, sign-in identities, platform settings), are the parts a tenant-filtered copy cannot restore cleanly. Keep them few, and keep tenant data from pointing into them except by stable ids.
- Soft deletes and history (audit) tables shrink the need for restores: many "undo" cases become an update rather than a database operation. They cost storage and query complexity, and they do not help after a bad migration or bulk update unless history rows capture the old values.
- Derive the table list from the catalog (every table with a `tenant_id` column) and test that it matches a declared list, so a new table cannot be silently skipped. AWS's hard-coded list is the failure mode to avoid.

**Gaps::**

- I found no first-party write-up from Slack, Shopify, Notion, or Figma describing single-tenant restore. Their posts cover moves, not restores.
- The AWS blog page fetched was partly truncated; its guidance on foreign keys and sequences was not visible.

## Moving a tenant to another cell or shard: precedent

**Takeaway::**

Every company that moves tenants online uses the same shape: bulk copy filtered by the tenant key, then stream later changes, then a short write freeze, flip the routing table, and delete the old copy. What made it hard was missing tenant keys on rows, cross-tenant data, and keeping the freeze short. Notion's stated regret is that it did not use a combined (tenant, id) primary key and did not shard earlier.

**Cited Findings::**

**Shopify (Ghostferry, pods):**

- Shopify moves shops between pods with Ghostferry, its open-source tool. It batch-copies a shop's rows table by table and tails the MySQL binlog, keeping only that shop's events — [Shopify Engineering, "Shard balancing" (2021-09-24)](https://shopify.engineering/mysql-database-shard-balancing-terabyte-scale).
- Cutover uses a Redis-backed multi-reader-single-writer lock to block the shop's writes, drains events to a final binlog position, then updates the routing table, which lives in a separate unsharded database. If the writer lock is not acquired in time, the move fails — [Shopify Engineering](https://shopify.engineering/mysql-database-shard-balancing-terabyte-scale).
- "We can attach a `shop_id` to all shop-owned tables," which serves as the sharding key — [Shopify Engineering](https://shopify.engineering/mysql-database-shard-balancing-terabyte-scale).
- Ghostferry runs verifiers before, during, and after migrations, and its core algorithm is specified in TLA+ — [Shopify Engineering](https://shopify.engineering/mysql-database-shard-balancing-terabyte-scale).
- A pod is an isolated instance of Shopify with its own datastores — [Shopify Engineering, "A pods architecture to allow Shopify to scale"](https://shopify.engineering/a-pods-architecture-to-allow-shopify-to-scale) (summarized in search results; not fetched).

**Notion (Postgres, sharded by workspace):**

- Notion partitions by workspace ID, a UUID assigned at creation, and each block belongs to exactly one workspace — [Notion, "Herding elephants" (2021-10-06)](https://www.notion.com/blog/sharding-postgres-at-notion).
- The old database lacked the partition key on rows, and backfilling it "would have exacerbated the load," so Notion filled `space_id` on the fly as rows were written to the shards — [Notion](https://www.notion.com/blog/sharding-postgres-at-notion).
- Notion's lessons: "Shard earlier," and "Introduce a combined primary key instead of a separate partition key," which would have removed the need to pass `space_id` throughout the application — [Notion](https://www.notion.com/blog/sharding-postgres-at-notion).
- Notion used an audit log plus a catch-up script rather than logical replication, which could not keep up with write volume. A separate team ran verification, including dark reads comparing old and new — [Notion](https://www.notion.com/blog/sharding-postgres-at-notion).

**Figma (Postgres, colos):**

- Figma grouped tables into "colos" sharing a key (UserID, FileID, OrgID) rather than adding one unified key, which would have needed backfills. Atomic cross-shard transactions were not supported — [Figma, "How Figma's databases team lived to tell the scale" (2024-03-14)](https://www.figma.com/blog/how-figmas-databases-team-lived-to-tell-the-scale/).
- Many keys were auto-increment or Snowflake-style timestamp-prefixed ids, so Figma routed on a hash of the key rather than migrate ids. Globally unique ids for sharded primary keys were listed as future work — [Figma](https://www.figma.com/blog/how-figmas-databases-team-lived-to-tell-the-scale/).
- Figma first sharded "logically" with views over one database, so rollback was a configuration change, before splitting physically — [Figma](https://www.figma.com/blog/how-figmas-databases-team-lived-to-tell-the-scale/).

**Stripe (DocDB data movement platform):**

- Steps: register the move in a chunk metadata service, bulk-load a point-in-time snapshot to the target, replicate changes from CDC, check correctness by comparing snapshots, switch traffic, then drop source data — [Stripe, "Zero-downtime data migrations" (2024-06-06)](https://stripe.dev/blog/how-stripes-document-databases-supported-99.999-uptime-with-zero-downtime-data-migrations).
- "The entire traffic switch protocol takes less than two seconds to execute," and failed requests succeed on retry. The goal is to keep that phase shorter than a planned primary failover — [Stripe](https://stripe.dev/blog/how-stripes-document-databases-supported-99.999-uptime-with-zero-downtime-data-migrations).

**Salesforce (org migration):**

- Salesforce built an internal service (MIPS) to automate Hyperforce org migrations across more than 95,000 orgs — [Salesforce Engineering (search snippet; full text not retrieved)](https://engineering.salesforce.com/?p=8121).
- A community post reports about three hours of read-only time for an org during migration — [Trailblazer Community, org migration topic (secondary)](https://trailhead.salesforce.com/trailblazer-community/topics/orgmigration). Treat as unverified.

**Citus (Postgres extension):**

- `isolate_tenant_to_new_shard(table_name, tenant_id, 'CASCADE')` moves one tenant's rows into a dedicated shard across all co-located tables; `citus_move_shard_placement` then moves it to another node — [Citus docs, Cluster management](https://docs.citusdata.com/en/stable/admin_guide/cluster_management.html).
- Moves use logical replication with a brief write lock "only during the time it takes to update metadata." A primary key on a non-distribution column "must be composite and contain the distribution column too." — [Citus docs](https://docs.citusdata.com/en/stable/admin_guide/cluster_management.html).

**AWS cell-based architecture:**

- AWS names a tenant becoming "too big and requires it to have a dedicated cell" as a reason to migrate. It gives four phases: Clone (non-authoritative copy), Flip (make the new copy authoritative), Redirect, Forget — [AWS whitepaper, Cell migration](https://docs.aws.amazon.com/wellarchitected/latest/reducing-scope-of-impact-with-cell-based-architecture/cell-migration.html).
- "Cells should not share state or components between cells." — [AWS whitepaper](https://docs.aws.amazon.com/wellarchitected/latest/reducing-scope-of-impact-with-cell-based-architecture/cell-migration.html).

**Slack (contrast):**

- Slack's cells are availability zones, not tenant groups. The goal is draining an AZ in about five minutes, and "Drains must not result in user-visible errors." — [Slack Engineering, "Slack's migration to a cellular architecture" (2023-08-22)](https://slack.engineering/slacks-migration-to-a-cellular-architecture/). This is not precedent for moving one tenant.

**Inferences::**

- Tetsu's plan (a routing table of addresses and a tenant key on every row) matches Shopify's design: routing table in a global store, `shop_id` on every shop-owned table, filtered copy plus change stream, short write lock.
- For Postgres, the change stream can be logical replication with a row filter (`CREATE PUBLICATION ... WHERE (tenant_id = ...)`, available since PostgreSQL 15), but that should be checked against Neon's logical replication support before relying on it. A simpler path at Tetsu's size: put the tenant in read-only mode for minutes, copy, flip the route. Salesforce's reported read-only window suggests customers accept planned windows.
- Notion's "combined primary key" lesson and Citus's composite key rule point the same way: make `tenant_id` part of every tenant table's primary key and foreign keys now.
- GitHub: I found no GitHub post about moving one tenant; its well-known partitioning work split by feature area, not tenant.

**Gaps::**

- Primary Salesforce engineering text on org migration was not retrieved; the 403 on the AppExchange org migration post blocked details on id preservation.
- No primary GitHub source on tenant moves.
- Shopify's pods post was not fetched; its details come from search summaries.

## ID strategy when tenants move between databases

**Takeaway::**

Per-database bigint sequences collide when a tenant moves into a database that already has rows. The fixes are globally unique ids (UUIDv7 now native in PostgreSQL 18) or a composite key (tenant_id, id) where id is only unique per tenant. UUIDv7 indexes are only modestly larger than bigint in informal tests, while UUIDv4 is much worse.

**Cited Findings::**

- PostgreSQL 18 adds `uuidv7([shift interval])`: "Generates a version 7 (time-ordered) UUID. The timestamp is computed using UNIX timestamp with millisecond precision + sub-millisecond timestamp + random." `uuid_extract_timestamp()` reads the time back — [PostgreSQL 18 docs, UUID functions](https://www.postgresql.org/docs/18/functions-uuid.html).
- Notion uses UUIDs for records and workspaces, and could verify by sampling UUID ranges — [Notion](https://www.notion.com/blog/sharding-postgres-at-notion).
- Figma kept auto-increment and Snowflake-style ids and listed globally unique ids as future work — [Figma](https://www.figma.com/blog/how-figmas-databases-team-lived-to-tell-the-scale/).
- Salesforce rows carry a GUID plus OrgID, and an index enforces uniqueness on OrgID plus GUID — [Salesforce Architects](https://architect.salesforce.com/fundamentals/platform-multitenant-architecture); [O'Reilly, Force.com Multitenant Architecture (preview snippet)](https://www.oreilly.com/library/view/the-forcecom-multitenant/30000LTI00089/30000LTI00089_ch08lev1sec11.html).
- Citus requires the distribution column in any composite primary key — [Citus docs](https://docs.citusdata.com/en/stable/admin_guide/cluster_management.html).
- Informal benchmark: primary key index 107 MB for BIGINT, 118 MB for UUIDv7, 285 MB for UUIDv4 — [DEV Community benchmark (informal, thin methodology)](https://dev.to/umangsinha12/postgresql-uuid-performance-benchmarking-random-v4-and-time-based-v7-uuids-n9b). Note: I saw this figure in search snippets; attribution to this exact post was not confirmed by fetch.
- A PostgreSQL 18 test at 50M rows found a 1981 MB index for v4 vs 1504 MB for v7 — [credativ (2025-12-05)](https://www.credativ.de/en/blog/postgresql-en/a-deeper-look-at-old-uuidv4-vs-new-uuidv7-in-postgresql-18/).
- A Spring Boot benchmark on PostgreSQL 18 put UUIDv7 inserts within about 15% of bigint — [stevenpg.com (2026)](https://stevenpg.com/posts/uuidv7-in-spring-boot-and-postgres/).

**Inferences::**

- Options for Tetsu, each makes a move collision-free:
  1. UUIDv7 primary keys. Unique everywhere, copy rows verbatim, no sequence work. Cost: 16 bytes vs 8, about 10% larger primary key index in informal tests, larger foreign key columns and indexes.
  2. Composite key `(tenant_id, id)` with bigint id. id may still come from one shared sequence, so in a pooled database no two rows share an id, but a new cell starts its own sequence and must be set above the highest moved id (or offset per cell). Keeps 8-byte ids but every foreign key becomes two columns.
  3. Keep bigint and offset sequences per cell (for example, each cell gets a disjoint range). Cheap, but fragile and easy to get wrong.
- Option 1 plus `tenant_id` leading every tenant index gives both: ids never collide, and tenant-scoped scans stay local. Switching later means rewriting every key and foreign key, so the choice is cheapest before real data exists.
- Externally visible ids (URLs, webhooks, exports) must survive a move unchanged. Globally unique ids make that automatic.

**Gaps::**

- No rigorous, reproducible bigint vs UUIDv7 index-size study at large row counts was found.
- The PostgreSQL 18 docs page fetched did not state monotonicity guarantees for `uuidv7()` within a session.

## Tables that make moves hard, and how to make a tenant enumerable

**Takeaway::**

Moves break on data that is not keyed by tenant: global accounts, cross-tenant links, shared queues, caches, search indexes, and object storage paths. Each tenant-owned thing needs the tenant key in its key or path, and anything global should live in a separate store that every cell reads.

**Cited Findings::**

- Shopify keeps the routing table in a separate unsharded database, and jobs not scoped to one shop take a global lock — [Shopify Engineering](https://shopify.engineering/mysql-database-shard-balancing-terabyte-scale).
- Shopify's pods: "all shared resources can only ever communicate to a single pod at a time" (search summary) — [Shopify Engineering, pods](https://shopify.engineering/a-pods-architecture-to-allow-shopify-to-scale).
- Notion chose the workspace key because nearly all data belongs to exactly one workspace, avoiding cross-shard joins. Transactions across datastores were a stated difficulty — [Notion](https://www.notion.com/blog/sharding-postgres-at-notion).
- Figma dropped atomic cross-shard transactions and product logic must tolerate partial failures — [Figma](https://www.figma.com/blog/how-figmas-databases-team-lived-to-tell-the-scale/).
- AWS: cells should not share state; dependencies between cells "decrease fault isolation." — [AWS whitepaper](https://docs.aws.amazon.com/wellarchitected/latest/reducing-scope-of-impact-with-cell-based-architecture/cell-migration.html).
- Neon instant restore does not revert Object Storage, so database and object data can diverge after a restore — [Neon docs, Branch restore](https://neon.com/docs/guides/branch-restore).

**Inferences::**

- Classify every table now as tenant-owned (has `tenant_id`, moves with the tenant), global (lives in a global store, read by all cells), or forbidden (mixes tenants). For Tetsu, likely global: sign-in identities and people who book at several studios, the tenant routing table, plans and billing catalog. Tenant-owned: services, staff, schedules, bookings, clients as seen by a business.
- A person who books at two businesses in different cells is the hardest case. Keep the global identity separate and give each tenant its own client row that references the global id, never a foreign key into another tenant's rows.
- Queues: put `tenant_id` on every job so jobs can be drained or moved. Pause a tenant's jobs during the cutover.
- Search indexes and caches: key by tenant and rebuild from the database after a move rather than copying.
- Object storage: prefix keys with the tenant id (for example `tenants/{tenant_id}/...`) so a tenant's files can be listed and copied with one prefix.
- Shared ledgers (platform billing, counters across tenants) belong in the global store, with per-tenant entries keyed by tenant.
- A test that fails when any table in a tenant schema lacks `tenant_id` (or is not on a declared global list) keeps enumeration complete.

**Gaps::**

- No primary source found that lists these table categories as a checklist; the list above is inference from the precedents.

## Neon specifics

**Takeaway::**

Neon restores whole branches only, within a history window of 6 hours on Free, up to 7 days on Launch, and up to 30 days on Scale. Per-tenant restore on Neon means branching at a past point and copying one tenant's rows back.

**Cited Findings::**

- History window: Free 6 hours default and maximum (capped at 1 GB of history); Launch default 1 day, maximum 7 days; Scale default 1 day, maximum 30 days. Launch and Scale history costs $0.20/GB-month. Setting it to zero disables instant restore and Time Travel — [Neon docs, History window (updated 2026-08-24)](https://neon.com/docs/postgres/backup-restore/history-window).
- Older Neon pages give different numbers (for example 24 hours on Free), so older figures are out of date — [Neon docs, older point-in-time restore page](https://neon.tech/docs/introduction/point-in-time-restore).
- Instant restore works on root branches only, by timestamp or LSN, from the branch's own history or another root branch's. Child branches cannot use it — [Neon docs, Branch restore (updated 2026-09-25)](https://neon.com/docs/guides/branch-restore).
- API: `POST /projects/{project_id}/branches/{branch_id}/restore` with `source_branch_id`, `source_timestamp` or `source_lsn`, and `preserve_under_name` — [Neon docs, Branch restore](https://neon.com/docs/guides/branch-restore).
- Restore applies to all databases on the branch, overwrites rather than merges, and leaves a backup branch named `{branch_name}_old_{head_timestamp}`. Connections drop briefly — [Neon docs, Branch restore](https://neon.com/docs/guides/branch-restore).
- PITR is not supported on branches created from a snapshot restore — [Neon docs, Branch restore](https://neon.com/docs/guides/branch-restore).

**Inferences::**

- On the Free plan, a tenant has at most 6 hours to notice a mistake before Neon can no longer recover it. A longer window needs a paid plan or Tetsu's own per-tenant exports or history tables.
- Because one schema per module lives in one database on one branch, there is no finer restore unit in Neon than the whole branch. Tenant-level restore must be built in the application.
- A cell on Neon could be its own project or branch. Moving a tenant into it is the same filtered copy as a restore, so building one tool serves both.

**Gaps::**

- Neon snapshot features and their retention were not covered in detail by the pages fetched.
- Neon support for row-filtered logical replication as a source was not verified.
