# 0012. First hosting and deploys

- Status: Accepted
- Date: 2026-11-04

## Context

The marketing site needs a public host, and the walking skeleton needs a deploy path
([#18](https://github.com/heavymedl-labs/tetsu/issues/18)). Tetsu ships as one OCI image
with PostgreSQL and S3-compatible storage ([ADR-0003](0003-architecture.md)).

The maintainer set two limits. This first phase should cost as close to nothing as
possible, and the provider should do the operating work. No real business depends on
Tetsu until milestone 2, so this phase holds only fictional data and can accept what real
customers could not. The evidence is in [Hosting research](../research/hosting.md).

## Decision

**Start free, on fictional data, and move to paid before a real business depends on Tetsu.**

### The free phase

- **App: Render's free web service,** in Ohio, with no payment method on file, so it
  suspends rather than bills.
- **Database: Neon's free plan,** on Postgres 18 in `aws-us-east-2`. The app uses the direct
  endpoint with TLS and no pooler, because a transaction pooler breaks the connection
  timeouts [ADR-0011](0011-server-stack-guardrails.md) requires.
- **Files: none yet.** Neither the marketing site nor sign-in stores files. When something
  does, Cloudflare R2 is the leading option, through the S3 API.
- **Environments: one,** holding fictional tenants, including both reference businesses for
  walkthroughs. Staging comes with the move to paid.
- **Domain and TLS:** Render's automatic TLS on our domain, or its `onrender.com` address
  until the maintainer buys one.

### The deploy pipeline

On `main`, GitHub Actions:

1. builds and tests, and pushes the image to the GitHub Container Registry;
2. runs `tetsu migrate` against Neon's direct endpoint, and stops if it fails;
3. deploys that image by digest with a pinned Render CLI, and fails if the deploy fails.

Rollback deploys the previous digest, which works because schema changes follow expand and
contract. Secrets stay out of the repository: GitHub Actions holds the deploy key and
migration URL, and Render holds the app's database URL and a registry read token. Keys are
as narrow as each provider allows.

`tetsu migrate` is the runner [ADR-0007](0007-server-stack.md) chose, run as its own command.
Choosing a job fires ADR-0011's trigger to revisit that runner, and we keep it. goose's
polling lock is only a Go library option, which would add Go to a Rust project. dbmate's lock
looks exposed to the deadlock ADR-0011 rules out. Migrations also ship inside the image, so
code and schema stay matched.

### Agent tooling

Agents use the Render and Neon CLIs. Their MCP servers are for development only: Render's
with a read-only key where possible, and Neon's on non-production data. Logs and rows that
agents read are data, never instructions. Real data needs a stricter rule first, because
connection strings can reach an agent's context.

### Moving to paid

Move when any of these is true:

- A real business is about to store customer data or take payments.
- The marketing site is promoted, so a slow first visit matters. A $7 instance solves this
  alone.
- We reach a free limit two months running.
- Sleeping or the 0.1 CPU limit gets in the way of something we need to show.

Before a real business moves, a new ADR chooses the paid setup, with Render leading and AWS
as the alternative. It needs the hosted tests in the research page, including failover under
write load, and all [must haves](../research/hosting.md#must-have) met. The $150 monthly
limit then applies.

### Benchmarking

None is required in the free phase. From the move to paid, each host gets two runs, and a
real business does not move until a capacity run has passed:

- **Capacity:** on a new host, plan, size, or region, and when a hot path changes. Raise the
  load until the latency budget breaks, and record the peak and what ran out first. Once
  real customers exist, this runs on matching staging, not production.
- **Check:** on every deploy. About a minute at a low fixed rate, compared with the last
  deploy on that host.

## Alternatives considered

- **Google Cloud Run with Neon.** Wakes faster, with real CPU and a larger free quota. It
  needs a billing account and has no spending limit of its own. It was the pick until
  benchmarking moved out of this phase, and is the fallback.
- **Paid from the start, on Render or AWS.** About $75 to $100 a month before any customer,
  buying failover and uptime nothing yet needs.
- **Supabase.** Hosted projects are on Postgres 17, and it cannot host our image.
- **Firebase.** Firestore is not Postgres, and its Postgres product is reached through
  GraphQL rather than SQL.
- **Vercel.** The free plan is non-commercial, and containers are in beta and stop when idle.
- **Cloudflare Workers.** Rust runs as WebAssembly, so our binary would not run unchanged.
  Containers need a paid plan.
- **Fly.io.** Offers Postgres 16 or 17, with no in-place upgrade, and no free plan.
- **Render's free Postgres.** Expires after 30 days, with no backups.
- **A self-run VM.** The open source baseline. ADR-0010 asks for a stated reason to use
  proprietary options: the maintainer does not want to run servers yet. Our image and plain
  Postgres keep this open.
- **One deployment per business.** Strong isolation at about $29 to $172 per business. This
  belongs to [#8](https://github.com/heavymedl-labs/tetsu/issues/8), where the maintainer's
  direction is shared now, with cells later.

## Consequences

The first deploy costs nothing, apart from any domain. Render says free instances are not
for production, and accepting that is why real data stays off them.

**Tradeoffs.** A visitor after 15 idle minutes waits about a minute, and Neon's wake adds to
that. Each query crosses an encrypted public connection between providers. Free services stop
when idle, so [#11](https://github.com/heavymedl-labs/tetsu/issues/11) must not assume a
loop that always runs. Neon's 100 CU-hours rule out uptime checks that wake the database
more often than every 5 minutes, which [#27](https://github.com/heavymedl-labs/tetsu/issues/27)
must allow for. The registry read token is long-lived, because the repository is private.

**Weakest evidence.** Nothing has run yet. These choices rest on provider docs, and on thin
usage and incident data, much of it from vendors and trackers. We could not confirm whether
Neon's free plan allows commercial use, or that prebuilt images deploy on Render's free plan.

**Cost to replace.** Low. The app is one image, and the data is plain Postgres that moves with
`pg_dump`. We estimate hours, not days, with fictional data. The pipeline's deploy step is
the only Render-specific code.

**Next.**

- A Postgres TLS crate needs its own ADR before the first deploy, because Neon requires TLS.
- The first deploy runs the connection and wake tests in the research page, and shows whether
  a free Cloudflare cache can hide the slow wake. Adopting that needs its own check.

**Revisit** the runner if a runner bug fails a deploy, as ADR-0011 says. goose is the
alternative, and plain SQL migrations would mostly need its annotations added. This rests on
earlier research and the tools' READMEs, with no new tests, and the runner is not built yet.

**Revisit** hosting when a [move to paid](#moving-to-paid) condition is met, or if the wake tests show
hangs that the pool's `create` timeout cannot stop, or if either provider changes these free
terms.

## Sources

- [Hosting research](../research/hosting.md): criteria, options, costs, usage, incidents, and
  failure tests, 2026-11-04, with links to each provider's documentation.
- AWS SaaS Lens: silo and pool isolation.
- PgBouncer documentation: `max_prepared_statements` and transaction pooling.
