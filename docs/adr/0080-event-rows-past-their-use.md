# 0080. Event rows past their use

- Status: Accepted
- Date: 2026-11-22
- Supersedes [ADR-0043](0043-domain-event-delivery.md)'s "stops until a fix replays it", and
  [ADR-0045](0045-operations-baseline.md)'s stopped event logged "so it can be found and replayed", which kept it
  forever, and widens the delivery role to the inboxes' IDs

## Context

[ADR-0043](0043-domain-event-delivery.md) deletes delivered events after 7 days, but two kinds of its rows are never
deleted, so the database grows with every event
([#153](https://github.com/heavymedl-labs/tetsu/issues/153)):

- **Each consumer's inbox** keeps the ID of every event it applied, to skip a repeat. A repeat can come only while
  the event is still in its producer's outbox, undelivered. Once the outbox row is gone, the ID guards nothing.
- **An event that stopped retrying,** after 10 failures and not marked never to be lost, is never delivered, so the
  7-day cleanup never takes it. ADR-0043 keeps it "until a fix replays it", with no limit.

The [database design review](../research/database-design-review.md) found the same of stopped events. An event
marked never to be lost retries without end, so no fixed age makes an inbox ID safe to drop on its own.

## Decision

- **An inbox keeps an applied event's ID for 30 days, and after that only while some outbox still holds the
  event.** The check is against every outbox of the same kind the database holds, found from its catalog when the
  cleanup runs, so an instance of an older build never misses a module a newer one added: a tenant inbox against
  tenant outboxes, and a global inbox against global ones. An event still retrying, kept or not, keeps its ID
  however old it is.
- **An event that stopped retrying is kept for 30 days after its last attempt,** long enough to find the failure from
  its error log and replay it, then deleted. An event marked never to be lost is never deleted undelivered, and
  one is marked so whenever a consumer that must never lose it failed or was not reached as it ran out of
  attempts.
- **The dispatcher removes both in its hourly cleanup,** after delivered events, as the delivery role, and logs how
  many of each it removed. The outboxes' rows go in one transaction, and inbox IDs in batches of a thousand that
  each commit, at most twenty a pass across every inbox, since delivery waits for the cleanup: what is left goes in
  the next pass.
- **The delivery role may read and delete the inboxes,** through one policy across tenants, as it does the outboxes,
  and nothing more: it never adds to an inbox or changes one. Each consuming module adds a shared migration that
  grants this, and the catalog check allows exactly it, on a table with the shared inbox's exact columns alone, so
  a module's own table named `inbox` stays closed. The outboxes an ID is checked against are the tables that hold
  the delivery role's own policy, as the shared outbox does, so a table named `outbox` elsewhere never counts, and a
  shared one the role can no longer read fails the cleanup rather than being passed over. A dispatcher refuses at start a consumer whose inboxes lack it.

## Alternatives considered

- **A fixed age alone for inbox IDs.** Simpler, but an event marked never to be lost that retries past the age
  would be applied twice once its ID went.
- **Each module removes its own inbox IDs.** It keeps the delivery role narrower, but a module's role cannot see
  other modules' outboxes, so it could only use a fixed age, and a tenant inbox would need a pass per tenant.
- **Let the database hold the 30-day floor,** with a delete policy on each inbox that admits only IDs over 30
  days old. It would stop a misused delivery role from dropping a recent ID, but the global inbox has no row
  security to carry a policy, and adding it changes every consumer's global table and the catalog check for a risk
  the application role already holds: it can act as the delivery role. Revisit with the Level 2 review.
- **Keep stopped events until someone deletes them by hand.** Nothing reminds anyone, so they would pile up, as
  they do now.

## Consequences

- The outbox and inboxes stay bounded by the events of the last 30 days, plus any still retrying.
- A stopped event cannot be replayed after 30 days. Its error log names it the day it stops, and operations alerts
  on that log ([#27](https://github.com/heavymedl-labs/tetsu/issues/27)).
- The delivery role reaches one more table in each consuming module, and with it a new risk: code that reaches
  the delivery role, which the application role may act as, could delete an inbox ID early and so have an event
  still held applied twice. Before, it could at most redeliver an event the inbox then skipped.
- The cleanup names schemas it finds in the catalog, so it quotes each name as Postgres quotes an identifier, and
  it stops, with an error, if an outbox the dispatcher delivers is no longer found as a shared one.
- The first cleanup removes at once every stopped event over 30 days old, including any a never-lose consumer was
  not reached for before the rule that keeps those. The demo holds no real business's data, so none is lost that
  matters; a database that did would need its stopped events replayed or kept first.
- A large backlog of inbox IDs, as the first cleanup meets, goes over many passes, twenty thousand an hour.
- The audit record is not an inbox or an outbox; its retention is
  [#22](https://github.com/heavymedl-labs/tetsu/issues/22)'s to decide.

## Sources

- Chris Richardson, [Pattern: Transactional outbox](https://microservices.io/patterns/data/transactional-outbox.html),
  and [Idempotent consumer](https://microservices.io/patterns/communication-style/idempotent-consumer.html), the
  pattern each inbox follows.
