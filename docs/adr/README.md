# Architecture Decision Records

Each ADR records one decision with its context, alternatives, consequences,
and sources. To add one, copy [template.md](template.md), take the next
number, and add a row below. Never renumber.

| ADR | Title | Status |
| --- | --- | --- |
| [0001](0001-how-we-work.md) | How we work | Proposed |
| [0002](0002-platform-and-verticals.md) | Platform and verticals | Proposed |
| [0003](0003-architecture.md) | Architecture | Proposed |
| [0004](0004-security.md) | Security | Proposed |
| [0005](0005-testing-and-performance.md) | Testing and performance | Proposed |

## Open decisions

Each is a **Decision needed** issue and becomes an ADR once decided. Work
that depends on one waits for it. The
[Market Scan](../research/market-scan/platform-implications.md) has a leading
option for most of them.

- [How verticals extend platform behavior](https://github.com/HeavyMedl/tetsu/issues/7)
- [Tenant isolation in Postgres](https://github.com/HeavyMedl/tetsu/issues/8)
- [Identity and customer accounts](https://github.com/HeavyMedl/tetsu/issues/9)
- [Payment processors and adapters](https://github.com/HeavyMedl/tetsu/issues/10)
- [Domain event delivery](https://github.com/HeavyMedl/tetsu/issues/11)
- [Public API design](https://github.com/HeavyMedl/tetsu/issues/12)
- [Data import from competitors](https://github.com/HeavyMedl/tetsu/issues/13)
- [Front ends: staff app framework, customer web or native](https://github.com/HeavyMedl/tetsu/issues/14)
- [Payroll: build or integrate](https://github.com/HeavyMedl/tetsu/issues/15)
- [Server stack: HTTP, async runtime, database, HTML templating](https://github.com/HeavyMedl/tetsu/issues/16)
- [First hosting environment and deploy pipeline](https://github.com/HeavyMedl/tetsu/issues/18)

## Finding sources

Each ADR cites one of these catalogs or a primary source.

| Catalog | Use it for |
| --- | --- |
| [Microsoft Azure: Cloud design patterns](https://learn.microsoft.com/en-us/azure/architecture/patterns/) | Reliability and data patterns: retry, circuit breaker, CQRS, anti-corruption layer. Provider-neutral despite the branding |
| [AWS Prescriptive Guidance: Cloud design patterns](https://docs.aws.amazon.com/prescriptive-guidance/latest/cloud-design-patterns/introduction.html) | Hexagonal architecture, transactional outbox, saga, strangler fig for migrations |
| [AWS SaaS Lens](https://docs.aws.amazon.com/wellarchitected/latest/saas-lens/saas-lens.html) | Multi-tenancy: isolation models, tenant onboarding, noisy neighbors |
| [Fowler: Patterns of Enterprise Application Architecture](https://martinfowler.com/eaaCatalog/) | Application internals: Money, Repository, Unit of Work, optimistic locking |
| [Richardson: microservices.io](https://microservices.io/patterns/) | Data consistency and messaging: transactional outbox, saga, idempotent consumer |
| [Hohpe and Woolf: Enterprise Integration Patterns](https://www.enterpriseintegrationpatterns.com/patterns/messaging/) | Messaging and integrations: process manager, channels, routing |
| [Evans: Domain-Driven Design Reference](https://www.domainlanguage.com/ddd/reference/) | Aggregates, bounded contexts, domain events, anti-corruption layer |
| [Rust Design Patterns](https://rust-unofficial.github.io/patterns/) | Rust idioms: newtype, builder, RAII guards |
| [OWASP Cheat Sheet Series](https://cheatsheetseries.owasp.org/) | Security: authentication, sessions, password storage, access control |
