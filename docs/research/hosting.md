# Hosting and deploy research

Research for [#18](https://github.com/heavymedl-labs/tetsu/issues/18), following
[ADR-0010](../adr/0010-evaluating-technology.md). This is research, not a decision.
Where it disagrees with an ADR, the ADR wins. Shortened 2026-11-06. The full version is at
`git show f46cbef:docs/research/hosting.md`.

## Criteria

Written 2026-11-04, before any option was looked at. Hosting is expensive to
reverse and sits on the hot path, so the full process applies.

### Load

These are estimates from the [reference businesses](../reference-businesses/README.md),
not measurements.

| Measure | Estimate | Basis |
| --- | --- | --- |
| Check-in rush | Under 1 request per second | 104 seats at Anvil Row's 6:00 a.m. classes, about 5 requests per customer, over 30 minutes |
| Booking opening | About 50 requests per second for a minute | 104 seats, three customers per seat, 10 requests each, in the minute booking opens |
| Design peak | 100 requests per second | Twice the booking burst, for both businesses and margin |
| Customers | About 4,200 | 1,800 at Anvil Row and 2,400 at Juniper Lane |
| Database | Under 10 GB through milestone 3 | Includes Anvil Row's three years of visit history |
| Files | Under 50 GB through milestone 3 | Mostly signed waivers |

Compute will not separate the options. Our benchmarks served far more than this on
a laptop. What separates them is the database round trip, failure handling, and
what they ask of a very small team.

### Limits set by the maintainer

- **Budget:** under $150 a month through milestone 1, production and staging
  together.
- **Operations:** as little as possible before Anvil Row goes live. The provider
  patches the OS and Postgres, runs backups, and handles failover.
- **Hosted tests:** research and local tests produce a shortlist. The maintainer
  then creates accounts for one or two finalists, and the hosted tests run there.

### Limit changed after the first shortlist

Added 2026-11-04 by the maintainer, after the Render and AWS shortlist below, so it did
not come before those results. This first phase is for experimenting and should cost
as close to nothing as possible, using free tiers and moving to paid tiers as needed.
The $150 limit now applies from that move. The must haves still describe what we need
before a real business runs on Tetsu. The free phase needs only M1, M2, M3, M4, M5,
M7, and M10, because it holds no real customer data.

The maintainer later set benchmarking aside for this phase: it matters once real
businesses use Tetsu, not while we are getting something running.

### Must have

An option that fails one of these is out.

| # | Criterion | Why |
| --- | --- | --- |
| M1 | Runs our OCI image unchanged, with configuration from the environment | [ADR-0003](../adr/0003-architecture.md) |
| M2 | Nothing Tetsu needs is specific to the provider. We can leave with a new deploy target, `pg_dump` or a physical copy, and an S3 copy | ADR-0003; Rowan left an incumbent over lock-in |
| M3 | Postgres 18 with row-level security, a non-owner application role, and `set_config` | [#8](https://github.com/heavymedl-labs/tetsu/issues/8), and the version our failure tests ran on |
| M4 | A direct, session-level Postgres connection for the migration runner | [ADR-0011](../adr/0011-server-stack-guardrails.md) |
| M5 | Encrypted connections to Postgres, or a private network the provider documents as encrypted | Tenant data crosses it; settles the open item on #18 |
| M6 | Point-in-time recovery to within minutes, or a way to run our own | A lost day of bookings or payments is unacceptable ([#27](https://github.com/heavymedl-labs/tetsu/issues/27)) |
| M7 | US region within reach of Denver and Phoenix, with the app and Postgres in one site | The app makes at least one database round trip per request |
| M8 | Rolling deploys that drop no requests, health checks, and a way to run a job that must succeed before the rollout | Rowan would leave over an outage during the morning rush |
| M9 | Rollback to the previous image in minutes | Expand and contract keeps old code working against the new schema ([ADR-0007](../adr/0007-server-stack.md)) |
| M10 | Deploys from GitHub Actions, with secrets kept out of the repository | [ADR-0001](../adr/0001-how-we-work.md) |
| M11 | TLS on our domains, renewed automatically | The marketing site, and SAQ A scope for payment pages ([ADR-0004](../adr/0004-security.md)) |
| M12 | S3-compatible object storage, encrypted at rest | ADR-0003 |

### Weighed

These are compared, not pass or fail.

| # | Criterion | How it is judged |
| --- | --- | --- |
| W1 | Operating burden | Who patches the OS and Postgres, who tests failover, and what wakes someone at night, for a one-person team with agents. Weighs most, per the limit above |
| W2 | Monthly cost | Milestone 1 (marketing site, login, staging), within the limit above, and milestone 3 (two live businesses, high availability) |
| W3 | Availability | What one instance, host, or zone failing does, and how long recovery takes without a person |
| W4 | Database latency | Round trip from app to Postgres, and whether connections pass through a pooler |
| W5 | Cost to leave | Hours to move, using a drill rather than a guess where possible |
| W6 | Provider risk | Ownership, funding, recent outages, price changes, and whether the service is still sold to new customers |
| W7 | Security and compliance | Isolation between customers, audit reports, a data processing agreement, and short-lived deploy credentials through OIDC |
| W8 | Agent fit | A complete CLI or API, and configuration kept in the repository |
| W9 | Staging | Cost and effort of a second environment holding both reference businesses |
| W10 | Room to grow | What changes at ten or a hundred businesses, and whether it is a migration or a setting |

### Not decided here

- Recovery targets, restore testing, alerting, and on-call belong to #27. This
  decision must only keep them possible.
- Postgres TLS crate choice follows this decision. It is now [ADR-0013](../adr/0013-postgres-tls.md).

## Where it stands

The decision is [ADR-0012](../adr/0012-first-hosting-and-deploys.md).

Read 2026-11-04. **Start on Render's free web service and Neon's free Postgres, at no cost
and with no card on Render, then [move to a paid plan](../adr/0012-first-hosting-and-deploys.md#moving-to-paid)
before a real business depends on Tetsu.** Render leads among the paid options, so moving may not change the app's host.
None of this has run yet, so both choices are provisional, and the usage and health
evidence is thin.

The main tradeoff between the paid options is that Render asks least of a one-person
team, while AWS is safer for data. RDS Multi-AZ replicates synchronously. Render's
standby is asynchronous, so a failover can lose a few seconds of acknowledged writes.
The failover test settles this when we move.

## Free phase

| Option | Result | Reason |
| --- | --- | --- |
| Render free web service, app | **Pick** | Runs our image. With no payment method it suspends rather than bills, so the cost is a true $0. Moving to a paid Render plan needs no new host. Sleeps after 15 minutes, takes about a minute to wake, runs on 0.1 CPU, and is not meant for production |
| Neon, Postgres | **Pick** | Postgres 18.6, 1 GB per project, a direct endpoint, and no weekly pause. Sleeps after 5 minutes, 100 CU-hours a month, 6-hour restore window |
| Cloudflare R2, files | Pick when needed | 10 GB free, with no outbound fees. The marketing site and login store no files, so this can wait. Developer guides disagree on whether enabling it needs a payment method, so check first |
| Cloudflare Workers, app | Out on M1 | Free for 100,000 requests a day, with 10 ms of CPU each. Rust runs as WebAssembly, so our binary and its Axum and Tokio stack would not run unchanged |
| Cloudflare Containers, app | Out, not free | Needs the $5 Workers Paid plan, and a Worker starts and routes to each container. Worth a look as a paid option: no egress fees, and about $12 a month always on at 1/4 vCPU and 1 GiB by our arithmetic, plus active CPU. We have not confirmed connections to outside Postgres |
| Cloudflare CDN, in front of the app | To try | Free DNS, TLS, and cache rules. Caching marketing pages could hide Render's slow wake. One developer reports free accounts do not reliably cache HTML, so test it. Login and any tenant page must never be cached |
| Google Cloud Run | Fallback | Wakes faster, with real CPU and a larger free quota: 2 million requests and 180,000 vCPU-seconds a month. Needs a billing account and has no spending limit of its own. It was the pick while we planned to benchmark in this phase |
| Render free Postgres | Out | Expires after 30 days, with no backups |
| Supabase | Out on M3 | Hosted projects are on Postgres 17, with no 18 announced. 500 MB, and free projects pause after a week. It is a database and backend, not a host for our image; its Compute product is in private alpha. Point-in-time recovery costs $100 a month. Its auth overlaps [#9](https://github.com/heavymedl-labs/tetsu/issues/9) |
| Firebase | Out on M1, M3, and M4 | Firestore is not Postgres. Its Postgres product is reached through a GraphQL API rather than SQL, the free trial lasts 3 months, and containers need the paid plan |
| Vercel | Out on M1 and on benchmarking | Hobby is non-commercial only. Containers are in beta and stop after 5 idle minutes. Load testing is allowed only on Enterprise |
| Koyeb | Out | No free web service, and the company is joining Mistral AI |
| Oracle Always Free | Out on operations | A VM we would run ourselves. Idle instances can be reclaimed, and reports of reduced limits conflict |
| AWS free plan | Out | The account closes after 6 months or when credits run out |
| Google e2-micro | Out on operations | A free VM, but we would run Postgres ourselves |

Limits of this pick:

- **Slow first visit.** A visitor after 15 idle minutes waits about a minute. That is fine
  while we build, but not once the marketing site is promoted. Then it needs a paid
  instance, $7 a month at the smallest.
- **Heavy traffic.** Render may suspend a free service for unusually high outbound traffic,
  including database use. Its 750 free instance hours are shared across the workspace.
- **Migrations.** Free services have no one-off jobs, so migrations run from GitHub Actions
  against Neon's direct endpoint. We assume the pre-deploy command is also unavailable.
- **Deploying the image.** Render's docs on prebuilt images name no plan limit, which we
  confirm on the first deploy.
- **Neon cold starts.** The database also sleeps, which adds to the first request.
  Neon says waking takes a few hundred milliseconds, reviews say 500 ms to 2 seconds, and
  one reviewer measured a 1.8-second median and a 3.1-second worst case over 200 samples. Fine for a
  marketing site, and to be measured.
- **Network.** The app and database are at different providers, because Render's free
  Postgres expires after 30 days. Each query crosses
  an encrypted public connection, so choose Render's Ohio region and Neon's `aws-us-east-2`.
- **Not yet checked.** Whether Neon's free plan allows commercial use (its terms and plans
  page do not say), and how many custom domains Render's free services allow beyond the
  2 on Hobby. A domain is the one likely cost.
- **Encrypted connections from day one.** Neon needs TLS, which [ADR-0013](../adr/0013-postgres-tls.md)
  settles.

### Free phase cost

Rates read on 2026-11-04. The totals are our arithmetic.

**The pick costs nothing, and with no card on Render it cannot start billing.** Neon's free
plan suspends the database at its limit. Cloud Run, the fallback, has no spending limit, and
sustained traffic costs money: about $70 a month at 10 requests per second, before outbound
data. If we ever use it, use a dedicated project, a budget of a few dollars, and Google's
function that removes billing.

| Past the free tier | Monthly cost |
| --- | --- |
| Neon Launch, 0.25 CU always on, 5 GB | About $21, no SLA |
| Neon Launch, 1 CU always on, 10 GB | About $81, no SLA |
| Render Starter, 512 MB, always on | $7 |
| Cloud Run, 1 vCPU and 512 MB always on, instance billing | About $45 |

Neon offers an SLA only on Scale, about $162 a month for 1 CU by a third-party review, so
staying on Neon is not clearly cheaper than the finalists once we need one.

Neon's 100 CU-hours cover about 400 hours at 0.25 CU, or 55% of a month. An uptime monitor
that wakes the database more often than every 5 minutes would use them up, and the
database then stays suspended until the next month. [#27](https://github.com/heavymedl-labs/tetsu/issues/27)
needs to allow for this.

### Free phase usage and health

The evidence is weak to moderate, and it was gathered while Cloud Run was the pick. Render's own
record is under [Weighed criteria](#weighed-criteria). We found no first-hand report of Axum or
`tokio-postgres` on Cloud Run with Neon, and no reliable adoption figure. Neon has been owned by Databricks
since 2025. It had two outages totaling 5.5 hours in May 2025, and slow compute starts in Ohio on
2 November 2026, per a tracker. R2 had about an hour down on 6 February 2025, and has no
versioning or object lock.

Three of these affect our design:

- **Background work.** A free Render service stops after 15 idle minutes, so a loop that delivers
  outbox events can stall, and the walking skeleton has to prove domain events.
  [#11](https://github.com/heavymedl-labs/tetsu/issues/11) must allow for this, for example by
  delivering when traffic wakes the service, until a paid instance runs all the time.
- **Connecting on a cold start.** A Neon forum thread reports `tokio-postgres` and SQLx
  hanging indefinitely on cold starts, with a connect timeout not helping. We do not know
  its age or outcome. Since ADR-0011 already found that `connect_timeout` does not cover a
  stalled server, the pool's `create` timeout has to bound this. The hosted tests cover it.
- **Files later.** #27 lists versioning as an option for protecting files, which counts against
  keeping R2 for signed waivers. We also did not confirm presigned URLs on R2's S3 API page.

### Agent tooling

Read 2026-11-04. Both have an official CLI and an official hosted MCP server, so agents can
work with them directly. Adopting any of these still needs the steps in
[ADR-0010](../adr/0010-evaluating-technology.md).

| | Render | Neon |
| --- | --- | --- |
| **CLI** | `render`: JSON output, `RENDER_API_KEY`, `--confirm`, and `deploys create --image --wait` | `neon`: JSON output, `NEON_API_KEY`, and projects, branches, and connection strings |
| **MCP server** | `mcp.render.com/mcp`, OAuth or API key. Creates services, updates environment variables, triggers deploys, reads logs and metrics, and runs read-only SQL | `mcp.neon.tech/mcp`, OAuth or API key. Manages projects and branches, and runs SQL. `?readonly=true` disables writes |
| **MCP limits** | Cannot delete anything or create image-backed services, which is how we deploy | Neon says it is for development and testing only, and never for production databases |
| **Other** | Official skills, a Claude Code plugin, and docs as Markdown | Guided setup through `npx neon@latest` |
| **Narrow credentials** | Scoped keys, in early access: one workspace, with permissions by resource type, including read-only. No limit to one service. Docs do not list plans | One-project keys with Editor access, created by an organization admin |

Project health, read 2026-11-04. All are open source and maintained by the provider.

| Project | License | Latest release | Commits in the last year | Notes |
| --- | --- | --- | --- | --- |
| [Render CLI](https://github.com/render-oss/cli) | Apache-2.0 | v2.35.0, 2 November 2026 | Led by bots and one engineer | 38 open issues |
| [Render MCP server](https://github.com/render-oss/render-mcp-server) | Apache-2.0 | v0.3.0, January 2026 | Led by a bot and two engineers | 13 open issues, and updated since its last release |
| [Neon CLI](https://github.com/neondatabase/neon-pkgs) | Apache-2.0 | `neon` 8.18.1 on npm, November 2026 | Not read in its new home | Moved from the now archived `neonctl` repository, so installs should use `neon` |
| [Neon MCP server](https://github.com/neondatabase/mcp-server-neon) | MIT | None published | Mostly one engineer | 46 open issues |

We did not check security advisories for these tools. The hosted MCP servers run on the
providers' systems, so the releases above may not match what they serve.

How we would use them:

- **CI deploys through the CLI,** with a pinned version. It handles image deploys and fails
  the job when a deploy fails.
- **MCP is for development.** Render's reads logs and metrics well, so give it a read-only
  key where possible. Its documented writes, such as environment variables and deploys, can reach production. Neon's stays on
  a development project or in read-only mode.
- **Secrets can reach the agent.** Render warns that connection strings may enter an agent's
  context, and Neon's `get_connection_string` returns them. So the free phase holds only
  fictional data, and real data needs a stricter rule first.
- **Prompt injection is a known MCP risk.** One forum comment reports an exploit in Neon's
  MCP server, which we could not confirm or date. Agents read logs and rows that outsiders
  can influence, and that content must be treated as data.

## Benchmarking findings

The rule is in [ADR-0012](../adr/0012-first-hosting-and-deploys.md#benchmarking). What shaped it:

- **Provider rules.** Vercel allows load tests only on Enterprise, and Supabase asks Team
  and Enterprise customers for two weeks' notice of heavy tests. We found no policy for
  Render. AWS has stress test rules for EC2. Check before running on any new host.
- **Load generator.** A 2-CPU GitHub runner may hit its own limit first
  ([ADR-0008](../adr/0008-benchmark-harness.md)), so each result must show it was not the
  limit.

## Paid options

| Option | Result | Reason |
| --- | --- | --- |
| Render, with R2 or S3 for files | Finalist | Lowest operating burden that passes every must have on paper |
| AWS ECS on Fargate, with RDS and S3 | Finalist | Synchronous standby, OIDC deploys, and the most portable parts, for more setup |
| Google Cloud Run with Cloud SQL | Held back | Close to AWS. Cloud SQL's shared-core machines have no SLA, and our prices for it are the weakest here. Next in line if a finalist fails |
| DigitalOcean App Platform with Managed Postgres | Out on cost | About $60 at milestone 1, but new Standard clusters lose standby nodes on 30 November 2026. High availability then means Advanced Edition, from $260 |
| Fly.io with Managed Postgres | Out on M3 | Offers Postgres 16 or 17, with no in-place upgrade. High availability from $38 is the best price here, so recheck if it adds 18 |
| Railway | Out on the operations limit | Staff replies describe its databases as unmanaged, and recovery features must be turned on first |
| Self-run VM, such as Kamal with pgBackRest | Out on the operations limit | The open source baseline, and the cheapest. We would own patches, backups, and failover. ADR-0003 keeps it open as an exit |
| Kubernetes, managed or k3s | Out on W1 | Much more to run than one image needs |
| Neon or Crunchy Bridge with a separate app host | Not evaluated as a paid setup | Adds a second vendor between the app and its database. We will learn from Neon in the free phase |

ADR-0010 asks for a stated reason to use a proprietary option: the maintainer's limit
on operations. Both finalists run plain Postgres and our own image, so leaving stays a
matter of moving data.

## Must haves for the finalists

| # | Render | AWS |
| --- | --- | --- |
| M1 image | Yes, from a registry | Yes |
| M2 no lock-in | Yes. External URL, `pg_dump`, and logical replication | Yes |
| M3 Postgres 18 | Yes, 13 through 18 | Yes, 18.6 listed |
| M4 direct connection | Yes. The pooler is optional | Yes. No pooler unless we add one |
| M5 encrypted connections | Yes, but internal TLS is optional and self-signed, so `sslmode=require` with no certificate check | To confirm in testing |
| M6 recovery | Yes. 7 days on Pro, but not the last 10 minutes, and into a new instance | To confirm in testing |
| M7 region | Oregon, Ohio, or Virginia | Oregon or Ohio, among others |
| M8 deploys | Yes. Zero-downtime, and a failed pre-deploy command stops the deploy | Rolling deploys. The migration job is a pipeline step we write |
| M9 rollback | Yes, to a recent deploy, pinned by digest | To confirm in testing |
| M10 GitHub Actions | Yes, through a deploy hook URL or API key, both long-lived secrets | Yes, through OIDC with no stored keys |
| M11 TLS | Yes, automatic, 15 domains on Pro | Yes, through the load balancer |
| M12 files | Not offered, so a second vendor | Yes, S3 |

## Cost

Our estimates at list price, per month. Nothing here has been billed.

| Stage | Render | AWS (Ohio) |
| --- | --- | --- |
| Milestone 1, production and staging, no high availability | About $75 | About $100 |
| Milestone 3, two businesses, high availability | About $210 | About $190 |

Both stay under the $150 limit, which applies through milestone 1.

AWS's lower figure at milestone 3 comes with less memory per task and a burstable
database, so the two are not exact equivalents.

## Weighed criteria

| # | Render | AWS |
| --- | --- | --- |
| W1 operating burden | Low. No network, load balancer, or IAM to run | Moderate. A VPC, security groups, IAM, and a load balancer, kept in infrastructure code that needs its own ADR. ECS Express Mode reduces this, but it is new |
| W3 availability | Failover starts after 30 seconds and takes a few seconds. Can lose a few seconds of writes | Synchronous standby in a second zone. Failover time to be measured |
| W4 database latency | Same-region private network, to be measured | Same-region or cross-zone, to be measured |
| W6 provider risk | Private. Raised $100M at $1.5B in February 2026. A tracker lists 77 incidents in 12 months, including a 53-minute high-availability Postgres outage on 7 May 2026 | Very low risk of closure, but services do close: App Runner stopped taking new customers on 30 April 2026 |
| W7 compliance | SOC 2 Type 2 report on Pro, with an NDA. DPA for everyone. No uptime SLA below Enterprise | The broadest set, to be confirmed for the services we use |
| W8 agent fit | CLI, API, Blueprint file, MCP server, and skills. See [Agent tooling](#agent-tooling) | CLI, API, and infrastructure code. MCP tooling not evaluated |
| W9 staging | Two small services, about $13 | About $21, sharing the load balancer |
| W10 room to grow | A larger plan is a setting | A larger instance is a setting, with more options past that |

W2 is under [Cost](#cost). W5, the cost to leave, needs the drill below.

Incident counts come from third-party trackers that count differently. We looked for a
comparable AWS figure and found no incident history for ECS or RDS in Ohio. The best known
recent AWS failure was in `us-east-1` on 19 and 20 October 2025, lasting over 15 hours. So
the figures show that both have real incidents, not that either is more reliable.

## Community practice

The evidence is weak: most comparisons were written by vendors selling an alternative, and we
found no independent data on managed Postgres reliability. Common advice is a platform like
Render before revenue, then ECS with RDS as a business grows, which fits starting on Render and
keeping the option to move. Two 2026 Railway forum threads report data loss with no backups
enabled and no recovery path.

## Failure tests

Run 2026-11-04 on PostgreSQL 18.6 and PgBouncer 1.24.0 in local containers, once per
setting: 8 clients, 16,000 transactions, and a server pool of 3. The hosts may run a
different PgBouncer version.

| Test | Direct | Transaction pooling |
| --- | --- | --- |
| Cached statements, `max_prepared_statements=0` | No errors | 85% of transactions failed: `prepared statement "s3" does not exist` |
| Cached statements, `max_prepared_statements=200` | No errors | No errors |
| Tenant set with `set_config(..., true)`, read in the transaction | Never wrong | Never wrong |
| Tenant read outside a transaction | `NULL` | `''`, as #8 found on reused connections |
| Session advisory lock | Held and released | Unsafe. In one of two runs the unlock reached another connection and the lock stayed held |
| `statement_timeout` as a startup option | Works | Refused: `unsupported startup parameter` |
| `SET statement_timeout` | Works | Applied to some queries only, and **leaked to another client** |
| `SET LOCAL statement_timeout` | Works | Works |

**Connect directly, with no pooler in between.** [ADR-0011](../adr/0011-server-stack-guardrails.md)
requires a timeout on every connection, and the simplest way to set one breaks behind a
transaction pooler. Render's pooler is off unless enabled, and it runs in transaction
mode. Its docs do not state a `max_prepared_statements` value. At our load we do not
need one: a few instances with small pools stay well under the 100 connection limit.

## Hosted tests still to run

Only tests 8 and 9 run in the free phase, as part of the first deploy. Tests 1 to 8 run on both paid finalists before we move, with accounts the maintainer
creates then. Expect under $20 if the resources are deleted afterward.

1. **Database round trip,** from app to Postgres, with the
   [benchmark harness](../../benchmarks/README.md) at the 100 requests per second peak.
2. **Rolling deploy under load:** count dropped requests.
3. **Failed migration job:** confirm the rollout stops and traffic stays on the old image.
4. **Rollback:** time it, by digest.
5. **Failover under write load:** measure downtime, count acknowledged writes lost, and
   check that our `connection closed` handling recovers. This is the deciding test.
6. **Point-in-time restore:** measure the time, and how close to the failure it can reach.
7. **Exit drill:** move the database out with `pg_dump` and time the move.
8. **Connection details:** TLS with `tokio-postgres`, and the migration runner's lock on a
   direct connection. The application role under row-level security is checked in
   [#8](https://github.com/heavymedl-labs/tetsu/issues/8)'s work, once it decides isolation.
9. **Waking from idle, free setup only:** connect after each sleep, at least 50 times.
   Record the time, any hang, and whether the pool's `create` timeout stops it. Also check
   that pooled connections recover after the service wakes.

## First deploy, 2026-11-09

The free setup went live on 2026-11-09 ([demo environment](../operations/demo-environment.md)). Findings so far:

- **Render runs the image behind Cloudflare** (`server: cloudflare`). A 70 KB header was dropped at the edge before
  any response. A 20 KB header or path, and a malformed header line, which the edge cleaned up, reached `tetsu`
  and came back with its security headers. So the 400, 414, and 431 that hyper answers before our headers, noted in
  the review of [#76](https://github.com/heavymedl-labs/tetsu/issues/76), were not seen through Render.
- **Render's command replaces the image's entrypoint,** not only its `CMD`: `serve` exited with status 128, and
  `/usr/local/bin/tetsu serve` runs.
- **Test 8, connection details:** `tetsu migrate` from a laptop and from Actions reached Neon's direct
  endpoint over TLS with the public roots and `channel_binding=require`, and took the migration lock. The
  application role under row-level security moved to [#8](https://github.com/heavymedl-labs/tetsu/issues/8), and
  passed on 2026-11-12 ([#106](https://github.com/heavymedl-labs/tetsu/issues/106)): after the demo moved onto
  `tetsu_owner`, the `check_isolation` example found no isolation problem on Neon's PostgreSQL 18.6, so no
  host-made object fell outside ADR-0042's allowlist.
- **A deploy** bills 33 seconds of job time: finding the CI run's digest, pulling, migrating, and waiting on
  Render, whose own deploy took about 20 seconds.
- **Test 9, waking from idle,** ran 50 times from 2026-11-10 00:16 to 14:23 UTC: a request to `/anvil-row` after
  17 idle minutes each time, so both Render and Neon had slept, then a second request. Each waited at most 120
  seconds.
  - 49 woke with a 200 in a median of 12.3 seconds, Neon's wake included; the slowest took 21 seconds. The
    request after each took 0.15 seconds, so the new process's pool worked at once.
  - Once, Render took 8 minutes to start the instance: it stopped at 11:59:54, the request came at 12:01:54, and
    its log shows `serving` at 12:09:54. Both requests timed out, and nothing reached `tetsu`.
  - No hang reached the database, and no 503 came back, so the pool's `create` timeout never had to act. That
    leaves ADR-0012's revisit condition unmet. The wake was faster than its "about a minute" expected.

## Sources

Read 2026-11-04. Sources for most options we ruled out are in the full version.

- Render docs: [deploys](https://render.com/docs/deploys), [prebuilt images](https://render.com/docs/deploying-an-image),
  [rollbacks](https://render.com/docs/rollbacks), [connecting to Postgres](https://render.com/docs/postgresql-creating-connecting),
  [connection pooling](https://render.com/docs/postgresql-connection-pooling),
  [high availability](https://render.com/docs/postgresql-high-availability),
  [backups](https://render.com/docs/postgresql-backups), [regions](https://render.com/docs/regions),
  [custom domains](https://render.com/docs/custom-domains), [pricing](https://render.com/pricing),
  and [free services](https://render.com/docs/free).
- Neon docs: [pricing](https://neon.com/pricing), [plans](https://neon.com/docs/introduction/plans),
  [regions](https://neon.com/docs/introduction/regions), [versions](https://neon.com/docs/postgresql/postgres-version-policy),
  [pooling](https://neon.com/docs/connect/connection-pooling), and [connection latency](https://neon.com/docs/connect/connection-latency).
- AWS: [ECS Express Mode](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/express-service-overview.html),
  [RDS Multi-AZ](https://docs.aws.amazon.com/AmazonRDS/latest/UserGuide/Concepts.MultiAZSingleStandby.html),
  [RDS PostgreSQL versions](https://docs.aws.amazon.com/AmazonRDS/latest/PostgreSQLReleaseNotes/postgresql-versions.html),
  [App Runner availability](https://docs.aws.amazon.com/apprunner/latest/dg/apprunner-availability-change.html),
  and the AWS price list API for Ohio. The IPv4 rate is from memory, not the API.
- Google Cloud: [Cloud Run pricing](https://cloud.google.com/run/pricing),
  [disabling billing with budgets](https://docs.cloud.google.com/billing/docs/how-to/disable-billing-with-notifications),
  [free features](https://docs.cloud.google.com/free/docs/free-cloud-features),
  and [third-party Cloud SQL prices](https://www.bytebase.com/dbcost/cloudsql-pricing/).
- Incidents: [Neon, May 2025](https://www.ilert.com/postmortems/neon-outage-may-2025),
  [R2, February 2025](https://blog.cloudflare.com/cloudflare-incident-on-february-6-2025/),
  [AWS, October 2025](https://www.infoq.com/news/2025/11/aws-dynamodb-outage-postmortem), and
  the [Neon tracker](https://statusgator.com/services/neon).
- Weaker sources: [Render's funding announcement](https://siliconangle.com/?p=731299) and
  the [Pingoru incident tracker](https://pingoru.io/providers/render/outage-history).
- GitHub: [OIDC in AWS](https://docs.github.com/en/actions/how-tos/secure-your-work/security-harden-deployments/oidc-in-aws).
- Cloudflare: [R2 pricing](https://developers.cloudflare.com/r2/pricing/) and [R2's S3 API](https://developers.cloudflare.com/r2/api/s3/api/).
- Agent tooling: [Render MCP server](https://render.com/docs/mcp-server), [Render CLI](https://render.com/docs/cli),
  [Render scoped API keys](https://render.com/docs/scoped-api-keys), [Neon MCP server](https://neon.com/docs/ai/neon-mcp-server),
  [Neon CLI](https://neon.com/docs/reference/neon-cli), and [Neon API keys](https://neon.com/docs/manage/api-keys).
- Single reports: [Neon cold start measurements](https://pickuma.com/for-dev/neon-serverless-postgres-review/),
  and the [Neon forum thread on Rust cold starts](https://community.neon.tech/t/non-deterministic-connection-hangup-with-rust-libraries-tokio-postgres-and-sqlx-when-connecting-with-neon-on-cold-start/940).
