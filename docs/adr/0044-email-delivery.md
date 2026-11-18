# 0044. Email delivery

- Status: Accepted
- Date: 2026-11-11
- The SMTP adapter and the choice of its library moved from #110 to the first slice that emails a real person by
  [ADR-0057](0057-operator-passkeys-before-level-2.md), and back into #110 by [ADR-0074](0074-operator-email-in-development.md)
- The choice of Resend superseded, until Tetsu has a domain, by [ADR-0074](0074-operator-email-in-development.md)

## Context

Login sends email: codes for signing up and recovering, recovery notices that must never be lost, and dispute
links ([ADR-0040](0040-identity-and-sign-in.md), [ADR-0041](0041-account-recovery.md)). ADR-0040 asked for an email
provider, chosen in its own ADR, before login.

Tetsu has no domain until a client showcase is close. Every reputable provider needs a domain the sender
controls, with SPF, DKIM, and DMARC records, before it delivers to other people, and Gmail and Yahoo have required
those records of bulk senders since February 2024. Free tiers do let an account send to its own address without a
domain. Tetsu also stays portable, so no provider's API may reach into the modules.

[ADR-0010](0010-evaluating-technology.md)'s steps 1, 4, and 7 apply, since a provider behind SMTP is cheap to
replace and on no hot path. **Criteria:** SMTP submission, so the adapter needs no provider code; a free tier
that sends now without a domain, at least to the maintainer; SPF, DKIM, and DMARC on Tetsu's own domain later;
bounce and complaint reports; and published prices beyond the free tier.

## Decision

Email goes out through a **driven port,** over SMTP, through **Resend's free tier.**

- **A `Mailer` port** takes a message: recipient, subject, a plain-text body, an HTML body rendered from a
  template, and an idempotency key derived from the event that caused it ([ADR-0043](0043-domain-event-delivery.md)).
  Modules send through it from outbox consumers, never inside a request's transaction.
- **An SMTP adapter** sends through any provider. SMTP with STARTTLS or implicit TLS is the one protocol every
  provider accepts, so changing provider is a change of settings.
- **Resend is the provider now.** Its free tier sends 3,000 emails a month and 100 a day, over SMTP at
  `smtp.resend.com`. Until a domain is verified, it sends only from its test address, `onboarding@resend.dev`, and
  only to the address on the Resend account, which is the maintainer's. With Tetsu's domain
  ([#102](https://github.com/heavymedl-labs/tetsu/issues/102)), the domain is verified with Resend, or the
  provider changes, with no code change either way.
- **A capture adapter** stores messages in Postgres instead of sending them, for recipients the provider cannot
  reach yet: the demo's fictional people. The maintainer reads them on an operator-only page. Tests use it as
  ADR-0040's email sink. A setting lists the addresses that go out by SMTP; every other message is captured.
- **Every message goes out once per idempotency key,** and each adapter records each send, so a retried consumer
  does not mail twice.
- **Messages hold no secrets beyond what they must.** A code or link is single use and short-lived, as ADR-0040
  and ADR-0041 set, and messages never include passkey or session details.

## Alternatives considered

- **Brevo.** A free tier of 300 emails a day with SMTP, but sending waits for account approval, and whether a
  single sender address works without a domain was not confirmed.
- **Amazon SES.** Its sandbox sends to verified addresses without a domain, but needs an AWS account with a card,
  and its free allowance lasts 12 months.
- **Capture only, choosing a provider with the domain.** Free and simple, but production email would go untested
  until the showcase.
- **Run Tetsu's own mail server.** Deliverability depends on IP reputation that a new server lacks.
- **A provider's HTTP API.** Richer features, but it ties the adapter to one provider.

## Consequences

- Login ([#20](https://github.com/heavymedl-labs/tetsu/issues/20)) sends real email to the maintainer from the
  start, and captures the rest, at no cost.
- The SMTP library is chosen through ADR-0010 when the adapter is built, in
  [#110](https://github.com/heavymedl-labs/tetsu/issues/110).
- The Resend credentials are a secret in the host's environment, never in the repository or logs.
- 100 emails a day is ample for one maintainer. Revisit when real people sign up, and before the Level 2 review.

## Sources

- Resend, [403 error using the resend.dev domain](https://resend.com/docs/knowledge-base/403-error-resend-dev-domain),
  [account quotas and limits](https://resend.com/docs/knowledge-base/account-quotas-and-limits), and
  [sending with SMTP](https://resend.com/docs/send-with-phpmailer-smtp), read 2026-11-11.
- Brevo, [free SMTP server](https://www.brevo.com/en/free-smtp-server/), read 2026-11-11.
- Google, [Email sender guidelines](https://support.google.com/a/answer/81126), bulk sender requirements from
  February 2024.
- RFC 6409, Message Submission for Mail; RFC 8314, Cleartext Considered Obsolete: Use of TLS for Email Submission.
