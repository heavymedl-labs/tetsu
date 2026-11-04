# Hosting and deploy research

Research for [#18](https://github.com/heavymedl-labs/tetsu/issues/18), following
[ADR-0010](../adr/0010-evaluating-technology.md). This is research, not a decision.
Where it disagrees with an ADR, the ADR wins.

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
| W2 | Monthly cost | Milestone 1 (marketing site, sign-in, staging), within the limit above, and milestone 3 (two live businesses, high availability) |
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
- Postgres TLS crate choice follows this decision, with its own ADR if M5 needs one.
