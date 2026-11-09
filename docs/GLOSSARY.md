# Glossary

The project's shared vocabulary. Use these terms in code, docs, issues, and
conversation. Add a term when it first matters; change one only with care,
because code will follow it.

**Accepted risk.** A security requirement Tetsu should meet but does not yet, recorded with the
maintainer's approval, an issue, and a fix-by date ([ADR-0029](adr/0029-security-target-revised.md)).

**Adapter.** Code at a module's edge. An inbound adapter (HTTP API,
server-rendered pages) calls the module's use cases. An outbound adapter
implements a driven port with a specific technology, such as Postgres or a
payment processor.

**ADR.** Architecture Decision Record: a short document in `docs/adr` that
records one significant decision, its alternatives, and its sources.

**Cell.** One complete, independent deployment of Tetsu serving some of its
tenants, so a failure in one cell does not reach the others. Whether Tetsu uses
cells is [undecided](https://github.com/heavymedl-labs/tetsu/issues/8).

**Check.** An automated control that gives the same answer every time, such as a
test or a lint. Contrast with a duty. See
[ADR-0014](adr/0014-checks-and-duties.md).

**Composition root.** The `tetsu` binary, which wires modules together. The
only place that knows every module.

**Connectors.** The group of platform modules that connect Tetsu to outside
systems, such as payments, notifications, and the public API. Not to be
confused with adapters, which sit at the edge of every module.

**Control plane.** The platform modules that run Tetsu itself, such as
tenancy, identity and access, and audit.

**Demo business.** A fictional tenant, kept as a data file in the `tetsu-demos`
repository and shown only in the demo environment, which never holds a real
business. See [ADR-0018](adr/0018-demo-businesses-as-data.md).

**Design token.** A named design value, such as a color or a spacing step, kept in one file and generated
into the `--tetsu-*` CSS properties every page uses. A tenant's theme is its token values
([ADR-0031](adr/0031-front-end-model-revised.md)).

**Domain event.** A record that something happened in a module, published for
other modules to react to. Past tense, such as `BookingCancelled`.

**Duty.** Judgment an agent or person must carry out because no tool can, such as
simplifying a change. Listed in [AGENTS.md](../AGENTS.md#duties).

**Embed.** A frame of a Tetsu page placed in a business's own website by the `<tetsu-embed>` element, for
browsing only: nothing signed in is ever framed ([ADR-0031](adr/0031-front-end-model-revised.md)).

**Fresh review.** A review by an agent that did not write the change, seeing only
the issue and the diff. See [docs/review.md](review.md).

**Island.** A small Web Component that adds behavior to server-rendered HTML on a Tetsu page, without
rendering markup of its own ([ADR-0031](adr/0031-front-end-model-revised.md)).

**Kernel.** The small core that defines the module contract: traits and types,
with no domain knowledge and no runtime plug-in loading.

**Minimum sellable product.** The least a reference business needs before it
switches to Tetsu. Each reference business's page defines its own, and going
live with it is a milestone.

**Module.** A Cargo crate that plugs into the kernel and owns at most one
Postgres schema. Platform capabilities and verticals are both modules.

**Platform.** Everything that is not a vertical: the kernel, the control plane,
shared capabilities, and connectors.

**Port.** A Rust trait a module calls out through, also called a driven port.
Outbound adapters implement it.

**Reference business.** A fictional business that adopts Tetsu, used to check
decisions, milestones, and tests from a customer's side. There is one for each
industry in ADR-0002, described in
[reference-businesses](reference-businesses/README.md).

**Rigor tier.** The testing and review bar for an area, from 1 (highest) to 3,
set by [ADR-0005](adr/0005-testing-and-performance.md).

**Shared capability.** Industry-neutral behavior the platform builds up front
for more than one vertical, such as scheduling, booking, or the ledger.

**Surface.** A way people reach Tetsu: booking pages in a browser, hosted or embedded,
the staff app, native apps, or the public API. Every surface uses the same use cases.

**Tenant.** A business using Tetsu, such as a boutique fitness brand,
together with all of its locations. Tenant data is isolated from every
other tenant's.

**Use case.** An operation a module offers. Other modules and inbound adapters
call it; together with domain events, use cases are how modules talk.

**Vertical.** A module for one industry. It holds only what is specific to
that industry and builds on the platform. Boutique fitness is the first;
salons and spas are the second industry every capability is designed against.

**Walking skeleton.** The first thin slice through every layer, from sign-in
to production, built to prove the architecture before feature work. It is the
first milestone.
