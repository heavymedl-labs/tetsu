# Architecture

A bird's-eye view of Tetsu's design; nothing is built yet. The
[ADRs](docs/adr/README.md) hold the reasoning; this page holds the shape and
the rules that must stay true.

## Shape

Tetsu is a modular monolith built as a microkernel with hexagonal modules,
shipped as one binary, `tetsu`, backed by PostgreSQL
([ADR-0003](docs/adr/0003-architecture.md)). A small kernel defines the module
contract, and the binary wires the modules together.

```text
                 tetsu (composition root)
                           |
     +---------------------+---------------------+
     |                     |                     |
control plane      shared capabilities      edge adapters
tenancy            scheduling, booking      payments
identity, access   catalog, pricing         notifications
audit              orders, subscriptions    public API
                   ledger
     \                     |                     /
      +-------- use cases, domain events --------+
                           |
                       verticals
                   fitness (first)
```

[ADR-0002](docs/adr/0002-platform-and-verticals.md) decides what belongs in
the platform; the
[Market Scan](docs/research/market-scan/platform-implications.md) has the full
capability map.

## Invariants

A change that breaks one needs a superseding ADR.

- The platform never depends on, imports, or names a vertical.
- A module's public surface is its use cases, its driven ports, and its domain
  events.
- Modules talk only through each other's use cases and domain events.
- A module owns at most one Postgres schema and touches no other.
- Card data never reaches Tetsu; cryptography comes from vetted crates
  ([ADR-0004](docs/adr/0004-security.md)).
- Nothing in the binary depends on a single cloud provider.
