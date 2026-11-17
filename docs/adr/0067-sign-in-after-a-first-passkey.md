# 0067. Signing in after a first passkey

- Status: Accepted
- Date: 2026-11-16

## Context

An operator enrolls from a one-time link, then lands on the sign-in page and signs in again. The choice came from
the [operator enrollment review](../research/identity/operator-enrollment-review.md), on the grounds that a
separate sign-in guards the account. A [later pass](../research/identity/sign-in-after-enrollment.md) found that
it does not: whoever holds a link can create a passkey and sign in with it. What a passkey sign-in does give is a
session as strong as the passkey, since NIST SP 800-63B-4 holds a session to the authentication that created it.
Staff will arrive by invite links, and customers by signing up with an email code or Google
([ADR-0040](0040-identity-and-sign-in.md)), and neither has a rule yet.

## Decision

- **Operators and staff begin their session with a passkey sign-in, never with the link.** Once their first passkey
  exists, the enrollment page begins that sign-in at once, on the same page, so they confirm on their device twice
  in one flow. A browser that refuses a prompt the person did not start by pressing something gets the sign-in
  button in its place. The link is used up either way; a sign-in that fails leaves them to sign in as usual.
- **Customers are signed in once their first passkey exists,** as the FIDO Alliance's sign-up pattern does. Their
  session rests on the email or Google sign-in their sign-up proved, and anything that needs more asks for a fresh
  passkey sign-in (`v5.0.0-7.5.1`).
- **Binding a passkey notifies the person** through a channel apart from the binding, as NIST asks, once the realm
  has one: email, for staff and customers. Until operators have email, the audit record is the notice, as now.

## Alternatives considered

- **Sign everyone in from the enrollment.** One device confirmation fewer, but an operator's or staff member's
  session would rest on a link, not a passkey.
- **A second page for every realm, as before.** Proves the passkey works, but costs a page and a decision for the
  same session strength the chained sign-in gives, and stalls a customer's booking.
- **No session after enrollment for customers either.** Safe, but against the FIDO pattern and the booking flow
  ADR-0040 asks for.

## Consequences

- An operator's enrollment ends on their page, not on the sign-in page, in one flow.
- The staff invite and customer sign-up flows, when built, follow this rule.
- Browser tests ([#129](https://github.com/heavymedl-labs/tetsu/issues/129)) cover the chained sign-in and its
  fallback.
- The operator enrollment review's reasoning on this point is corrected by the later pass.

**Revisit** if Safari's refusals make the chained sign-in fail often, or if a realm's sign-up comes to rest on
less than an email.

## Sources

- [Signing in after a first passkey](../research/identity/sign-in-after-enrollment.md), 2026-11-16, and its
  sources: the FIDO Alliance's sign-up pattern, Google's passkey user journeys, OWASP's Forgot Password cheat
  sheet, and NIST SP 800-63B-4's sessions and authenticator events.
