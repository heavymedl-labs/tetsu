# 0002. Platform and verticals

- Status: Proposed
- Date: 2026-11-02

## Context

Tetsu should serve many verticals, starting with boutique fitness. In the
[Market Scan](../research/market-scan/README.md), most features of the seven
fitness products map to industry-neutral capabilities rather than fitness-only
ones, and two incumbents (Mindbody, WellnessLiving) already serve fitness,
wellness, salon and spa from one product.

## Decision

- The platform builds industry-neutral capabilities up front, designed to serve
  more than one vertical. The capability map in
  [Platform implications](../research/market-scan/platform-implications.md) is
  the starting list.
- Each platform capability is designed against two industries: boutique
  fitness and salons and spas. Salons and spas test the platform where
  fitness is light: one-to-one appointments, staff and room resources, retail,
  and tips.
- A vertical module holds only what is specific to its industry, and builds on
  platform use cases and events.
- The platform never depends on, imports, or names a vertical. Platform code
  uses industry-neutral names, and new terms go in the
  [glossary](../GLOSSARY.md).

## Alternatives considered

- **Extract from a proven vertical.** Avoids guessing, but each module owns a
  Postgres schema, so moving code out of a vertical later means migrating data
  and renaming concepts. The platform would also stay thin while the vertical
  became the real product.
- **A fork per vertical.** Duplicates every fix and shared capability.

## Consequences

Platform capabilities take longer to design, and a wrong abstraction would
affect every vertical; designing against two industries limits that risk. How
verticals extend platform behavior is an open decision.

## Sources

- Eric Evans, *Domain-Driven Design*, Addison-Wesley, 2003, ch. 15: core
  domain and generic subdomains.
- Martin Fowler, *Analysis Patterns*, Addison-Wesley, 1996: Party and Account.
- Sandi Metz, "The Wrong Abstraction," 2016: the risk this ADR accepts.
