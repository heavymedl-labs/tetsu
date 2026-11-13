# Audit

The record of security events, such as sign-ins, passkey changes, recoveries, and relinks, kept per tenant and
never lost ([ADR-0045](../../docs/adr/0045-operations-baseline.md)). Tier 1, since it records identity and access.

Audit builds its record from domain events ([ADR-0043](../../docs/adr/0043-domain-event-delivery.md)).
`consumer(&["identity"])` records every event from the named modules: one row per event in `audit.records` for
the event's tenant, or in `audit.records_global` for events that belong to no business, such as an operator's
sign-in. Each row keeps the event's ID, producer, type, version, time, and payload, which holds IDs and facts,
never personal details. The record is append-only: Audit's own role may add and read rows, and never change or
remove one.

No module records events yet. Identity, the first, arrives with operator sign-in
([ADR-0048](../../docs/adr/0048-sign-in-design.md), slice 5), and `tetsu serve` starts the dispatcher then.
