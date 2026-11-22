# 0074. Operator email in development

- Status: Accepted
- Choice of Mailgun's sandbox superseded by [ADR-0079](0079-no-mail-provider-before-a-domain.md)
- Date: 2026-11-18
- Supersedes, until Tetsu serves a real business, [ADR-0040](0040-identity-and-sign-in.md)'s rule that an operator's
  invite link prints only to the terminal, and [ADR-0044](0044-email-delivery.md)'s choice of Resend until Tetsu has
  a domain
- Moves [ADR-0044](0044-email-delivery.md)'s SMTP adapter, and the choice of its library, back into
  [#110](https://github.com/heavymedl-labs/tetsu/issues/110), reversing that part of
  [ADR-0057](0057-operator-passkeys-before-level-2.md)

## Context

ADR-0057 left #110 to build the `Mailer` port, its capture adapter, and the page that reads it, and moved the SMTP
adapter to the first slice that emails a real person, since nothing in #110 sent email. The same is true of the port
and capture adapter: built now, they would have no caller until staff invites ([#111](https://github.com/heavymedl-labs/tetsu/issues/111)).

The maintainer wants email built and tested end to end in #110, against real inboxes: their own, and a few people
they name. Operators are people working on Tetsu while it is in development, and reach only fictional data
(ADR-0057), so they can be the first to receive it.

Tetsu has no domain until a client showcase is close. Resend, ADR-0044's provider, delivers only to its account
owner without one. The [research](../research/email-without-a-domain.md) found two ways to reach a few chosen people
without a domain over SMTP: Mailgun's sandbox and a Gmail account with an app password. Teams usually test SMTP
against a local mail catcher such as Mailpit.

[ADR-0010](0010-evaluating-technology.md)'s steps 1, 4, and 7 apply, since a provider behind SMTP and a test tool
are cheap to replace and on no hot path. **Criteria:** real delivery to the maintainer and up to five people they
name, at no cost; SMTP, so a provider changes in settings; credentials that can only send; and SMTP tests that run
in the checks with no provider and no secret.

## Decision

- **Operators have an email address,** given to `tetsu operator create` beside their name. It is a plain address:
  no quoted local part, no line break, and no display name from input.
- **Operators receive three emails:**
  - **Their invite.** The command emails the link after storing the invite, and still prints it. It sends through
    the `Mailer` directly, not through an event, so the link never enters the outbox or the audit record, and keys
    the send on the invite's hash. The link stays single use and lasts an hour (`v5.0.0-6.4.1`).
  - **A notice of each login,** from an outbox consumer of `identity.logged_in`: when, and the session's coarse
    device label (`v5.0.0-6.3.5`).
  - **A notice of each passkey added,** from an outbox consumer of `identity.passkey_added` (`v5.0.0-6.3.7`).

  Notices carry no link, and say to ask the maintainer to disable the account if the operator did not do this.
- **#110 builds ADR-0044's mail parts in full:** the `Mailer` port, defined by the kernel and wired by the binary;
  a Mail module owning a `mail` schema, with the capture adapter and a record of each send by idempotency key; the
  SMTP adapter; the allowlist setting, which sends listed addresses by SMTP and captures the rest; and the
  operator-only mailbox page.
- **The SMTP adapter uses `lettre`,** at least 0.11.24, which fixed six advisories published on 2026-11-17. It
  connects with implicit TLS on port 465, never STARTTLS, and sends only to addresses the allowlist names.
- **Mailgun's sandbox is the provider until Tetsu has a domain.** It delivers to up to five recipients, each of
  whom accepts an invitation from Mailgun first, with no card. Its credentials can only send. They are a secret in
  the host's environment and in `.env.demo` ([ADR-0073](0073-settings-files-per-environment.md)), never in the
  repository or logs. With a domain ([#102](https://github.com/heavymedl-labs/tetsu/issues/102)), the provider
  returns to Resend, or stays, in settings alone.
- **Mailpit tests the SMTP adapter.** `scripts/install-tools.sh` installs it pinned; the adapter's tests start it,
  send through it, and check what arrived through its API. It also shows mail on a developer's machine. It is never
  deployed.
- **This lasts until Tetsu serves a real business,** at the gate ADR-0057 sets before the Level 2 review. There,
  invites return to the terminal alone, and an ADR decides whether notices stay.

## Alternatives considered

- **Keep ADR-0057's split.** The port and capture adapter would have no caller in #110, so their tests would prove a
  path no feature takes, the reason ADR-0057 moved the SMTP adapter.
- **A dedicated Gmail account with an app password.** Reaches anyone, but the password signs in to the whole
  account, and Google calls app passwords not recommended.
- **Buy a domain now and keep Resend.** The lasting answer, and about $10.50 a year, but the maintainer holds off on
  domains until a showcase is close.
- **Resend to the maintainer alone.** Meets ADR-0044 unchanged, but cannot reach the other people the maintainer
  names.
- **Notices only, invites by terminal.** Keeps ADR-0040's rule, but nothing an operator does would depend on mail
  arriving.
- **`mail-send`,** Stalwart's SMTP client. Maintained and dual licensed, but used far less (166,426 downloads in 90
  days against 9,417,182), so its code has had less scrutiny; it has no advisories.
- **A hand-written fake SMTP server in the tests.** No tool to adopt, but it would check only what we already
  believe about SMTP, where Mailpit parses each message as a real server does.
- **Mailtrap's hosted sandbox.** A shared inbox, but its free tier holds about 50 messages a month, and tests would
  need its secret.

## Consequences

- The maintainer, and up to four people they name, receive Tetsu's real email while it is in development; the demo's
  fictional people land in the mailbox page.
- The `operators` table gains an email column, which the demo's operators fill when created again. The README and
  the demo runbook give `operator create` its new argument.
- #110's mail criterion grows to the SMTP adapter, invites by email, and both notices. #111 starts with mail built.
- An invite now reaches whoever controls the operator's mailbox, not only whoever ran the command. It is single use,
  lasts an hour, and enrolls an account reaching only fictional data, which is why it ends at the gate.
- `lettre` had six advisories in one day, and two people write nearly all of it. Implicit TLS, plain addresses, and
  the allowlist leave little for those bug classes to reach; cargo-deny reports the next advisory.
- Mailpit is a test tool with one main maintainer. Losing it means a hand-written fake, a change to tests alone.
- **Revisit** when Tetsu has a domain, when Mailgun changes its sandbox terms, or at the Level 2 gate.

## Sources

- [Research: email to a few real people without a domain](../research/email-without-a-domain.md), read 2026-11-18.
- Mailgun, [sandbox domains](https://documentation.mailgun.com/docs/mailgun/user-manual/domains/domains-sandbox),
  read 2026-11-18.
- RustSec, [lettre advisories](https://rustsec.org/packages/lettre.html), including RUSTSEC-2026-0560 to
  RUSTSEC-2026-0565, read 2026-11-18.
- Mailpit, [repository](https://github.com/axllent/mailpit), v1.34.0 of 2026-11-17, read 2026-11-18.
- OWASP ASVS 5.0.0, [V6 Authentication](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x15-V6-Authentication.md):
  6.3.5, 6.3.7, and 6.4.1.
