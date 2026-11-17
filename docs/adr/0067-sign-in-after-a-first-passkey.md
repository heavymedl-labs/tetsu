# 0067. Logging in after a first passkey

- Status: Accepted
- Date: 2026-11-16

## Context

An operator joins from a one-time invite link, then lands on the login page and logs in again. The choice came from
the [operator enrollment review](../research/identity/operator-enrollment-review.md), on the grounds that a
separate login guards the account. A [later pass](../research/identity/sign-in-after-enrollment.md) found that
it does not: whoever holds a link can create a passkey and log in with it. What a passkey login does give is a
session as strong as the passkey, since NIST SP 800-63B-4 holds a session to the authentication that created it.
Staff will arrive by invite links, and customers by signing up with an email code or Google
([ADR-0040](0040-identity-and-sign-in.md)), and neither has a rule yet.

## Decision

- **Operators and staff begin their session with a passkey login, never with the link.** Once their first passkey
  exists, the join page begins that login at once, on the same page, so they confirm on their device twice
  in one flow. A browser that refuses a prompt the person did not start by pressing something gets the login
  button in its place. The link is used up either way; a login that fails leaves them to log in as usual.
- **Customers are logged in once their first passkey exists,** as the FIDO Alliance's signup pattern does. Their
  session rests on the email or Google login their signup proved, and anything that needs more asks for a fresh
  passkey login (`v5.0.0-7.5.1`).
- **Binding a passkey notifies the person** through a channel apart from the binding, as NIST asks, once the realm
  has one: email, for staff and customers. Until operators have email, the audit record is the notice, as now.

## Alternatives considered

- **Log everyone in from their first passkey.** One device confirmation fewer, but an operator's or staff member's
  session would rest on a link, not a passkey.
- **A second page for every realm, as before.** Proves the passkey works, but costs a page and a decision for the
  same session strength the chained login gives, and stalls a customer's booking.
- **No session after signup for customers either.** Safe, but against the FIDO pattern and the booking flow
  ADR-0040 asks for.

## Consequences

- Joining ends on the operator's page, not on the login page, in one flow.
- The staff invite and customer signup flows, when built, follow this rule.
- Browser tests ([#129](https://github.com/heavymedl-labs/tetsu/issues/129)) cover the chained login and its
  fallback.
- The operator enrollment review's reasoning on this point is corrected by the later pass.

**Revisit** if Safari's refusals make the chained login fail often, or if a realm's signup comes to rest on
less than an email.

## Sources

- [Logging in after a first passkey](../research/identity/sign-in-after-enrollment.md), 2026-11-16, and its
  sources: the FIDO Alliance's signup pattern, Google's passkey user journeys, OWASP's Forgot Password cheat
  sheet, and NIST SP 800-63B-4's sessions and authenticator events.
