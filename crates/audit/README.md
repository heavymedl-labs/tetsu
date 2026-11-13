# Audit

The record of security events, such as sign-ins, passkey changes, recoveries, and relinks, kept per tenant and
never lost ([ADR-0045](../../docs/adr/0045-operations-baseline.md)). Tier 1, since it records identity and access.

Audit builds its record from domain events ([ADR-0043](../../docs/adr/0043-domain-event-delivery.md)).
`consumer(&["identity"])` records every event from the modules with those schemas: one row per event in
`audit.records` under the tenant the event names, or in `audit.records_global` for events that belong to no
business, such as an operator's sign-in. Each row keeps the event's ID, producer, type, version, time, and payload.

- **Never lost.** Audit is a never-lose consumer, so an event it fails to record keeps retrying, hourly at most,
  however often it fails.
- **Append-only.** Audit's role may add and read rows, and never change, remove, or truncate one.
- **Payloads are kept as producers send them.** ADR-0043 has producers send IDs and facts, never personal details;
  Audit cannot check that, and an append-only row cannot be edited later, so a producer's event types are reviewed
  for it.
- **The global record has no tenant policy,** so a future use case that shows a business its record reads
  `audit.records` alone.

## Hot paths

- **Recording.** One insert per event, inside the dispatcher's transaction, beside the inbox insert. Its indexes are
  the primary key and one unique index on `(tenant_id, event_id)`.
- **Reading.** No use case reads the record yet. The first one, such as a business viewing its sign-ins, adds an
  index on `(tenant_id, occurred_at)` and partitions by time, as ARCHITECTURE.md plans for the audit log.

## Not yet

No module records events yet. Identity, the first, arrives with operator sign-in
([ADR-0048](../../docs/adr/0048-sign-in-design.md), slice 5). `tetsu serve` starts the dispatcher then, with a test
that an Identity event reaches the record, which also catches a consumer given the wrong schema name.
