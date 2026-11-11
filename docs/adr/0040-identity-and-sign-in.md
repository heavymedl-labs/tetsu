# 0040. Identity and sign-in

- Status: Proposed
- Date: 2026-11-11

## Context

Tetsu has three kinds of people who sign in: its own operators, each business's staff, and each business's
customers. [#9](https://github.com/heavymedl-labs/tetsu/issues/9) asks how they sign in and who owns their
accounts, and sign-in ([#20](https://github.com/heavymedl-labs/tetsu/issues/20)) waits on the answer. The
maintainer wants a sign-up like Linear's, with "continue with Google" and "continue with email", and the
simplest experience for customers and the app.

[ADR-0029](0029-security-target-revised.md) requires multi-factor authentication for every account
(`v5.0.0-6.3.3`), and #9 records that customer accounts belong to each business, after Mindbody's shared login
drew a backlash. The [identity research](../research/identity.md) and the
[passkey domains research](../research/passkey-domains.md) evaluate the options as
[ADR-0010](0010-evaluating-technology.md) asks. Their main findings:

- Email codes and links are not an authentication factor under ASVS or NIST SP 800-63B-4, and Google does not
  reliably say when it checked a second factor. A passkey that verifies the user (fingerprint, face, or PIN) is
  the one consumer-friendly method that is multi-factor by itself.
- A passkey works only on the domain it was created for, and nothing moves it to another. `onrender.com` is on
  the Public Suffix List, so a passkey made on the demo host can never reach Tetsu's own domain.
- Linear, Stripe, Square, and GitHub give staff one identity with a membership in each workspace. Shopify keeps
  customer accounts per store.

## Decision

**Tetsu builds its own sign-in, with a passkey as the everyday credential for every account, no passwords, and
three separate realms.**

- **Three realms.**
  - An **operator** is Tetsu's own staff. Operators are created by hand and never sign up.
  - A **staff member** is one identity with a **membership** in each business they work for, carrying their role
    there. Email is unique across staff.
  - A **customer account** belongs to one business. Email is unique within that business, so one person at two
    businesses has two unrelated accounts. A staff identity and a customer account are never the same account.
- **Accounts are apart from client profiles.** A **client profile** is the person a business serves: their
  bookings, waivers, and history. An account signs in and may act for profiles. A walk-in is a profile with no
  account, a guardian's account acts for a minor's profile, and an adult who comes of age gets an account of
  their own that takes over their profile.
- **Every account signs in with a passkey,** which counts as multi-factor when it verifies the user, through the
  W3C WebAuthn standard. Customers create one at sign-up, after an email code; the prompt says to use a
  fingerprint or face next time, and never interrupts a booking. A device that cannot make a passkey uses an
  authenticator app (TOTP) after an email code. Enrollment and drop-off are recorded from the first release, so
  any future relaxation rests on evidence.
- **Email and Google are ways in, never sign-ins.** An email code, or "continue with Google", proves who someone is
  at sign-up and recovery and leads to the passkey step. A Google identity is keyed by its issuer and subject
  (`sub`), never by email, and links to an account only while its owner is signed in.
- **No passwords.** Tetsu stores none. A shared front-desk computer signs a staff member in through their own
  phone, a hardware security key, or the authenticator app.
- **Every passkey belongs to one domain Tetsu owns** ([#102](https://github.com/heavymedl-labs/tetsu/issues/102)).
  Sign-in pages carry each business's name and colors on Tetsu's hosts. A business that later uses its own domain
  gets a one-time code from Tetsu's sign-in, exchanged on the server for a session on that domain, so its
  customers' passkeys survive. A customer account's name includes its business, such as "Lee Park · Anvil Row",
  and each business's sign-in asks only for its own accounts.
  - No real person enrolls a passkey before that domain exists. Demo accounts on the demo host are disposable.
  - Before #20 ships, a test on iOS, Android, and a password manager shows how two accounts with one email
    appear under one domain. If they confuse, the fallback is a Tetsu subdomain per business as its passkey
    domain, which is cheap only before anyone enrolls.
  - Sign-in never happens in a frame, as [ADR-0031](0031-front-end-model-revised.md) already says.
- **Operators sign in on a separate domain, with hardware security keys only,** at least two each. Their sessions
  are short, sensitive actions ask again, every action is logged under the operator's own identity, and an operator
  can never change a business's sign-in email or passkeys. What operators may see or change inside a business, and
  with whose consent, is [#24](https://github.com/heavymedl-labs/tetsu/issues/24)'s decision.
- **On demo environments, the maintainer owns every demo business through seed data,** and signs in like anyone
  else. No code bypasses sign-in in any environment (`v5.0.0-6.3.4`, `v5.0.0-15.2.3`). This is safe because demo
  businesses are fictional ([ADR-0018](0018-demo-businesses-as-data.md)) and no real person's data arrives before
  the Level 2 review (ADR-0029).
- **Recovery.** A customer who loses every passkey proves their email, waits, and gets a notification on every
  channel before enrolling a new passkey. A staff member's owner starts a reset but never sets a credential. Each
  account is offered recovery codes at enrollment. An operator uses their second key.
- **SAML comes later through a broker,** such as WorkOS SSO or a self-hosted Ory Polis, which speaks plain OpenID
  Connect to Tetsu, so Tetsu never parses SAML. Whether Tetsu then trusts a business's own identity provider for
  multi-factor is decided with the first business that asks.
- **The crates** are `webauthn-rs` for passkeys and `openidconnect` for Google, behind a port, each adopted with a
  `Proposed` ADR of its own or in the issue that first uses it, as ADR-0010 asks. Passkeys need a small script,
  which ADR-0031 allows, and the content security policy gains `script-src 'self'` with it.

The work meets these ASVS 5.0.0 requirements at least, and the fresh review checks for any others that apply:
`v5.0.0-6.3.1` (brute force), `v5.0.0-6.3.3` (multi-factor), `v5.0.0-6.3.4` (no undocumented pathways),
`v5.0.0-6.4.3` and `v5.0.0-6.4.4` (recovery), `v5.0.0-6.5.1` to `v5.0.0-6.5.5` (codes and TOTP), `v5.0.0-10.5.1` to
`v5.0.0-10.5.5` (OpenID Connect clients), `v5.0.0-7.2.4` and `v5.0.0-7.5.1` (sessions), `v5.0.0-3.3.3` (`__Host-`
cookies), `v5.0.0-3.5.4` (separate hostnames), `v5.0.0-8.4.1` (cross-tenant controls), and `v5.0.0-13.2.1` (no shared
credentials).

## Alternatives considered

- **One customer account across businesses,** as Mindbody tried in 2025. It contradicts #9's own forces and
  caused sign-in failures there.
- **A separate staff account per business,** as Slack does. Staff at two businesses would juggle two logins.
- **A hosted identity provider for everything,** such as WorkOS, Clerk, or Auth0. They keep one shared user
  pool, which fits per-business customer accounts poorly, and a passkey made on a vendor's domain stays with the
  vendor. WorkOS stays the fallback behind the same port if building proves too much.
- **A self-hosted identity server,** such as Keycloak or Ory Kratos. A second service to run, and its
  multi-tenancy is limited or paid.
- **Trusting Google's multi-factor claim.** Saves a step only when Google sends the claim, which it does not
  promise, and adds a second path to test.
- **Email codes alone for customers,** as a documented relaxation. ADR-0029 rejected relaxing multi-factor for
  customers before passkeys were weighed, and a guardian's account holds a minor's waiver.
- **Passwords with a second factor.** Brings hashing, breach checks, resets, and credential stuffing for no
  gain over passkeys.
- **Sign-in on each business's own domain, or one passkey shared across domains.** The first loses every
  customer's passkeys when a business changes domain. The second, WebAuthn's related origins, is capped at
  five domains in Chrome.

## Consequences

Customers and staff sign in with one tap after their first visit, and nobody keeps a password. Tetsu owns the
most security-critical code, kept by one maintainer, so it is Tier 1 ([ADR-0005](0005-testing-and-performance.md))
and every boundary gets negative tests. ADR-0010's benchmark step is skipped, since sign-in is no hot path and a
passkey check costs microseconds; its step on testing how things fail is the device test above and #20's negative
tests.

Next:

- #20 drops passwords from its criteria, and names the requirements above.
- #102 blocks #20, since every passkey binds to its domain.
- Sending email codes needs an email provider, chosen with a `Proposed` ADR before #20.
- [#23](https://github.com/heavymedl-labs/tetsu/issues/23) builds business sign-up on the staff realm, and #24
  decides operator access to businesses.
- The demo seed format gains the maintainer's owner membership.

**Weakest evidence.** No study measures how many customers of a booking service drop off when a passkey is
required at sign-up, which is why enrollment is measured from the start. How password managers show two
accounts with one email under one domain is untested, which is why the device test comes first. Several vendor
and crate details come from prior knowledge, and each adoption ADR checks them.

**Revisit** when enrollment data shows customers dropping off, when the device test fails, or when a business
needs SAML.

## Sources

- [Identity and sign-in research](../research/identity.md) and its notes, read 2026-11-11.
- [Passkey domains research](../research/passkey-domains.md) and its notes, read 2026-11-11.
- OWASP, "Application Security Verification Standard 5.0.0", chapters V6, V7, V10, and V13,
  <https://github.com/OWASP/ASVS/tree/v5.0.0_release/5.0/en>.
- NIST, "SP 800-63B-4: Authentication and Authenticator Management", July 2025,
  <https://pages.nist.gov/800-63-4/sp800-63b.html>.
- W3C, "Web Authentication: An API for accessing Public Key Credentials, Level 3", 2026,
  <https://www.w3.org/TR/webauthn-3/>.
- FIDO Alliance, "Passkey Index", October 2026, <https://fidoalliance.org/>.
