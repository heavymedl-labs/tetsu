<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/brand/tetsu-wordmark-dark.svg">
    <img alt="Tetsu" src="docs/brand/tetsu-wordmark-light.svg" width="320">
  </picture>
</h1>

<p align="center">
  <a href="https://github.com/heavymedl-labs/tetsu/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/heavymedl-labs/tetsu/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://tetsu-demo.onrender.com"><img alt="Demo: live" src="https://img.shields.io/badge/demo-live-E5622B"></a>
  <img alt="Rust 1.99" src="https://img.shields.io/badge/Rust-1.99-3A4250?logo=rust">
  <img alt="Postgres 18" src="https://img.shields.io/badge/Postgres-18-3A4250?logo=postgresql&amp;logoColor=white">
  <a href="https://github.com/orgs/heavymedl-labs/projects/1"><img alt="Status: early" src="https://img.shields.io/badge/status-early-3A4250"></a>
</p>

**Booking software, built like iron.** Scheduling, memberships, and payments for businesses that sell time.

Tetsu (a working name, Japanese for iron) is one Rust platform for studios, salons, courts, and campgrounds. Every
business is a tenant, isolated in the database itself, and industries add only what is theirs.

## Built so far

- **Tenancy:** each business's address and page, kept apart by row-level security
- **Audit:** a record of security events that is never lost
- **Identity:** operators, who join with a passkey from a one-time invite link and log in with it at `/admin`, with
  server-side sessions whose tokens are stored only as hashes
- **Domain events:** delivered once through an outbox, retried, and logged when they stop
- **Mail:** operators' invites and notices, sent by SMTP to listed addresses and otherwise kept for a mailbox page
- **Operations:** JSON logs with alert fields, exported to any OTLP backend

Next: login ([#20](https://github.com/heavymedl-labs/tetsu/issues/20)), then bookings and payments.

## Run it

With the test Postgres from [AGENTS.md](AGENTS.md#checks) running:

```bash
cp .env.example .env
cargo tetsu setup       # creates the database if missing, then migrates
cargo tetsu serve       # http://localhost:8080
```

`cargo tetsu` is an alias in `.cargo/config.toml` for `cargo run -q -p tetsu --`: it takes any `tetsu` command, such as
`cargo tetsu operator create "Lee Park" lee@example.com`, and rebuilds first if anything changed. Run it from the repository's root,
where `tetsu` reads `.env`.

One-time setup is in [crates/tetsu](crates/tetsu/README.md#run-locally). `scripts/check.sh` runs every check.
To test its security, and to report what you find privately, see [SECURITY.md](SECURITY.md).

## Runs on

Free tiers for now, each behind a setting or a port so it can be swapped
([runbook](docs/operations/demo-environment.md)).

| What | Provider | Status |
| --- | --- | --- |
| App | [Render](https://render.com) ([ADR-0012](docs/adr/0012-first-hosting-and-deploys.md)) | Live |
| Database | [Neon](https://neon.com), Postgres 18 ([ADR-0012](docs/adr/0012-first-hosting-and-deploys.md)) | Live |
| Code, CI, images | [GitHub](https://github.com/heavymedl-labs/tetsu) ([ADR-0023](docs/adr/0023-ci-on-demand.md), [ADR-0033](docs/adr/0033-deploys-on-demand.md)) | Live |
| Logs and alerts | [Grafana Cloud](https://grafana.com/products/cloud/) ([ADR-0049](docs/adr/0049-log-export.md)) | Live |
| Email | [Mailgun](https://www.mailgun.com)'s sandbox ([ADR-0074](docs/adr/0074-operator-email-in-development.md)) | Built; its settings come next |

## License

None yet. The source is public to read, but no license is granted to use, copy, modify, or deploy it.

[Architecture](ARCHITECTURE.md) · [Decisions](docs/adr/README.md) · [Quality bar](CONSTRAINTS.md) · [Glossary](docs/glossary.md) · [Contributing](CONTRIBUTING.md) · [Agents](AGENTS.md) · [Roadmap](https://github.com/heavymedl-labs/tetsu/milestones) · [Board](https://github.com/orgs/heavymedl-labs/projects/1)
