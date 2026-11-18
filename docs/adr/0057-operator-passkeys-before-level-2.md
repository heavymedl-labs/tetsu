# 0057. Operator passkeys and email before the Level 2 review

- Status: Accepted
- Date: 2026-11-14
- Supersedes, until the Level 2 review, [ADR-0040](0040-identity-and-sign-in.md)'s rule that operators log in with
  attested hardware keys, at least two each, and [ADR-0041](0041-account-recovery.md)'s rule for an operator who
  loses both
- Moves [ADR-0044](0044-email-delivery.md)'s SMTP adapter, and the choice of its library, from
  [#110](https://github.com/heavymedl-labs/tetsu/issues/110) to the first slice that emails a real person
- That move reversed by [ADR-0074](0074-operator-email-in-development.md), which builds the SMTP adapter in #110

## Context

ADR-0040 has operators log in with hardware keys whose attestation is checked against the key maker's pinned
roots, at least two keys each, because the operator realm reaches every business. The maintainer, the only
operator, has no hardware keys, and does not want to require them while Tetsu holds only fictional data.

Until the Level 2 review, no real person's data reaches Tetsu, every demo business is fictional, and every demo
passkey belongs to `onrender.com` hosts and is thrown away when Tetsu's domains arrive (ADR-0040,
[ADR-0042](0042-tenant-isolation.md)). A passkey that verifies the user is multi-factor by itself; attestation adds
proof of which device holds it, and rules out synced passkeys.

ADR-0044 places the SMTP adapter, and the choice of its library, in #110. Slice 5 of
[ADR-0048](0048-sign-in-design.md) sends no email to a real person: operators are invited by command, and the capture
mailbox shows what demo accounts would receive.

## Decision

- **Until the Level 2 review, an operator logs in with any passkey that verifies the user,** synced passkeys
  included, with no attestation check. Every other operator rule stands: the operator realm's own host and relying
  party ID, invites only through ADR-0040's command printed to its terminal, sessions of 8 hours and 15 minutes
  idle (ADR-0048), rate limits, and an audit entry for every login.
- **Each operator enrolls two passkeys,** on different devices or passkey managers, so losing one does not mean
  joining again. The invite sets up the first; the second is added after a fresh login, as ADR-0040 requires
  of any added passkey.
- **An operator who loses both** gets a new account from ADR-0040's command, and the old one is removed with its
  sessions.
- **Attested hardware keys return as a gate before the Level 2 review,** together with Tetsu's domains
  ([#102](https://github.com/heavymedl-labs/tetsu/issues/102)). Every operator joins again there in any case,
  since a passkey cannot move from `onrender.com` to Tetsu's domain, and ADR-0040's and ADR-0041's operator rules
  apply again in full. No real business's data is ever reachable with a synced operator passkey.
- **#110 builds ADR-0044's `Mailer` port, its capture adapter, and the operator page that reads it.** The SMTP
  adapter, and the choice of its library through [ADR-0010](0010-evaluating-technology.md), come with the first
  slice that emails a real person: staff invites in [#111](https://github.com/heavymedl-labs/tetsu/issues/111),
  or earlier if another slice needs it.

## Alternatives considered

- **Buy two hardware keys now.** Keeps ADR-0040 whole, but costs money and setup for an operator realm that reaches
  only fictional data until the gate.
- **One passkey per operator.** Simpler, but a lost phone would mean running the command again for the only
  operator.
- **Build the SMTP adapter in #110 as ADR-0044 placed it.** Nothing in the slice would use it, so its tests would
  prove a path no feature takes yet.

## Consequences

- #110's criterion on hardware keys becomes a passkey that verifies the user, two per operator.
- A synced passkey's security rests on the account it syncs through, such as the maintainer's Apple or Google
  account. Until the gate, taking that account could take the operator realm, which reaches only fictional data.
- Before the Level 2 review: attested hardware keys for every operator, at least two each, with the key maker's
  roots pinned, as ADR-0040 sets.
- ADR-0044's Resend setup waits for #111.

## Sources

- [ADR-0040](0040-identity-and-sign-in.md), [ADR-0041](0041-account-recovery.md), and
  [ADR-0044](0044-email-delivery.md), the rules this revises.
- NIST, "SP 800-63B-4: Authentication and Authenticator Management", July 2025, on syncable authenticators,
  <https://pages.nist.gov/800-63-4/sp800-63b.html>.
- W3C, "Web Authentication: An API for accessing Public Key Credentials, Level 3", 2026, on attestation,
  <https://www.w3.org/TR/webauthn-3/>.
