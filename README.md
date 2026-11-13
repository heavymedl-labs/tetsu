<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/brand/tetsu-wordmark-dark.svg">
    <img alt="Tetsu" src="docs/brand/tetsu-wordmark-light.svg" width="320">
  </picture>
</h1>

[![CI](https://github.com/heavymedl-labs/tetsu/actions/workflows/ci.yml/badge.svg)](https://github.com/heavymedl-labs/tetsu/actions/workflows/ci.yml)
[![Demo: live](https://img.shields.io/badge/demo-live-E5622B)](https://tetsu-demo.onrender.com)
![Rust 1.99](https://img.shields.io/badge/Rust-1.99-3A4250?logo=rust)
![Postgres 18](https://img.shields.io/badge/Postgres-18-3A4250?logo=postgresql&logoColor=white)
[![Status: early](https://img.shields.io/badge/status-early-3A4250)](https://github.com/orgs/heavymedl-labs/projects/1)

**Booking software, built like iron.** Scheduling, memberships, and payments for businesses that sell time.

Tetsu (a working name, Japanese for iron) is one Rust platform for studios, salons, courts, and campgrounds. Every
business is a tenant, isolated in the database itself, and industries add only what is theirs.

## Built so far

- **Tenancy:** each business's address and page, kept apart by row-level security
- **Audit:** a record of security events that is never lost
- **Domain events:** delivered once through an outbox, retried, and logged when they stop
- **Operations:** JSON logs with alert fields, exported to any OTLP backend

Next: sign-in ([#20](https://github.com/heavymedl-labs/tetsu/issues/20)), then bookings and payments.

## Run it

With the test Postgres from [AGENTS.md](AGENTS.md#checks) running:

```bash
cp .env.example .env
cargo run -- migrate
cargo run -- serve      # http://localhost:8080
```

One-time setup is in [crates/tetsu](crates/tetsu/README.md#run-locally). `scripts/check.sh` runs every check.

## Runs on

Free tiers for now, each behind a setting or a port so it can be swapped
([runbook](docs/operations/demo-environment.md)).

| What | Provider | Status |
| --- | --- | --- |
| App | [Render](https://render.com) ([ADR-0012](docs/adr/0012-first-hosting-and-deploys.md)) | Live |
| Database | [Neon](https://neon.com), Postgres 18 ([ADR-0012](docs/adr/0012-first-hosting-and-deploys.md)) | Live |
| Code, CI, images | [GitHub](https://github.com/heavymedl-labs/tetsu) ([ADR-0023](docs/adr/0023-ci-on-demand.md), [ADR-0033](docs/adr/0033-deploys-on-demand.md)) | Live |
| Logs and alerts | [Grafana Cloud](https://grafana.com/products/cloud/) ([ADR-0049](docs/adr/0049-log-export.md)) | Being set up |
| Email | [Resend](https://resend.com) ([ADR-0044](docs/adr/0044-email-delivery.md)) | With sign-in |

[Architecture](ARCHITECTURE.md) · [Decisions](docs/adr/README.md) · [Quality bar](CONSTRAINTS.md) · [Glossary](docs/glossary.md) · [Contributing](CONTRIBUTING.md) · [Agents](AGENTS.md) · [Roadmap](https://github.com/heavymedl-labs/tetsu/milestones) · [Board](https://github.com/orgs/heavymedl-labs/projects/1)
