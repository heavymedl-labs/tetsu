# 0003. Architecture

- Status: Accepted
- Date: 2026-11-03
- Schema boundary enforcement, left open below, decided by [ADR-0042](0042-tenant-isolation.md)

## Context

Tenancy, money, and each vertical need hard boundaries. The team is small, and
Tetsu must run on any cloud or our own hardware.

## Decision

- **Modular monolith, microkernel style.** A small kernel defines the module
  contract: the traits and types every module implements. Platform and
  vertical modules plug in at compile time, and the `tetsu` binary, the
  composition root, registers them. The kernel is a contract, not a framework:
  no runtime plug-in loading and no module registry beyond the composition
  root.
- **Hexagonal modules.** Each module's domain sits at the center. Other
  modules and inbound adapters (HTTP API, server-rendered pages) call its use
  cases. It calls out through driven ports, Rust traits that
  outbound adapters implement (Postgres, payment processor, SMS).
- **Isolation.** Each module is its own crate and owns at most one Postgres
  schema. Modules talk only through each other's use cases and domain
  events, never through another module's internals or tables.
- **Deployment.** One Rust binary in an OCI image. PostgreSQL holds records,
  and S3-compatible object storage holds files such as signed documents and
  media. Configuration comes from the environment, and nothing depends on
  provider-specific services.

## Alternatives considered

- **Microservices.** Strong boundaries, but network calls and operating cost
  arrive before independent scaling is needed.
- **Layered monolith.** Cheap to start; boundaries erode.
- **Shared schema across modules.** Easy joins, no enforcement.

## Consequences

The crate graph enforces code boundaries; schema boundaries need their own
enforcement, decided with tenant isolation. Cross-module reporting uses read
models built from events. New infrastructure needs are met with Postgres or
added behind a port.

## Sources

- Alistair Cockburn, "Hexagonal Architecture," 2005.
- Mark Richards and Neal Ford, *Fundamentals of Software Architecture*,
  O'Reilly, 2020, ch. 12: microkernel.
- Chris Richardson, "Pattern: Database per service," microservices.io.
- Adam Wiggins, "The Twelve-Factor App," 12factor.net.
