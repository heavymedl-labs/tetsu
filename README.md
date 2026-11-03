# Tetsu

Tetsu (working name) is a planned multi-tenant SaaS platform in Rust for
businesses that sell scheduled time and memberships. Its features are
industry-neutral; each industry, or vertical, adds only what is specific to it.

The project is in its documentation phase: no application code exists yet.

## What Tetsu offers

- **Run the day:** recurring sessions, appointments, and events; bookings
  with capacity limits, waitlists, check-in, and no-show tracking; locations,
  staff roles, pay rules, custom branding, and an audit log.
- **Sell and get paid:** memberships, credits, retail, discounts, and taxes.
  Every movement lands in a double-entry ledger that exports to accounting
  tools, and card data never reaches Tetsu.
- **Know your clients:** profiles with history, signed documents, segments,
  automated messages, and reports.
- **Connect:** customer sign-in to accounts the business owns, a public API
  with a sandbox and signed webhooks, imports from the previous system, and
  partner integrations.

Tetsu competes on trust and experience. Pricing is flat and published,
businesses choose their processor, and customer data stays with them. Every
surface, web or native, feels fast and intuitive.

The [capability map](docs/research/market-scan/platform-implications.md) shows
how each feature is built, and several wait on
[open decisions](docs/adr/README.md#open-decisions). The marketing site ships
first, then sign-in.

## Planned stack

Rust and PostgreSQL. The staff and admin app is WebAssembly; public pages are
server-rendered HTML.

## Where things live

| Document | Holds |
| --- | --- |
| [ARCHITECTURE.md](ARCHITECTURE.md) | The system's shape and invariants |
| [docs/adr](docs/adr/README.md) | Decisions and the reasoning behind them |
| [docs/GLOSSARY.md](docs/GLOSSARY.md) | Shared vocabulary |
| [docs/research](docs/research/market-scan/README.md) | Market research that informs decisions |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How work moves from issue to merge |
| [AGENTS.md](AGENTS.md) | Rules for coding agents, and the checks to run |

Work is tracked in the [Tetsu project](https://github.com/users/HeavyMedl/projects/1).
