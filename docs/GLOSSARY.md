# Glossary

The project's shared vocabulary. Use these terms in code, docs, issues, and
conversation. Add a term when it first matters; change one only with care,
because code will follow it.

**Adapter.** Code at a module's edge. An inbound adapter (HTTP API, staff app,
server-rendered pages) calls the module's use cases. An outbound adapter
implements a driven port with a specific technology, such as Postgres or a
payment processor.

**ADR.** Architecture Decision Record: a short document in `docs/adr` that
records one significant decision, its alternatives, and its sources.

**Composition root.** The `tetsu` binary, which wires modules together. The
only place that knows every module.

**Connectors.** The group of platform modules that connect Tetsu to outside
systems, such as payments, notifications, and the public API. Not to be
confused with adapters, which sit at the edge of every module.

**Control plane.** The platform modules that run Tetsu itself, such as
tenancy, identity and access, and audit.

**Domain event.** A record that something happened in a module, published for
other modules to react to. Past tense, such as `BookingCancelled`.

**Kernel.** The small core that defines the module contract: traits and types,
with no domain knowledge and no runtime plug-in loading.

**Module.** A Cargo crate that plugs into the kernel and owns at most one
Postgres schema. Platform capabilities and verticals are both modules.

**Platform.** Everything that is not a vertical: the kernel, the control plane,
shared capabilities, and connectors.

**Port.** A Rust trait a module calls out through, also called a driven port.
Outbound adapters implement it.

**Rigor tier.** The testing and review bar for an area, from 1 (highest) to 3,
set by [ADR-0005](adr/0005-testing-and-performance.md).

**Shared capability.** Industry-neutral behavior the platform builds up front
for more than one vertical, such as scheduling, booking, or the ledger.

**Tenant.** A customer business using Tetsu, such as a boutique fitness
brand, together with all of its locations. Tenant data is isolated from every
other tenant's.

**Use case.** An operation a module offers. Other modules and inbound adapters
call it; together with domain events, use cases are how modules talk.

**Vertical.** A module for one industry. It holds only what is specific to
that industry and builds on the platform. Boutique fitness is the first;
salons and spas are the second industry every capability is designed against.
