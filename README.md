<h1 align="center">
  <br>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/brand/tetsu-wordmark-dark.svg">
    <img alt="Tetsu" src="docs/brand/tetsu-wordmark-light.svg" width="360">
  </picture>
  <br>
  <br>
</h1>

**Booking software, built like iron.** One platform for businesses that sell time: studios, salons, courts, and
campgrounds. Scheduling, memberships, and payments, on published prices, with the payment processor you choose.

Tetsu (a working name, Japanese for iron) is written in Rust. Every business is a tenant on one shared platform,
isolated in the database itself. Industries add only what is theirs; the platform never names one.

## Where it stands

Early, and deployed. The live demo at <https://tetsu-demo.onrender.com> serves fictional businesses' pages and
Tetsu's own. Built so far:

- **Tenancy:** each business's address and page, kept apart by row-level security
- **Audit:** a record of security events that is never lost
- **Domain events:** delivered once through an outbox, retried, and logged when they stop
- **Operations:** JSON logs with alert fields, ready to export to any OTLP backend

Sign-in is next ([#20](https://github.com/heavymedl-labs/tetsu/issues/20)), then bookings and payments. The
[milestones](https://github.com/heavymedl-labs/tetsu/milestones) hold the roadmap, and the
[project board](https://github.com/orgs/heavymedl-labs/projects/1) the work in flight.

## Run it

With the test Postgres from [AGENTS.md](AGENTS.md#checks) running, copy `.env.example` to `.env` once, then:

```bash
cargo run -- migrate
cargo run -- serve
```

[crates/tetsu](crates/tetsu/README.md#run-locally) has the one-time setup, and `scripts/check.sh` runs every check.

## Runs on

Free tiers until a client showcase is close. Every provider sits behind a setting or a port, so each can be
swapped. Setup and IDs are in [the demo runbook](docs/operations/demo-environment.md).

| What | Provider | Plan | Status | Decision |
| --- | --- | --- | --- | --- |
| App | [Render](https://render.com), web service in Ohio | Free, sleeps when idle | Live | [ADR-0012](docs/adr/0012-first-hosting-and-deploys.md) |
| Database | [Neon](https://neon.com), Postgres 18 in `aws-us-east-2` | Free | Live | [ADR-0012](docs/adr/0012-first-hosting-and-deploys.md) |
| Code, CI, images | [GitHub](https://github.com/heavymedl-labs/tetsu): Actions, Container Registry | Free | Live | [ADR-0023](docs/adr/0023-ci-on-demand.md), [ADR-0033](docs/adr/0033-deploys-on-demand.md) |
| Logs and alerts | [Grafana Cloud](https://grafana.com/products/cloud/) over OTLP | Free | Being set up | [ADR-0045](docs/adr/0045-operations-baseline.md), [ADR-0049](docs/adr/0049-log-export.md) |
| Email | [Resend](https://resend.com) over SMTP | Free | Chosen, arrives with sign-in | [ADR-0044](docs/adr/0044-email-delivery.md) |

## Find your way

- [ARCHITECTURE.md](ARCHITECTURE.md): the system's shape
- [docs/adr](docs/adr/README.md): every decision and why
- [CONSTRAINTS.md](CONSTRAINTS.md): the quality bar
- [docs/glossary.md](docs/glossary.md): the words we use
- [docs/reference-businesses](docs/reference-businesses/README.md): the fictional businesses we build against
- [docs/research](docs/research/market-scan/README.md): the research behind the decisions
- [CONTRIBUTING.md](CONTRIBUTING.md) and [AGENTS.md](AGENTS.md): how work gets done, by people and agents
- `.config/`: settings only our scripts read. Settings editors also read stay at the root.
