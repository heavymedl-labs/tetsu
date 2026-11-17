# 0069. Logged in on joining

- Status: Accepted
- Date: 2026-11-17
- Supersedes the operator and staff rule in [ADR-0067](0067-sign-in-after-a-first-passkey.md)

## Context

[ADR-0067](0067-sign-in-after-a-first-passkey.md) began an operator's session only from a passkey login, which the
join page started at once after the first passkey existed, so the operator confirmed on their device twice. Its own
[research](../research/identity/sign-in-after-enrollment.md) found the second login does not guard against a stolen
link: whoever holds the link creates their own passkey and logs in with it. What it buys is a session as strong as
the passkey, since NIST SP 800-63B-4 holds a session to the authentication that created it. In the maintainer's
manual run on 2026-11-17, with the passkey on a phone, the second prompt was another QR code, which read as the first
one failing to close.

## Decision

- **Joining logs an operator in,** in the transaction that stores their first passkey and uses up the link, ending
  any session the browser presented, as a login does. The join page goes straight to their page.
- **That session is never fresh.** A session records when a passkey last proved the person in it (`proven_at`): at
  the start of a login, and never for a session joining began, which rests on the link. Whatever ADR-0048 asks a
  fresh login for, such as adding a passkey, waits for a passkey login. The session's limits are the same as any
  other's.
- **Staff joining by invite follow the same rule** when their realm is built. Customers are unchanged: ADR-0067
  already logs them in once their first passkey exists.

## Alternatives considered

- **Keep the chained login (ADR-0067).** A session as strong as the passkey from the first page, at the cost of a
  second device confirmation, which on a phone is a second QR code.
- **Chain only when the passkey is on this device.** One tap on a laptop's own passkey manager, and the button on a
  phone. Two flows to explain and test for a guard the research found protects nothing a stolen link cannot get.
- **Land on the login page after joining,** as before ADR-0067. The same second confirmation, on another page.

## Consequences

- Joining is one device confirmation. A stolen link still gets a session, as it always could through a login.
- A session joining began cannot add a passkey or do anything else that needs a fresh login, so the operator's
  page tells them to log out and log in with their passkey first.
- The chained login and its fallbacks leave the passkey island, and with them its hardest cases.
- Changing the first identity migration is folded in as the [runbook](../operations/demo-environment.md) allows
  before the first real business, so the demo deploys with its identity schema reset, and local databases run
  `tetsu reset identity`.

## Sources

- [NIST SP 800-63B-4, sessions](https://pages.nist.gov/800-63-4/sp800-63b/session/): a session "SHALL NOT be
  considered at a higher AAL than the authentication event".
- [Logging in after a first passkey](../research/identity/sign-in-after-enrollment.md), 2026-11-16.
- [Operator lifecycles and their extensions](../research/identity/operator-lifecycles.md), 2026-11-17.
