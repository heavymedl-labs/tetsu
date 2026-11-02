# 0001: Modular monolith with hexagonal ports

- Status: accepted
- Date: 2026-11-02
- Risk tier: 1 (shapes tenant isolation and the ledger)

## Context

Tetsu is a multi-tenant vertical SaaS platform: shared platform capabilities with
verticals plugged in, starting with boutique fitness. The team is small. The
riskiest flows (book a spot, spend a credit, record the ledger entry) must succeed
or fail together. The boundaries between capabilities are not proven yet; we
extract shared abstractions only after a vertical needs them. Deployment must be a
single binary plus Postgres that runs on any cloud or our own hardware.

## Decision

Build Tetsu as a **microkernel-style modular monolith** using **hexagonal
architecture** (ports and adapters):

- **One crate per platform capability or vertical**, in one Cargo workspace,
  compiled into one binary (`crates/server`, the composition root).
- **Capabilities expose ports as Rust traits.** Adapters (Postgres, payment
  processors, email, HTTP) implement or call them; domain code never imports an
  adapter.
- **Each module owns its own Postgres schema.** No module reads another module's
  tables; modules talk only through ports and domain events.
- **Three platform groups:** control plane (tenancy, identity, authorization,
  audit), shared domain capabilities (scheduling, booking, entitlements, ledger and
  others), and adapters at the edge. A vertical holds its own core domain on top.
- **Cross-module work that must be atomic** runs in one Postgres transaction;
  events leave through a transactional outbox.

## Options considered

1. **Modular monolith (chosen).** One deployable, real transactions, compiler-checked
   boundaries. Coarse scaling and a shared release, mitigated by running the binary
   in separate web and worker roles.
2. **Microservices.** Independent deploys and scaling, but distributed transactions
   (sagas) on money and bookings, network failure modes, and operational cost a
   small team can't carry. Boundaries would be frozen before they are understood.
3. **Unstructured monolith.** Fastest at first, but nothing stops modules reaching
   into each other, which makes later extraction or verticals expensive.

## Consequences

- Boundaries only hold if enforced: crate visibility, no shared tables, one schema
  per module, and review of any new cross-crate dependency.
- Extracting a module later (likely candidates: notifications, analytics, media)
  means swapping its adapter for a network client; callers keep the same port.
- Every module is testable alone (unit and module tests) and together (end-to-end),
  per the testing pyramid.
- Follow-up ADRs: tenant isolation (0002), ledger design, customer identity.

## Sources

- Alistair Cockburn, [Hexagonal architecture](https://alistair.cockburn.us/hexagonal-architecture/)
- Martin Fowler, [MonolithFirst](https://martinfowler.com/bliki/MonolithFirst.html)
- Mark Richards, *Software Architecture Patterns* (O'Reilly), microkernel architecture
- Eric Evans, *Domain-Driven Design*: core, supporting and generic subdomains
- Chris Richardson, [Transactional outbox](https://microservices.io/patterns/data/transactional-outbox.html)
- Microsoft, [Control planes in multitenant solutions](https://learn.microsoft.com/en-us/azure/architecture/guide/multitenant/considerations/control-planes)
