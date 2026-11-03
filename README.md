# Tetsu

Tetsu (working name) is a planned multi-tenant vertical SaaS platform in Rust.
The platform builds industry-neutral capabilities up front, and each vertical
plugs in as a module holding only what is specific to its industry. Boutique
fitness, benchmarked against Mariana Tek, is the first vertical.

The project is in its documentation phase: no application code exists yet.

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
