# Invariants

Hard rules for every change. A change that breaks one needs a superseding ADR.

- The platform never depends on, imports, or names a vertical.
- A module's public surface is its use cases, its driven ports, its domain
  events, and the `Router` the composition root mounts
  ([ADR-0007](adr/0007-server-stack.md)).
- Modules talk only through each other's use cases and domain events.
- Domain models, use cases, and driven ports name no HTTP framework, database
  driver, or template engine types
  ([ADR-0007](adr/0007-server-stack.md)).
- A module owns at most one Postgres schema and touches no other.
- A tenant sees only its own data.
- Card data never reaches Tetsu ([ADR-0004](adr/0004-security.md)).
- Cryptography comes from vetted crates; we never write our own primitives or
  protocols.
- Nothing in the binary depends on a single cloud provider.
