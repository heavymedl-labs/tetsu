# Architecture Decision Records

Each ADR records one decision with its context, alternatives, consequences,
and sources. To add one, copy [template.md](template.md), take the next
number, and add a row below. Never renumber.

| ADR | Title | Read when | Status |
| --- | --- | --- | --- |
| [0001](0001-how-we-work.md) | How we work | Changing how we record decisions or agent rules | Accepted |
| [0002](0002-platform-and-verticals.md) | Platform and verticals | Deciding what is platform and what is vertical | Accepted |
| [0003](0003-architecture.md) | Architecture | Structuring modules, ports, or deployment | Accepted |
| [0004](0004-security.md) | Security | Touching identity, tenant data, payments, or secrets | Accepted |
| [0005](0005-testing-and-performance.md) | Testing and performance | Setting tests, rigor tiers, or benchmarks | Accepted |
| [0006](0006-diagrams.md) | Diagrams | Drawing diagrams | Accepted |
| [0007](0007-server-stack.md) | Server stack | Choosing or using server libraries: HTTP, Postgres, templates, migrations | Accepted |
| [0008](0008-benchmark-harness.md) | Benchmark harness | Running benchmarks | Accepted |
| [0009](0009-defer-generated-queries.md) | Defer generated queries | Writing database queries | Accepted |
| [0010](0010-evaluating-technology.md) | Evaluating technology | Adopting a framework, library, service, or tool | Accepted |
| [0011](0011-server-stack-guardrails.md) | Server stack guardrails | Setting timeouts, statement caching, or migration locks | Accepted |
| [0012](0012-first-hosting-and-deploys.md) | First hosting and deploys | Deploying, or moving to a new host or paid plan | Accepted |
| [0013](0013-postgres-tls.md) | Postgres TLS | Connecting to Postgres | Accepted |
| [0014](0014-checks-and-duties.md) | Checks and duties | Changing a check, duty, or the quality bar | Accepted |
| [0015](0015-rust-checks.md) | Rust checks | Changing a Rust tool or limit | Accepted |
| [0016](0016-crate-layout-and-coverage.md) | Crate layout and coverage by tier | Adding a crate or module, or setting its tier | Accepted |
| [0017](0017-format-and-lint.md) | Format and lint | Adding a formatter, linter, or language, or changing a hook | Accepted |
| [0018](0018-demo-businesses-as-data.md) | Demo businesses as data | Adding a demo, seeding, or changing the seed file format | Accepted |
| [0019](0019-seed-format-versions.md) | Seed format versions | Changing the seed file format | Accepted |
| [0020](0020-branch-coverage.md) | Branch coverage | Setting coverage limits, or moving a crate to Tier 1 | Accepted |
| [0021](0021-module-public-surface.md) | Module public surface | Deciding what a module exports | Accepted |
| [0022](0022-benchmark-image-tag.md) | Benchmark image tag | Changing how the benchmark image is built or tagged | Proposed |
| [0023](0023-ci-on-demand.md) | CI on demand | Changing when CI runs, or what it runs | Accepted |
| [0024](0024-secrets.md) | Secrets | Adding, storing, or rotating a secret, or changing the secret scan | Superseded by 0027 |
| [0025](0025-dependency-checks.md) | Dependency checks | Adding a dependency, or handling an advisory or license | Superseded by 0028 |
| [0026](0026-security-target.md) | Security target | Building identity, tenant data, or payments, or planning a security review | Superseded by 0029 |
| [0027](0027-secrets-revised.md) | Secrets, revised | Adding, storing, or rotating a secret, or changing the secret scan | Accepted |
| [0028](0028-dependency-checks-revised.md) | Dependency checks, revised | Adding a dependency, or handling an advisory or license | Accepted |
| [0029](0029-security-target-revised.md) | Security target, revised | Building identity, tenant data, or payments, or planning a security review | Accepted |
| [0030](0030-front-end-model.md) | Front-end model | Building pages, components, themes, or anything a business embeds | Superseded by 0031 |
| [0031](0031-front-end-model-revised.md) | Front-end model, revised | Building pages, islands, themes, embeds, or the security headers | Accepted |
| [0032](0032-styling-engine.md) | Styling engine | Styling pages or templates, or using Tailwind Plus | Accepted |
| [0033](0033-deploys-on-demand.md) | Deploys on demand | Deploying, rolling back, or publishing the image | Accepted |
| [0034](0034-front-end-build.md) | Front-end build | Building or serving front-end assets, or adding a Node tool | Accepted |
| [0035](0035-component-sources-and-look.md) | Component sources and look | Building a page or component, or choosing colors or type | Accepted |
| [0036](0036-build-assets-where-used.md) | Build front-end assets where they are used | Building or deploying the site, or changing how assets are built | Accepted |
| [0037](0037-fictional-customers-on-tetsus-page.md) | Fictional customers on Tetsu's page | Writing content for Tetsu's own pages | Accepted |
| [0038](0038-color-scheme-control.md) | A color-scheme control without script | Changing colors, the color schemes, or the header's control | Accepted |
| [0039](0039-response-compression-and-page-budget.md) | Response compression and a page budget | Changing how responses are sent, adding page weight, or a page that holds a secret | Accepted |
| [0040](0040-identity-and-sign-in.md) | Identity and sign-in | Building sign-in, sign-up, accounts, sessions, or anything operators use | Accepted |
| [0041](0041-account-recovery.md) | Account recovery | Building recovery, recovery codes, anything that replaces a credential, or any link between an account and a client profile | Accepted |
| [0042](0042-tenant-isolation.md) | Tenant isolation | Storing tenant data, adding a table or module schema, writing a query, or adding a database role | Accepted |
| [0043](0043-domain-event-delivery.md) | Domain event delivery | Recording or consuming a domain event, or sending a webhook | Accepted |
| [0044](0044-email-delivery.md) | Email delivery | Sending email, or choosing an email provider | Accepted |
| [0045](0045-operations-baseline.md) | Operations baseline | Logging, auditing, backups, alerts, or recovery targets | Accepted |
| [0046](0046-property-testing.md) | Property testing | Writing property tests for Tier 1 code | Accepted |
| [0047](0047-sign-in-libraries.md) | Sign-in libraries | Adding or upgrading a sign-in crate, or hashing tokens and codes | Proposed |
| [0048](0048-sign-in-design.md) | Sign-in design | Building sign-in, sessions, staff roles, rate limits, or recovery | Accepted |
| [0049](0049-log-export.md) | Log export | Exporting logs, or changing what a log line carries off the host | Accepted |
| [0050](0050-local-settings-file.md) | Local settings file | Running `tetsu` locally, or adding a setting | Accepted |
| [0051](0051-coverage-reporting.md) | Coverage reporting | Reporting or showing test coverage | Proposed |
| [0052](0052-grafana-from-agent-sessions.md) | Grafana from agent sessions | Reading logs or alerts from a session, or changing Grafana | Accepted |
| [0053](0053-supported-browsers.md) | Supported browsers | Using newer CSS or HTML, or setting build targets | Proposed |
| [0054](0054-browser-tests.md) | Browser tests | Changing pages, styles, or interactions, or testing in a browser | Proposed |
| [0055](0055-fault-injection-tests.md) | Fault injection tests | Testing how Tetsu handles Postgres or network failures | Proposed |
| [0056](0056-typescript-tests.md) | TypeScript tests | Writing or testing TypeScript in `web/` | Accepted |

## Open decisions

Each is a **Decision needed** issue and becomes an ADR once decided. Work
that depends on one waits for it. The
[Market Scan](../research/market-scan/platform-implications.md) has a leading
option for most of them.

- [How verticals extend platform behavior](https://github.com/heavymedl-labs/tetsu/issues/7)
- [Payment processors and adapters](https://github.com/heavymedl-labs/tetsu/issues/10)
- [Public API design](https://github.com/heavymedl-labs/tetsu/issues/12)
- [Data import from competitors](https://github.com/heavymedl-labs/tetsu/issues/13)
- [Front ends: the staff app and native apps](https://github.com/heavymedl-labs/tetsu/issues/14); ADR-0031 decided customers on the web
- [Payroll: build or integrate](https://github.com/heavymedl-labs/tetsu/issues/15)
- [Time zones and recurring schedules](https://github.com/heavymedl-labs/tetsu/issues/21)
- [Data export, retention, and deletion](https://github.com/heavymedl-labs/tetsu/issues/22)
- [An optional personal profile across businesses](https://github.com/heavymedl-labs/tetsu/issues/117)
- [How businesses sign up and pay for Tetsu](https://github.com/heavymedl-labs/tetsu/issues/23)
- [Support access to tenant accounts](https://github.com/heavymedl-labs/tetsu/issues/24)
- [Money: currencies, rounding, and tax](https://github.com/heavymedl-labs/tetsu/issues/26)
- [Load test tools, latency budgets, and memory measurement](https://github.com/heavymedl-labs/tetsu/issues/37)
- [Fault injection tests, proposed as ADR-0055](https://github.com/heavymedl-labs/tetsu/issues/130)
- [The benchmark image tag rule, proposed as ADR-0022](https://github.com/heavymedl-labs/tetsu/issues/75)
- [Whether the benchmark harness keeps monoio](https://github.com/heavymedl-labs/tetsu/issues/94)
- [How long to override markdownlint-cli2's vulnerable dependencies](https://github.com/heavymedl-labs/tetsu/issues/95)
- [Object storage for files](https://github.com/heavymedl-labs/tetsu/issues/99)
- [Whether a CDN serves Tetsu's assets and pages](https://github.com/heavymedl-labs/tetsu/issues/100)

## Finding sources

Each ADR cites one of these catalogs or a primary source.

| Catalog | Use it for |
| --- | --- |
| [Microsoft Azure: Cloud design patterns](https://learn.microsoft.com/en-us/azure/architecture/patterns/) | Reliability and data patterns: retry, circuit breaker, CQRS, anti-corruption layer. Provider-neutral despite the branding |
| [AWS Prescriptive Guidance: Cloud design patterns](https://docs.aws.eu/prescriptive-guidance/latest/cloud-design-patterns/introduction.html) | Hexagonal architecture, transactional outbox, saga, strangler fig for migrations |
| [AWS SaaS Lens](https://docs.aws.amazon.com/wellarchitected/latest/saas-lens/saas-lens.html) | Multi-tenancy: isolation models, tenant onboarding, noisy neighbors |
| [Fowler: Patterns of Enterprise Application Architecture](https://martinfowler.com/eaaCatalog/) | Application internals: Money, Repository, Unit of Work, optimistic locking |
| [Richardson: microservices.io](https://microservices.io/patterns/) | Data consistency and messaging: transactional outbox, saga, idempotent consumer |
| [Hohpe and Woolf: Enterprise Integration Patterns](https://www.enterpriseintegrationpatterns.com/patterns/messaging/) | Messaging and integrations: process manager, channels, routing |
| [Evans: Domain-Driven Design Reference](https://www.domainlanguage.com/ddd/reference/) | Aggregates, bounded contexts, domain events, anti-corruption layer |
| [Rust Design Patterns](https://rust-unofficial.github.io/patterns/) | Rust idioms: newtype, builder, RAII guards |
| [OWASP Cheat Sheet Series](https://cheatsheetseries.owasp.org/) | Security: authentication, sessions, password storage, access control |
