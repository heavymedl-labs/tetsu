# Platform implications

What the research means for Tetsu, our platform, across every vertical scanned: proposed changes to the plan, then the platform capabilities the feature matrix points to, with each one's role and pattern. Pattern catalogs for ADR sources are in the [ADR index](../../adr/README.md#finding-sources).

## Proposed changes from boutique fitness

Proposals, not decisions. The open ones are listed in the [ADR index](../../adr/README.md#open-decisions).

- Put events, programs and trainings in the platform's Scheduling and Catalog modules.
- Build the Payments port with two adapters from the start, so processor choice is a feature.
- Make the public API spec-first: OpenAPI generated from Rust types, self-serve keys, a sandbox tenant, machine-to-machine credentials, signed at-least-once webhooks.
- Make customer identity tenant-owned, in response to Mindbody's shared-login backlash.
- Add a journal export from the Ledger now; treat full bookkeeping as a later module.
- Add an Import home: one adapter per source system that moves clients, memberships, credits and stored cards, idempotent and resumable. Migration is the main switching cost.
- Decide whether to build payroll tax filing (as Arketa does) or integrate a provider.
- Decide on client apps: one responsive web app (Walla) or native branded apps (Mariana Tek).

## Platform capabilities

Three groups, named as in the [glossary](../../GLOSSARY.md): the **control plane** (tenants and access), **shared capabilities** that several verticals reuse, and **connectors**. Each vertical's industry-specific code sits on top.

| Group | Capability | Role | Pattern |
| --- | --- | --- | --- |
| Control plane | Tenancy | Domain module | Pool model: shared schema, tenant ID on every row, Postgres row-level security, tenant context carried in Rust types |
| Control plane | Identity | Domain module + driven port | OAuth 2.0 and OpenID Connect; Argon2id password hashing from vetted crates; customer accounts owned by the tenant |
| Control plane | Authorization | Domain module | Role-based access control, checked at the use-case boundary; staff and customer scopes |
| Control plane | Theming | Domain module | Per-tenant configuration read by the presentation adapters |
| Control plane | Audit | Domain module | Append-only log of actor, tenant and action; not in the competitor maps, but required for us |
| Control plane | Import (new) | Domain module + driven port | One anti-corruption adapter per source system; idempotent, resumable batch import |
| Shared capability | Party | Domain module | Party pattern: one record for people and organizations, with roles |
| Shared capability | Documents | Domain module + driven port (storage) | Immutable versions; each signature records the exact version signed |
| Shared capability | Scheduling | Domain module | Temporal expressions for recurrence and exceptions |
| Shared capability | Booking | Domain module | Aggregate per session enforcing capacity, waitlist and payment invariants; optimistic concurrency |
| Shared capability | Attendance | Domain module | Check-ins, late cancels and no-shows recorded as domain events |
| Shared capability | Catalog | Domain module | Service types with explicitly linked offerings |
| Shared capability | Entitlements | Domain module | Specification pattern for eligibility; credits kept as an account of movements |
| Shared capability | Pricing | Domain module | Pure price-breakdown function; Strategy for discount and tax rules |
| Shared capability | Orders | Domain module | Aggregate; idempotency key on every write |
| Shared capability | Inventory | Domain module | Account pattern: stock as a ledger of movements |
| Shared capability | Subscriptions | Domain module | Explicit state machine (active, frozen, cancelled); scheduled renewals |
| Shared capability | Ledger | Domain module | Double-entry, append-only accounting; corrections by reversal; Money as integer minor units with currency |
| Shared capability | Staff | Domain module | Pay rules as Strategy (per class, per head, revenue share, hourly) |
| Shared capability | Domain events | Domain module | Events stored in the same transaction as the state change (transactional outbox), then consumed by Analytics, Workflows and webhooks |
| Shared capability | Segments | Domain module | Saved queries over Party and Analytics read models |
| Shared capability | Workflows | Domain module | Process managers triggered by domain events |
| Shared capability | Analytics | Domain module | CQRS read models projected from domain events |
| Connector | Payments | Driven port | One adapter per processor behind an anti-corruption layer; idempotent handling of processor webhooks; card data stays with the processor |
| Connector | Notifications | Driven port | One adapter per channel; sends dispatched from the transactional outbox |
| Connector | Media (new) | Driven port | Adapter for object storage or a video provider |
| Connector | Integrations | Driven port | Anti-corruption layer per partner (ClassPass, Zoom, accounting tools) |
| Connector | Public API | Inbound adapter | REST over the same use cases the staff app uses; OpenAPI generated from Rust types; keys, rate limits, versioning |
| Connector | Webhooks | Outbound adapter | Signed, at-least-once delivery from the outbox, with retries; mapped as "Public API, Domain events" |

## Sources

- [Cockburn: Hexagonal architecture](https://alistair.cockburn.us/hexagonal-architecture/)
- [AWS SaaS Lens: silo, pool and bridge models](https://docs.aws.amazon.com/wellarchitected/latest/saas-lens/silo-pool-and-bridge-models.html); [PostgreSQL: row security policies](https://www.postgresql.org/docs/current/ddl-rowsecurity.html)
- [OWASP: password storage](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html); [NIST: role-based access control](https://csrc.nist.gov/projects/role-based-access-control)
- Fowler, *Analysis Patterns* (Party, Account); [Fowler: Recurring events for calendars](https://martinfowler.com/apsupp/recurring.pdf); [Evans and Fowler: Specifications](https://martinfowler.com/apsupp/spec.pdf)
- [Vernon: Effective aggregate design](https://www.dddcommunity.org/library/vernon_2011/); [Microsoft: anti-corruption layer](https://learn.microsoft.com/en-us/azure/architecture/patterns/anti-corruption-layer)
- [Fowler: Accounting patterns](https://martinfowler.com/eaaDev/AccountingNarrative.html); [Fowler: Money](https://martinfowler.com/eaaCatalog/money.html)
- [Richardson: Transactional outbox](https://microservices.io/patterns/data/transactional-outbox.html); [Hohpe and Woolf: Process manager](https://www.enterpriseintegrationpatterns.com/patterns/messaging/ProcessManager.html); [Fowler: CQRS](https://martinfowler.com/bliki/CQRS.html)
- [Stripe: idempotent requests](https://docs.stripe.com/api/idempotent_requests)
