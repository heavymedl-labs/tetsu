# Architecture

A bird's-eye view of Tetsu's design. The [README](README.md) describes what is implemented.
The [ADRs](docs/adr/README.md) hold the reasoning; this page holds the shape and
the rules that must stay true.

## Shape

Tetsu is a modular monolith built as a microkernel with hexagonal modules
([ADR-0003](docs/adr/0003-architecture.md)). It ships as one binary, `tetsu`,
which runs as the server, keeps records in PostgreSQL, and keeps files in
object storage. A small kernel defines the module contract, and the binary
wires the modules together.

[ADR-0002](docs/adr/0002-platform-and-verticals.md) decides what belongs in
the platform. The full capability map is in the
[Market Scan](docs/research/market-scan/platform-implications.md).

The diagrams follow the [C4 model](https://c4model.com/) and zoom in one level
at a time: system context, containers, components, and a single module, which
stands in for C4's code level. Views of tenancy, a request flow, and
deployment come last.

**Key.** Rounded gray boxes are people and their devices. Blue boxes are parts
of Tetsu, green cylinders are data stores, and purple marks the kernel,
composition root, and domain events. Rounded warm-gray boxes are outside
systems. Where it helps, a box adds its role or technology under its name.

### System context

Tetsu as one box: who uses it, and which outside systems it depends on. Many
businesses share one Tetsu, and each sees only its own data.

```mermaid
flowchart TB
    accTitle: System context
    accDescr: Many businesses share Tetsu. Each business's staff run it with Tetsu and its customers book and pay with it. Prospective businesses read about Tetsu. Partner software calls Tetsu's API and receives its webhooks. Tetsu charges, messages, syncs, streams, and imports through outside systems: payment processors, email and SMS providers, marketplaces, accounting tools, video and streaming, and previous systems.

    prospects([Prospective businesses])
    staff([Staff of each business])
    customers([Customers of each business])

    tetsu["Tetsu<br/>scheduling, booking, memberships, and payments<br/>for many businesses"]

    partners([Partner software])
    subgraph outside[Outside systems]
        processors([Payment processors]) ~~~ messaging([Email and SMS providers]) ~~~ marketplaces([Marketplaces])
        accounting([Accounting tools]) ~~~ video([Video and streaming]) ~~~ previous([Previous systems])
    end

    prospects -- reads about --> tetsu
    staff -- runs the business with --> tetsu
    customers -- books and pays with --> tetsu
    partners -- calls the API of --> tetsu
    tetsu -- sends webhooks to --> partners
    tetsu -- charges, messages, syncs, streams, imports through --> outside

    classDef actor fill:#f1f5f9,stroke:#64748b,color:#0f172a
    classDef ours fill:#dbeafe,stroke:#2563eb,color:#1e3a8a
    classDef external fill:#f5f5f4,stroke:#78716c,color:#292524
    class prospects,staff,customers actor
    class tetsu ours
    class partners,processors,messaging,marketplaces,accounting,video,previous external
```

### Containers

Tetsu's separately running parts are blue and green. The server renders each
business's booking pages and a simple page about Tetsu, and serves the API.
PostgreSQL holds records, and object storage holds files such as signed
documents and media. Card details go from the browser or app
straight to the processor's hosted fields and never reach Tetsu
([ADR-0004](docs/adr/0004-security.md)).

Customers use the booking pages in a browser on Tetsu's address; a business's own domain
waits on [#8](https://github.com/heavymedl-labs/tetsu/issues/8). These are server-rendered
HTML with Web Component islands. A business can embed them in its own site as a frame for
browsing, and signing in and paying always happen at the top level
([ADR-0031](docs/adr/0031-front-end-model-revised.md)). The staff app, native customer apps, and
sites businesses build call the API. The staff app's technology is open
([#14](https://github.com/heavymedl-labs/tetsu/issues/14)), with WebAssembly the
intended choice. Every surface goes through the same use cases.

```mermaid
flowchart TB
    accTitle: Containers
    accDescr: Customers browse their business's booking pages in a browser, hosted or framed in its own site, and book and pay on Tetsu's pages, and prospective businesses read about Tetsu, both served by the Tetsu server. Staff run the business in the staff app, and customers may use native apps; their technology is open. Apps and partner software call the server's API over HTTPS, and the server sends webhooks to partner software. Browsers and apps send card details straight to payment processors' hosted fields. The server, one Rust binary, reads and writes PostgreSQL over SQL with one schema per module, stores documents and media in object storage, charges and refunds through payment processors, and messages, syncs, and imports through other outside systems: email and SMS, marketplaces, accounting tools, video, and previous systems.

    prospects([Prospective businesses])
    staff([Staff of each business])
    customers([Customers of each business])
    partners([Partner software])

    apps["Staff app and native customer apps<br/>technology open, #14"]
    server["Tetsu server<br/>Rust binary, serves server-rendered pages and API"]
    db[("PostgreSQL<br/>one schema per module")]
    files[("Object storage<br/>documents and media")]

    processors(["Payment processors<br/>hosted card fields"])
    services(["Email, SMS, marketplaces, accounting,<br/>video, previous systems"])

    staff -- runs the business in --> apps
    customers -- may use --> apps
    prospects -- reads about Tetsu, HTTPS --> server
    apps -- calls API, HTTPS --> server
    customers -- browses booking pages, hosted or framed; books and pays at the top level, HTTPS --> server
    customers -- enters card details in a browser, HTTPS --> processors
    partners -- calls API, HTTPS --> server
    server -- sends webhooks, HTTPS --> partners
    apps -- sends card details, HTTPS --> processors
    server -- reads and writes, SQL --> db
    server -- stores files, S3 API --> files
    server -- charges and refunds, HTTPS --> processors
    server -- messages, syncs, imports, HTTPS --> services

    classDef actor fill:#f1f5f9,stroke:#64748b,color:#0f172a
    classDef ours fill:#dbeafe,stroke:#2563eb,color:#1e3a8a
    classDef store fill:#dcfce7,stroke:#16a34a,color:#14532d
    classDef external fill:#f5f5f4,stroke:#78716c,color:#292524
    class prospects,staff,customers actor
    class apps,server ours
    class db,files store
    class partners,processors,services external
```

### Components

Inside the server. Inbound adapters call module use cases. Any module may call
another's use cases, except that the platform never calls a vertical. Modules
reach storage and outside systems through driven ports, talk through domain
events, and implement the kernel's contract. The composition root registers
every module. How a vertical changes platform behavior, such as a late-cancel
fee, is open ([#7](https://github.com/heavymedl-labs/tetsu/issues/7)): policy
ports the platform defines, verticals wrapping platform use cases, or
per-tenant configuration.

```mermaid
flowchart TB
    accTitle: Components of the Tetsu server
    accDescr: Inbound adapters, server-rendered HTML pages and a JSON HTTP API, call use cases of the modules, one crate each: verticals, shared capabilities, control plane, and connectors. Verticals call use cases of shared capabilities. Modules call driven ports that outbound adapters implement, reaching their own Postgres schema, object storage, and outside systems. Modules publish domain events, which a transactional outbox delivers to other modules (ADR-0043). Modules implement the kernel's module contract, and the composition root registers them.

    subgraph inbound[Inbound adapters]
        pages["Server-rendered pages<br/>HTML"] ~~~ api["HTTP API<br/>JSON, OpenAPI"]
    end

    subgraph modules[Modules, one crate each]
        verticals[Verticals] -- call use cases of --> shared[Shared capabilities]
        shared ~~~ control[Control plane] ~~~ connectors[Connectors]
    end

    subgraph out[Outside the modules]
        db[(Own Postgres schema)] ~~~ files[(Object storage)] ~~~ outside([Outside systems])
    end

    events{{Domain events}}
    kernel[["Kernel<br/>module contract, traits"]]
    root[["Composition root<br/>main, wires modules"]]

    inbound -- call use cases of --> modules
    modules -- call driven ports --> out
    modules -- publish --> events
    events -- deliver to --> modules
    modules -- implement --> kernel
    root -- registers --> modules

    classDef ours fill:#dbeafe,stroke:#2563eb,color:#1e3a8a
    classDef core fill:#ede9fe,stroke:#7c3aed,color:#3b0764
    classDef store fill:#dcfce7,stroke:#16a34a,color:#14532d
    classDef external fill:#f5f5f4,stroke:#78716c,color:#292524
    class pages,api,verticals,control,shared,connectors ours
    class kernel,events,root core
    class db,files store
    class outside external
```

### Module

Every module has the same hexagonal shape: use cases in, driven ports out. Outside the module, its use cases
are reached only through a handle that its adapter constructors return, so no caller can skip them by calling a
port ([ADR-0021](docs/adr/0021-module-public-surface.md)).

```mermaid
flowchart LR
    accTitle: One module
    accDescr: Inbound adapters and other modules call a module's use cases. Use cases change the domain model, call driven ports, which are Rust traits implemented by outbound adapters, and record domain events. Outbound adapters read and write the module's own Postgres schema and call outside systems. Domain events are published to other modules.

    callers[Inbound adapters<br/>and other modules]

    subgraph module[One module]
        usecases{{Use cases}}
        domain[Domain model]
        ports[/"Driven ports<br/>Rust traits"/]
        adapters[Outbound adapters]
        events[Domain events]
        usecases -- change --> domain
        usecases -- call --> ports
        ports -- implemented by --> adapters
        usecases -- record --> events
    end

    schema[(Its own Postgres schema)]
    outside([Outside system])
    subscribers[Other modules]

    callers -- call --> usecases
    adapters -- read and write --> schema
    adapters -- call --> outside
    events -- published to --> subscribers

    classDef ours fill:#dbeafe,stroke:#2563eb,color:#1e3a8a
    classDef core fill:#ede9fe,stroke:#7c3aed,color:#3b0764
    classDef store fill:#dcfce7,stroke:#16a34a,color:#14532d
    classDef external fill:#f5f5f4,stroke:#78716c,color:#292524
    class callers,subscribers,usecases,domain,ports,adapters ours
    class events core
    class schema store
    class outside external
```

### Tenancy

A tenant is one business: a brand and all its locations. Every request resolves
its tenant, and every query is scoped to it. Customer accounts belong to their
business ([ADR-0040](docs/adr/0040-identity-and-sign-in.md)). Tenants share
tables, with a tenant ID on every tenant-owned row, enforced by Postgres
row-level security ([ADR-0042](docs/adr/0042-tenant-isolation.md)).

```mermaid
flowchart TB
    accTitle: Tenancy
    accDescr: Business A, a brand with two locations, and Business B, a brand with one location, both use the same Tetsu server. The server resolves the tenant on every request and scopes every query to it. PostgreSQL keeps shared tables with a tenant ID on every row, enforced by row-level security, so no tenant sees another's data.

    subgraph a[Business A, one brand]
        a1[Location 1] ~~~ a2[Location 2]
    end

    subgraph b[Business B, one brand]
        b1[Location 1]
    end

    server["Tetsu server<br/>resolves the tenant on every request"]
    db[("PostgreSQL<br/>tenant ID on every row,<br/>row-level security")]

    a -- staff and customers sign in to --> server
    b -- staff and customers sign in to --> server
    server -- queries scoped to one tenant, SQL --> db

    classDef actor fill:#f1f5f9,stroke:#64748b,color:#0f172a
    classDef ours fill:#dbeafe,stroke:#2563eb,color:#1e3a8a
    classDef store fill:#dcfce7,stroke:#16a34a,color:#14532d
    class a1,a2,b1 actor
    class server ours
    class db store
```

### Request flow

Provisional: one booking, end to end. Sign-in, tenant isolation, and event
delivery follow [ADR-0040](docs/adr/0040-identity-and-sign-in.md),
[ADR-0042](docs/adr/0042-tenant-isolation.md), and
[ADR-0043](docs/adr/0043-domain-event-delivery.md).

```mermaid
sequenceDiagram
    accTitle: Booking a session
    accDescr: The customer app asks the HTTP API to book a session. The API verifies the token with Identity, which returns the tenant and customer, and calls Booking with that tenant context. Booking checks with Authorization, uses a credit from Entitlements, and saves the booking and its BookingConfirmed event in one transaction. The API returns 201 Created. The event is delivered from the outbox to Notifications, which emails the customer, and to Audit, which records who booked what.
    autonumber
    participant app as Customer app
    participant api as HTTP API
    participant identity as Identity
    participant booking as Booking
    participant authz as Authorization
    participant entitlements as Entitlements
    participant db as PostgreSQL
    participant notify as Notifications
    participant audit as Audit

    app->>api: Book session, HTTPS with token
    api->>identity: Verify token
    identity-->>api: Tenant and customer
    api->>booking: Book session, with tenant context
    booking->>authz: May this customer book?
    authz-->>booking: Allowed
    booking->>entitlements: Use a credit
    entitlements-->>booking: Credit used
    booking->>db: Save booking and BookingConfirmed, one transaction
    booking-->>api: Confirmed
    api-->>app: 201 Created
    db--)notify: BookingConfirmed, from the outbox
    db--)audit: BookingConfirmed, from the outbox
    Note over notify,audit: Email the customer, record who booked what
```

### Deployment

The shape stays the same as we grow. The first environment runs on free tiers of Render
and Neon with fictional data, and moves to paid before a real business depends on Tetsu
([ADR-0012](docs/adr/0012-first-hosting-and-deploys.md)). The maintainer deploys a commit CI has checked, using
the image CI built ([ADR-0033](docs/adr/0033-deploys-on-demand.md)). Here "instances" are running
copies of the `tetsu` OCI image, not C4 containers.

```mermaid
flowchart TB
    accTitle: Deployment
    accDescr: GitHub Actions builds and tests Tetsu and produces an OCI image, which is pushed to a container registry. GitHub Actions then runs tetsu migrate against PostgreSQL, and a failure stops the deploy. One or more tetsu instances on a hosting platform pull the image from it. Apps and browsers reach the instances over HTTPS through TLS and a load balancer, which forwards to them. Secrets and environment config configure the instances. Instances use PostgreSQL over SQL, object storage over the S3 API, and outside systems over HTTPS.

    subgraph ci[GitHub Actions]
        build[Build and test] -- produces --> image[OCI image]
        migrate[tetsu migrate<br/>a failure stops the deploy]
    end

    registry[(Container registry)]
    clients([Apps and browsers])

    subgraph host[Hosting platform]
        lb[TLS and load balancer]
        app["tetsu instances<br/>OCI containers, one or more"]
        config[Secrets and env config]
        lb -- forwards HTTPS to --> app
        config -- configures --> app
    end

    pg[("PostgreSQL<br/>managed, backups")]
    files[("Object storage<br/>S3 compatible")]
    outside([Outside systems])

    image -- pushed to --> registry
    migrate -- changes schema, SQL --> pg
    registry -- image pulled by --> app
    clients -- sends requests, HTTPS --> lb
    app -- reads and writes, SQL --> pg
    app -- stores files, S3 API --> files
    app -- calls, HTTPS --> outside

    classDef actor fill:#f1f5f9,stroke:#64748b,color:#0f172a
    classDef ours fill:#dbeafe,stroke:#2563eb,color:#1e3a8a
    classDef store fill:#dcfce7,stroke:#16a34a,color:#14532d
    classDef external fill:#f5f5f4,stroke:#78716c,color:#292524
    class clients actor
    class build,image,migrate,app,lb,config ours
    class pg,registry,files store
    class outside external
```

## Quality goals

- Every surface, web or native, feels fast and intuitive. Performance budgets
  and an accessibility target come with the front-end decision
  ([#14](https://github.com/heavymedl-labs/tetsu/issues/14)).
- Identity, money, and tenant isolation get the highest testing rigor, and hot
  paths are benchmarked on every change
  ([ADR-0005](docs/adr/0005-testing-and-performance.md)).

## Known limits

What we expect to reach as Tetsu grows, most likely first. Nothing here is measured yet:
each is an estimate until a capacity run tests it
([ADR-0012](docs/adr/0012-first-hosting-and-deploys.md#benchmarking)).

| Limit | What happens | Planned response | Where it is decided |
| --- | --- | --- | --- |
| Outside services | Payment processors and SMS providers limit our rate during bursts such as waitlist messages or monthly renewals | Send through a queue with pacing and retries | [#11](https://github.com/heavymedl-labs/tetsu/issues/11) |
| Peaks at the same moment | Businesses share popular times, and bookings can open at a set moment, so load does not average out | Test with many simulated tenants, and consider staggering when bookings open | [ADR-0012](docs/adr/0012-first-hosting-and-deploys.md) and booking design |
| One tenant affecting others | Shared deployments mean a heavy report, a bad deploy, or a slow migration reaches every tenant | Per-tenant rate limits, timeouts, staged rollouts, and the option of separate [cells](docs/GLOSSARY.md) | [#8](https://github.com/heavymedl-labs/tetsu/issues/8) |
| Database connections | Instances times pool size reaches the host's limit, sooner if transactions hold connections longer | Pools that fail fast when full, no outside calls inside a transaction, and the app near Postgres | Pool limits in [ADR-0011](docs/adr/0011-server-stack-guardrails.md); the rest is not decided yet |
| Popular resources | Many customers compete for the same spots at once, so those requests slow down or fail | Concurrency tests and a tested locking choice | [ADR-0005](docs/adr/0005-testing-and-performance.md) and booking design |
| Postgres writes and growth | One primary takes all writes, and the ledger and audit log grow forever | Partition by time early, start indexes with tenant ID, and shard by tenant if ever needed | [#8](https://github.com/heavymedl-labs/tetsu/issues/8) |

## Invariants

The hard rules are in [docs/invariants.md](docs/invariants.md).
