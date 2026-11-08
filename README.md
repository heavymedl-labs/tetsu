<h1 align="center">
  <br>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/brand/tetsu-wordmark-dark.svg">
    <img alt="Tetsu" src="docs/brand/tetsu-wordmark-light.svg" width="360">
  </picture>
  <br>
  <br>
</h1>

Software for businesses whose customers book time and resources.

Tetsu (working name) is a multi-tenant platform in Rust that includes:

- **scheduling and booking**, with options such as waitlists and check-in
- **sales and payments**, including memberships, on a double-entry ledger
- **customer profiles**, messaging, and reports
- a **public API** with a sandbox and webhooks

Each industry adds only what is specific to it. Industries such as fitness, salons, and
campgrounds are examples that test the platform, not its definition.

It is a hobby project that aims to become a real business, so designs keep real-business
needs such as isolation, compliance, and sign-up in view. The project is early: only the
kernel and the `tetsu` binary exist so far.

## Where things live

- [ARCHITECTURE.md](ARCHITECTURE.md): the system's shape and invariants
- [docs/adr](docs/adr/README.md): decisions and the reasoning behind them
- [docs/GLOSSARY.md](docs/GLOSSARY.md): shared vocabulary
- [CONSTRAINTS.md](CONSTRAINTS.md): the quality bar
- [docs/reference-businesses](docs/reference-businesses/README.md): fictional
  businesses we check the product against
- [docs/research](docs/research/market-scan/README.md): market research that
  informs decisions
- [CONTRIBUTING.md](CONTRIBUTING.md): how work moves from issue to merge
- [AGENTS.md](AGENTS.md): rules for coding agents, and the checks to run
- [Milestones](https://github.com/heavymedl-labs/tetsu/milestones): the roadmap,
  from walking skeleton to each reference business going live
- [Tetsu project](https://github.com/orgs/heavymedl-labs/projects/1): tracked work
