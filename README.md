# Tetsu

Software for businesses that sell scheduled time and memberships.

Tetsu (working name) is a multi-tenant platform in Rust that includes:

- **scheduling and booking** with waitlists and check-in
- **memberships, sales, and payments** on a double-entry ledger
- **client profiles**, messaging, and reports
- a **public API** with a sandbox and webhooks

Each industry adds only what is specific to it. The project is in its design
phase, and nothing is built yet.

## Where things live

- [ARCHITECTURE.md](ARCHITECTURE.md): the system's shape and invariants
- [docs/adr](docs/adr/README.md): decisions and the reasoning behind them
- [docs/GLOSSARY.md](docs/GLOSSARY.md): shared vocabulary
- [docs/reference-businesses](docs/reference-businesses/README.md): fictional
  businesses we check the product against
- [docs/research](docs/research/market-scan/README.md): market research that
  informs decisions
- [CONTRIBUTING.md](CONTRIBUTING.md): how work moves from issue to merge
- [AGENTS.md](AGENTS.md): rules for coding agents, and the checks to run
- [Milestones](https://github.com/HeavyMedl/tetsu/milestones): the roadmap,
  from walking skeleton to first customer
- [Tetsu project](https://github.com/users/HeavyMedl/projects/1): tracked work
