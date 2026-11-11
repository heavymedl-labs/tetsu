# 0044. Email delivery

- Status: Proposed
- Date: 2026-11-11

## Context

Sign-in sends email: codes for signing up and recovering, recovery notices that must never be lost, and dispute
links ([ADR-0040](0040-identity-and-sign-in.md), [ADR-0041](0041-account-recovery.md)). ADR-0040 asked for an email
provider, chosen in its own ADR, before sign-in.

Two facts change what can be chosen now. Tetsu has no domain until a client showcase is close, and every
reputable provider needs a domain Tetsu controls, with SPF, DKIM, and DMARC records, before it delivers to anyone's
inbox. Gmail and Yahoo have required those records of bulk senders since February 2024. And Tetsu must stay
portable, so no provider's API may reach into the modules.

## Decision

Email goes out through a **driven port,** and no provider is chosen until Tetsu has a domain.

- **A `Mailer` port** takes a message: recipient, subject, a plain-text body, an HTML body rendered from a
  template, and an idempotency key derived from the event that caused it ([ADR-0043](0043-domain-event-delivery.md)).
  Modules send through it from outbox consumers, never inside a request's transaction.
- **The demo uses a capture adapter.** It stores each message in Postgres instead of sending it, and the
  maintainer reads them on an operator-only page. Demo accounts are fictional, so no real inbox is needed, and
  nothing is sent from a domain Tetsu does not own.
- **Production uses an SMTP adapter.** SMTP with STARTTLS or implicit TLS is the one protocol every provider
  accepts, so changing provider is a change of settings, not of code. A provider's HTTP API is used only if SMTP
  lacks something Tetsu needs, behind the same port.
- **The provider is chosen with Tetsu's domains** ([#102](https://github.com/heavymedl-labs/tetsu/issues/102)),
  through [ADR-0010](0010-evaluating-technology.md)'s steps, before any real person signs up. Its criteria are
  set now:
  - SMTP submission, so the adapter needs no provider code
  - SPF, DKIM, and DMARC alignment on Tetsu's own domain
  - a free or low tier that covers the showcase, and a published price beyond it
  - bounce and complaint reports, so Tetsu stops mailing addresses that fail
  - data processing terms suitable for customer email addresses
- **Every message goes out once per idempotency key,** and the adapter records each send, so a retried consumer
  does not mail twice.
- **Messages hold no secrets beyond what they must.** A code or link is single use and short-lived, as ADR-0040
  and ADR-0041 set, and messages never include passkey or session details.

## Alternatives considered

- **Pick a provider now on a shared sending domain.** Free tiers that send without your own domain deliver only to
  the account holder's own address, and anything sent from a shared domain would move later anyway.
- **Run Tetsu's own mail server.** Deliverability depends on IP reputation that a new server lacks, and it adds a
  service to run.
- **A provider's HTTP API from the start.** Richer features, but it ties the adapter to one provider, which goes
  against portability.

## Consequences

- Sign-in ([#20](https://github.com/heavymedl-labs/tetsu/issues/20)) can be built and used on the demo with no
  provider and no cost.
- The capture page is an operator-only route and follows ADR-0040's operator rules.
- Choosing a provider becomes part of #102, with its own ADR, before the Level 2 review.
- The SMTP adapter, and the library it uses, come with the provider, through ADR-0010. Until then only the
  capture adapter exists, and production deliverability is untested.

## Sources

- Google, [Email sender guidelines](https://support.google.com/a/answer/81126), bulk sender requirements from
  February 2024.
- Yahoo, [Sender best practices](https://senders.yahooinc.com/best-practices/).
- RFC 6409, Message Submission for Mail; RFC 8314, Cleartext Considered Obsolete: Use of TLS for Email Submission.
