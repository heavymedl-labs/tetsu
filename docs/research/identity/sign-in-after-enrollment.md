# Signing in after a first passkey

Research on 2026-11-16 for [#110](https://github.com/heavymedl-labs/tetsu/issues/110), step 4d, after the
maintainer's manual run: an operator who enrolls from a one-time link lands on the sign-in page and signs in again,
which felt like a needless step. Should a person who has just created their first passkey be signed in, and does the
answer differ by realm? It feeds [ADR-0067](../../adr/0067-sign-in-after-a-first-passkey.md).

## Summary

The step's stated reason was partly wrong. [Operator enrollment](operator-enrollment-review.md) cited OWASP's
"don't automatically log the user in" and NIST's advice to authenticate with a new authenticator, and concluded
that a separate sign-in guards the account. **It does not guard against a stolen link:** whoever holds the link can
create their own passkey and then sign in with it. The guards against a stolen link are the link's own: single use,
an hour's life, and delivery to the right person, with NIST's notice on binding once email exists.

What a real passkey sign-in does buy is a session as strong as the passkey. NIST says a session is never stronger
than the authentication that created it, so a session started at enrollment rests on the link, and one started by
a passkey sign-in rests on the passkey. That matters for operators and staff, who act on Tetsu's or a business's
data, and much less for customers, whose sign-up rests on their email anyway.

## Findings

| Source | What it says |
| --- | --- |
| FIDO Alliance, [new account creation with a passkey](https://www.passkeycentral.org/design-guidelines/optional-patterns/new-account-creation-with-a-passkey) | After sign-up, "all people, those who opt to create a passkey and those who do not, are authenticated". No second sign-in. |
| Google, [passkey user journeys](https://developers.google.com/identity/passkeys/ux/user-journeys) | Creates the passkey during sign-up and asks for no second sign-in. |
| OWASP, [Forgot Password cheat sheet](https://cheatsheetseries.owasp.org/cheatsheets/Forgot_Password_Cheat_Sheet.html) | "Don't automatically log the user in" after a password reset: auto-login complicates session handling, and a leaked reset link becomes a signed-in account. Written for resets, not sign-up. |
| NIST SP 800-63B-4, [sessions](https://pages.nist.gov/800-63-4/sp800-63b/session) | A session "SHALL NOT be considered at a higher AAL than the authentication event" that created it. |
| NIST SP 800-63B-4, [authenticator events](https://pages.nist.gov/800-63-4/sp800-63b/events) | Binding a new authenticator SHALL notify the subscriber through a channel independent of the binding. Nothing requires a sign-in after it. |
| Tailscale, [passkeys](https://tailscale.com/kb/1269/passkeys) | A single-use invite link creates the passkey and joins in one step, "Create passkey and join". |
| Okta, [passkey end-user experience](https://help.okta.com/oie/en-us/Content/Topics/identity-engine/authenticators/passkeys-end-user-experience.htm) | Enrolls a passkey at a sign-in, prompting a user who has none, rather than from a link. |

The earlier review said Teleport signs a person in after enrollment. This pass found nothing to confirm it, so it is
not relied on.

## By realm

- **Operators and staff** arrive by a link: a one-time enrollment link, or a staff invite
  ([ADR-0040](../../adr/0040-identity-and-sign-in.md)). Their session should come from a passkey sign-in, not the
  link. The page can begin that sign-in itself as soon as the passkey exists, so the person confirms on their
  device twice in one flow and never sees a second page. Safari may refuse a passkey prompt the person did not
  start by pressing something, so the page falls back to its sign-in button.
- **Customers** arrive by signing up: an email code or Google, then a passkey. They are signed in once the passkey
  exists, as FIDO's pattern does; booking never waits. What needs more, such as saved payment, already asks for a
  fresh passkey sign-in (`v5.0.0-7.5.1`).

## Not checked

Each provider's own invite flow, beyond its documentation; how often Safari refuses a prompt that follows another
in practice, which a browser test can measure once [#129](https://github.com/heavymedl-labs/tetsu/issues/129)
builds them.
