# 0043. Domain event delivery

- Status: Accepted
- Date: 2026-11-11

## Context

Modules react to each other through domain events ([ADR-0003](0003-architecture.md)), and outside systems will
receive them as webhooks. The reference businesses set the bar. A late cancel at Anvil Row must promote the
waitlist, charge a fee, and notify customers, each exactly once. A failed renewal must not send its messages again
after a crash. A sign-in writes an audit entry, the first event Tetsu delivers
([#20](https://github.com/heavymedl-labs/tetsu/issues/20)), and account recovery's notices must never be lost
([ADR-0041](0041-account-recovery.md)).

Tetsu is one binary on one Postgres database, runs on free hosting that sleeps when idle, and must stay portable
across hosts. Tenant data is isolated by row-level security ([ADR-0042](0042-tenant-isolation.md)).

## Decision

Events go through a **transactional outbox in Postgres,** delivered at least once to consumers that apply each
event once. No broker is added.

### Recording

- **A module records an event in the same transaction as the change it describes,** in an `outbox` table in its
  own schema. Either both commit or neither does, so no change goes unannounced and no event describes a change
  that rolled back.
- **A module has two outboxes.** `outbox` is tenant-owned under ADR-0042, keyed `(tenant_id, id)` with a UUIDv7
  `id`, for events that belong to one business. `outbox_global` is a global table for events with no business, such
  as an operator's sign-in or a staff member's before they choose a business. A row holds the event's type and
  version, when it occurred, and a JSON payload.
- **An event names a fact in the past tense,** such as `identity.signed_in` or `booking.cancelled_late`. Its type
  carries a version, and a change to a payload's meaning adds a new version rather than altering the old one.
- **Payloads carry IDs and facts, not personal details,** such as an account's ID rather than its email. Consumers
  look up what they need, so erasing a person does not mean rewriting history.

### Delivery

- **A dispatcher in the same binary** reads each module's outboxes, oldest first, using `FOR UPDATE SKIP LOCKED` so
  a second running instance never takes the same rows. It polls about once a second, and wakes at once
  when a module records an event in the same process. A sleeping host simply delivers late when it wakes.
- **It reads with a delivery role** whose own policies (ADR-0042) allow every tenant's rows on outbox and inbox
  tables only, and which has no rights anywhere else. It hands each event to a consumer, which works in its own tenant transaction.
- **Consumers subscribe through use cases** of their own module, never by reading another module's tables.
- **Delivery is at least once.** An event is marked delivered for a consumer only after that consumer commits, so a
  crash means a retry, never a loss.
- **Delivery promises no order.** Two instances skip each other's locked rows, transactions commit in a different
  order from when they began, and a failing event lets later ones pass. A consumer that cares about order checks
  the state it holds, such as ignoring a cancellation for a booking it has not seen and retrying it later.
- **Failures retry with exponential backoff and jitter,** capped at one attempt an hour. After 10 failed attempts the
  failure is logged with the event's ID, type, and consumer, and operations ([#27](https://github.com/heavymedl-labs/tetsu/issues/27))
  alerts on it. An event marked as never to be lost, such as a recovery notice ([ADR-0041](0041-account-recovery.md)),
  keeps retrying until it succeeds; any other event stops until a fix replays it.
- **Delivered events are deleted after 7 days.** The outbox is a delivery mechanism, not a history. Audit and
  analytics keep their own records, built from events.

### Applying each event once

- **Every consumer keeps an inbox** in its own schema: the IDs of the events it has applied. It records the ID in
  the same transaction as the event's effect, and skips an ID it already holds. At-least-once delivery then has
  exactly-once effect inside Tetsu.
- **An effect outside Tetsu,** such as an email or a payment call, cannot share Tetsu's transaction. It sends an
  idempotency key derived from the event's ID where the outside system accepts one. Where it does not, the
  connector chooses by what is worse. Messages, such as notices and codes, are sent and then recorded, so a crash
  can repeat a message but never lose it. Money and other effects that must not repeat record the attempt first,
  and a missed one is found and settled by hand. Each connector states which it does.

### Webhooks

- **Webhooks are a consumer of the outbox,** signed and retried as the [Standard Webhooks](https://www.standardwebhooks.com/)
  specification describes: an HMAC-SHA256 signature over the event's ID, timestamp, and body, with the event's ID
  sent so receivers can drop duplicates. The webhook contract itself, its event catalog and endpoints, belongs to
  the public API ([#12](https://github.com/heavymedl-labs/tetsu/issues/12)).

## Alternatives considered

- **An in-process bus only.** Simplest, but an event is lost if the process stops between the commit and the send,
  which breaks the exactly-once needs above.
- **A message broker,** such as Kafka, NATS, RabbitMQ, or a cloud queue. Built for scale Tetsu does not have, adds a
  service to run or pay for, and still needs an outbox to publish reliably. A broker can be fed from the outbox
  later if one is needed.
- **Postgres `LISTEN`/`NOTIFY` alone.** A notification is lost when nothing is listening, and transaction-mode
  poolers do not support `LISTEN`.
- **Change data capture from the write-ahead log,** as Debezium does. Needs replication slots and host support that
  varies, and couples events to table layout.
- **Event sourcing.** Rebuilds state from events. Powerful, but changes how every module stores data, which the
  reference businesses do not need.

## Consequences

- Modules gain an `outbox` and an `inbox` table in their own schemas, plus a shared library in the Postgres crate
  for recording, dispatching, and the inbox check. `outbox` and `inbox` are declared tenant-owned, and
  `outbox_global` and its inbox global.
- The first event is sign-in's `identity.signed_in`, consumed by an audit record
  ([#20](https://github.com/heavymedl-labs/tetsu/issues/20)).
- Consumers must tolerate a repeat and must not rely on order. Tests deliver each event twice and in shuffled
  order.
- Delivery lags by up to the poll interval, and longer while a free host sleeps. Nothing in the reference
  businesses needs faster.
- Polling costs a cheap query a second per module while awake, which on a free database counts against compute
  hours. The interval is configurable.
- Revisit if delivery volume outgrows one Postgres, or a consumer needs strict order, which would take one locked
  cursor per module.

## Sources

- Chris Richardson, ["Pattern: Transactional outbox"](https://microservices.io/patterns/data/transactional-outbox.html)
  and ["Pattern: Idempotent consumer"](https://microservices.io/patterns/communication-style/idempotent-consumer.html),
  microservices.io.
- PostgreSQL 18 documentation, [SELECT, the locking clause](https://www.postgresql.org/docs/18/sql-select.html#SQL-FOR-UPDATE-SHARE)
  (`SKIP LOCKED`).
- [Standard Webhooks specification](https://www.standardwebhooks.com/).
- [Market Scan platform implications](../research/market-scan/platform-implications.md): transactional outbox as the
  leading option.
