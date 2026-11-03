# Tetsu

Tetsu (working name) is a planned multi-tenant SaaS platform in Rust for
businesses that sell scheduled time and memberships. Its features are
industry-neutral; each industry, or vertical, adds only what is specific to it.

The project is in its documentation phase: no application code exists yet.

## What Tetsu offers

| Area | What a business gets |
| --- | --- |
| Business setup | Locations, staff roles, custom branding, and an audit log |
| Schedule | Recurring sessions, appointments, and events, with exceptions |
| Booking | Capacity limits, waitlists, check-in, and no-show tracking |
| Selling | Services, memberships, credits, discounts, taxes, and retail stock |
| Payments | Any supported processor; card data never reaches Tetsu |
| Books | A double-entry ledger, exported to accounting tools |
| Staff | Pay rules per session, per head, revenue share, or hourly |
| Clients | Profiles with history, signed documents, and segments |
| Automation | Notifications and workflows triggered by events |
| Reporting | Reports built from recorded events |
| Customer accounts | Customers sign in to book, pay, and manage memberships; the business owns the accounts |
| Developers | A REST API, self-serve keys, a sandbox, and signed webhooks |
| Switching | Imports of clients, memberships, and credits from the previous system |
| Integrations | Connectors for booking marketplaces, accounting tools, and other partners |

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
