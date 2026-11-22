# 0079. No mail provider before a domain

- Status: Accepted
- Date: 2026-11-21
- Supersedes [ADR-0074](0074-operator-email-in-development.md)'s choice of Mailgun's sandbox as the provider until
  Tetsu has a domain

## Context

[ADR-0074](0074-operator-email-in-development.md) chose Mailgun's sandbox to reach a few real inboxes before Tetsu
has a domain, because it delivered to five invited recipients "with no card", and said to revisit if Mailgun changed
its sandbox terms. On 2026-11-21, signing up asked for a card, so the premise no longer holds.

The [email without a domain research](../research/email-without-a-domain.md) left two other ways to real inboxes
without a domain: Gmail with an app password, which needs a Google account made only for Tetsu, since the password
opens all of it, and a domain at about $10.50 a year, which waits for a client showcase to be close
([#102](https://github.com/heavymedl-labs/tetsu/issues/102)).

Meanwhile the whole mail path was shown end to end on 2026-11-21: through the local Mailpit
([#149](https://github.com/heavymedl-labs/tetsu/issues/149)), an operator received their invite, then a notice of
their login and of the passkey they added, over SMTP with TLS and a login, as a provider receives them. A real inbox
would add only one provider's handling, such as its spam filtering.

## Decision

- **No mail provider until Tetsu has a domain.** The demo sets no SMTP settings, so it keeps notices for its
  mailbox page and prints invites, as ADR-0074 already allows without them.
- **The local Mailpit is the end-to-end check,** for #110 and for any later change to mail.
- **Real inboxes come with the domain** ([#151](https://github.com/heavymedl-labs/tetsu/issues/151)), through
  [ADR-0044](0044-email-delivery.md)'s provider, set in settings alone.
- Everything else in ADR-0074 stands: operators' addresses, emailed invites and notices, the Mail module, the SMTP
  adapter, and Mailpit.

## Alternatives considered

- **Mailgun with a card on file.** Its free plan may still cost nothing, but a card held by a vendor for a demo is
  what ADR-0074 chose Mailgun to avoid.
- **Gmail with an app password.** Free, no card, and reaches anyone, but the password opens a whole account, so it
  needs one made for this, kept, and secured, for a check the local Mailpit already covers.
- **Buy the domain now.** The lasting fix, but before a client showcase is close, against how Tetsu spends.

## Consequences

- No one receives real email from the demo until #102. Testers see invites in the terminal and notices on the
  mailbox page.
- Provider behavior, such as spam filtering and sender reputation, goes untested until the domain arrives.
- No mail secret exists to keep, rotate, or leak.

## Sources

- Mailgun's signup, on 2026-11-21, asking for a card before any sending.
- [Email without a domain](../research/email-without-a-domain.md), the options compared on 2026-11-18.
