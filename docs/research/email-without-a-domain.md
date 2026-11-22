# Email to a few real people without a domain

Research on 2026-11-18 for [#110](https://github.com/heavymedl-labs/tetsu/issues/110). The maintainer chose to let
operators receive email while Tetsu is in development (invites and security notices), and to test it end to end
against real inboxes: their own, and a few people they name. [ADR-0044](../adr/0044-email-delivery.md)'s provider,
Resend, delivers only to its account owner until a domain is verified, and Tetsu has no domain until a client
showcase is close ([#102](https://github.com/heavymedl-labs/tetsu/issues/102)). This note asks what reaches a
handful of chosen addresses with no domain, at no cost, over SMTP so that changing provider stays a change of
settings, as ADR-0044 requires. Facts and prices are as read on 2026-11-18; where only third-party pages say
something, the table says so.

## Summary

Two options reach other people without a domain and keep SMTP: **Mailgun's sandbox**, which delivers to up to five
recipients who each accept an invitation, and **Gmail over SMTP with an app password**, which reaches anyone but
needs a dedicated account, since the password opens that account. Every single-sender provider fails for a
gmail.com sender, since no one can authenticate a domain they do not own. A domain at cost, about $10.50 a year,
removes every limit and is what ADR-0044 already plans for.

## Options

| Option | Reaches | Cost | SMTP | What it takes | Evidence |
| --- | --- | --- | --- | --- | --- |
| Resend, as ADR-0044 decided | The account owner only, until a domain is verified; others get a 403 | Free, 3,000 a month | Yes | Nothing new | [Resend](https://resend.com/docs/knowledge-base/403-error-resend-dev-domain), official |
| Mailgun sandbox | Up to 5 authorized recipients, each accepting an invite email | Free with no card; third-party pages say 100 a day | Yes, `smtp.mailgun.org` | A Mailgun account; mail comes from its `mailgun.org` sandbox subdomain | [Mailgun](https://documentation.mailgun.com/docs/mailgun/user-manual/domains/domains-sandbox), official; [Mailtrap](https://mailtrap.io/blog/mailgun-sandbox-tutorial/) |
| Gmail with an app password | Anyone | Free, 500 a day | Yes, `smtp.gmail.com` | A dedicated Google account with 2-Step Verification; Google calls app passwords "not recommended" and revokes them on a password change | [Google](https://support.google.com/accounts/answer/185833), official; limits from [third parties](https://www.emailvendorselection.com/gmail-smtp-server/) |
| Gmail API, `gmail.send` scope | Anyone | Free, 500 a day | No, an HTTP API | Send-only, but a testing app's refresh token lasts 7 days, and production needs Google's verification | [Courier](https://www.courier.com/docs/integrations/email/gmail), third party |
| Amazon SES sandbox | Verified addresses, 200 a day | No standing free tier for new accounts; credits for 6 months | Yes | An AWS account with a card | [Third-party pricing pages](https://sendops.dev/amazon-ses-pricing/), conflicting |
| Brevo, Mailjet, Postmark | A gmail.com sender cannot be authenticated, so mail is rewritten or lands in spam | Free tiers | Yes | Your own domain in practice | [Brevo](https://help.brevo.com/hc/en-us/articles/14925263522578-Comply-with-Gmail-Yahoo-and-Microsoft-s-requirements-for-email-senders), official |
| SendGrid | | Free plan ended in 2025; a 60-day trial | Yes | | [Third parties](https://dev.to/cendyandreoli/sendgrid-ended-its-free-plan-free-email-api-alternatives-with-real-limits-2026-1be) |
| Outlook.com | | | Basic authentication ended for personal accounts in 2024 | OAuth only | [Third parties](https://www.getmailbird.com/microsoft-modern-authentication-enforcement-email-guide/), conflicting dates |
| A domain at cost | Anyone, with SPF, DKIM, and DMARC | About $10.50 a year at Cloudflare's registrar | Yes, through Resend | Buying a domain before the showcase | [Third-party price trackers](https://domaindetails.com/registrars/cheapest) |

## Findings

- **Mailgun's sandbox fits the request exactly** while it stays small: five people, each opting in by email, with no
  card and no secret beyond sending credentials. Mailgun asks for a card or a verified domain to send to anyone else.
  Sandbox mail comes from a shared `mailgun.org` subdomain, so it may land in spam, which matters little for testing.
- **Gmail reaches anyone** and is signed by Google, so it delivers well. Its cost is the credential: an app password
  signs in to the whole account, not only to send, so it belongs to an account made for this and nothing else.
- **No single-sender service helps.** A gmail.com address cannot pass DMARC through someone else's servers, so
  Brevo rewrites the sender and warns of rejection, and the others share the problem.
- **The domain is the only lasting fix,** and ADR-0044 already says the provider or domain changes in settings,
  with no code change, once [#102](https://github.com/heavymedl-labs/tetsu/issues/102) arrives.

**Note, 2026-11-21.** Mailgun's signup now asks for a card before any sending, so its sandbox no longer fits as
read above. [ADR-0079](../adr/0079-no-mail-provider-before-a-domain.md) sets no provider until Tetsu has a domain.

## What teams use

Teams split email by stage, and ADR-0044 already follows the split; only the step before a domain is unusual.

| Stage | Common practice | Tetsu |
| --- | --- | --- |
| Local and tests | A mail catcher such as Mailpit, which shows sent mail in a web page and delivers none | The capture adapter, in Postgres |
| Staging or demo | Real mail to an allowlist of the team's addresses, the rest redirected or caught, with staging's own credentials and often its own sending domain | ADR-0044's setting of addresses that go out by SMTP, the rest captured |
| Production | A transactional service on the company's domain, often a mail subdomain kept apart from marketing mail: Postmark, Amazon SES, Resend, SendGrid, or Mailgun | Resend once Tetsu has a domain |

Most teams buy a domain early, so they never need the no-domain options above. No source gave market share, only
comparison posts, so the providers are listed as common, not ranked. Sources:
[Mailpit](https://makerstack.co/reviews/mailpit-review/), [Mailtrap and Mailpit](https://emailfordevelopers.com/compare/mailtrap-vs-mailpit/),
[staging interceptors](https://makandracards.com/makandra/46094-rails-write-custom-email-interceptors),
[staging filtering](https://renderedtext.com/blog/2012/04/27/filtering-emails-on-staging/),
[transactional services](https://www.suprsend.com/post/best-transactional-email-services), and
[subdomains](https://www.braze.com/resources/articles/email-subdomains).

## Libraries and tools

Project health as [ADR-0010](../adr/0010-evaluating-technology.md) step 4 asks, read on 2026-11-18.

| Project | License | Latest release | Use, last 90 days | Maintainers | Advisories |
| --- | --- | --- | --- | --- | --- |
| [`lettre`](https://github.com/lettre/lettre), SMTP client | MIT | 0.11.24, 2026-11-17; five releases in 2026 | 9,417,182 downloads | Two write nearly all of it (759 and 360 commits) | Six published 2026-11-17, fixed in 0.11.24: STARTTLS reply injection, command injection through bare line endings, quoted local parts sent elsewhere, and three crashes or memory exhaustion from a hostile server; one in May 2026 for its Boring TLS backend |
| [`mail-send`](https://github.com/stalwartlabs/mail-send), SMTP client | Apache-2.0 or MIT | 0.7.0, 2026-11-01 | 166,426 downloads | Stalwart Labs | None listed |
| [Mailpit](https://github.com/axllent/mailpit), mail catcher | MIT | v1.34.0, 2026-11-17 | About 11,000 GitHub stars | One writes nearly all of it (2,069 commits) | None found |

`lettre` needs Rust 1.85 and supports tokio and rustls, which Tetsu uses. Its advisories came in one batch, which
reads as an audit answered within a day, but they show the SMTP protocol's edges are where its bugs live. Sources:
crates.io, GitHub, and the [RustSec advisory database](https://rustsec.org/packages/lettre.html).

## Gaps

- Mailgun's free daily limit comes from third-party pages; its own pricing page was not read.
- Whether Google will keep app passwords for personal accounts is unclear; its help page discourages them but does
  not set an end date.
- SES's free terms changed in 2025 and 2026, and the pages read disagree.
