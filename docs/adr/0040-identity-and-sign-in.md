# 0040. Identity and sign-in

- Status: Accepted
- Date: 2026-11-11
- Supersedes the rule in [ADR-0029](0029-security-target-revised.md) that #9's ADR decides recovery and records
  any relaxation of `v5.0.0-6.3.3`; both move to #105's ADR

## Context

Tetsu has three kinds of people who sign in: its own operators, each business's staff, and each business's
customers. [#9](https://github.com/heavymedl-labs/tetsu/issues/9) asks how they sign in, who owns their accounts,
and how sessions carry the tenant, and sign-in ([#20](https://github.com/heavymedl-labs/tetsu/issues/20)) waits on
the answer. The maintainer wants a sign-up like Linear's, with "continue with Google" and "continue with email",
and the simplest experience for customers and the app.

[ADR-0029](0029-security-target-revised.md) requires multi-factor authentication for every account
(`v5.0.0-6.3.3`), and #9 records that customer accounts belong to each business, after Mindbody's shared login
drew a backlash. The [identity research](../research/identity.md) and the
[passkey domains research](../research/passkey-domains.md) evaluate the options as
[ADR-0010](0010-evaluating-technology.md) asks. Their main findings:

- Email codes and links are not an authentication factor under ASVS or NIST SP 800-63B-4, and Google does not
  reliably say when it checked a second factor. A passkey that verifies the user (fingerprint, face, or PIN) is
  the one consumer-friendly method that is multi-factor by itself.
- A passkey belongs to one relying party ID, a domain, and every host under that domain may ask for it. A passkey
  cannot move to a wider or different domain, and `onrender.com` is on the Public Suffix List.
- Linear, Stripe, Square, and GitHub give staff one identity with a role in each workspace. Shopify keeps
  customer accounts per store.

Three doubt-driven review cycles shaped this ADR. It decides how people sign in and how a session carries its
tenant. Recovery, hosts, and operator access are decided in the issues named under Consequences.

## Decision

**Tetsu builds its own sign-in, with a passkey as the only credential for every account, no passwords, and three
separate realms.**

### Realms and accounts

- An **operator** works for Tetsu itself and signs in to Tetsu's own tools. Whether operators may see inside a
  business, and with whose consent, is [#24](https://github.com/heavymedl-labs/tetsu/issues/24)'s decision.
- A **staff member** is one identity with a **staff role** at each business they work for. Email is unique across
  staff, and only the staff member changes it. The roles themselves are #20's.
- A **customer account** belongs to one business. Email is unique within that business, so one person at two
  businesses has two unrelated accounts. A staff member's identity and a customer account are never the same
  account.
- A **client profile** is the person a business serves: their bookings, waivers, and history. Sign-up creates the
  customer's own profile and links it. An account acts for any other profile, such as a guardian for a minor, only
  when the business's staff grant it, and staff remove it the same way (`v5.0.0-8.2.2`). No account claims a
  profile by its email; staff merge a walk-in's profile into an account's.
- A **guest** books without an account and gets a new client profile. A guest's emailed link can view or cancel
  that one booking and nothing else. Waivers, minors, saved payment, and history need a customer account.

### Credentials

- **Every account signs in with a passkey** under the W3C WebAuthn standard. Tetsu requires user verification
  and rejects any response without it. There are no passwords and no authenticator apps.
- **Each account has its own random WebAuthn user ID,** and enrollment lists the account's existing passkeys so
  none is overwritten. Lee Park's passkeys at two businesses are then two separate entries.
- **A customer account exists only once its passkey does.** Sign-up proves the email with a code, or with
  "continue with Google", then enrolls the passkey; until then nothing is stored but the pending attempt, and an
  abandoned sign-up leaves nothing behind. Booking never waits on it, since a guest can book. Enrollment and
  drop-off are recorded from the first real customer.
- **An email or Google sign-in that matches an existing account never enrolls a passkey.** It leads to sign-in,
  or to recovery as [#105](https://github.com/heavymedl-labs/tetsu/issues/105) defines. A Google identity is keyed
  by its issuer and subject (`sub`), never by email (`v5.0.0-6.8.1`). Google proves an email only when it says
  the email is verified and Google hosts it, as a Gmail or Google Workspace address; any other email takes a code.
  A Google identity used at sign-up is linked when the account is created with its passkey; linking one to an
  existing account needs a fresh passkey sign-in (`v5.0.0-7.5.1`). After sign-up, a linked Google identity does
  only what #105 allows.
- **Staff join by invite.** An invite to an email that already belongs to a staff member means "sign in and
  accept"; otherwise its single-use, short-lived link enrolls a new identity's passkey (`v5.0.0-6.4.1`).
- **Adding or removing a passkey, or changing an email,** needs a fresh passkey sign-in (`v5.0.0-7.5.1`) and sends
  a notice to the account's email.
- **A device that cannot make a passkey** signs in through the person's phone (WebAuthn's cross-device flow, which
  needs Bluetooth on the computer) or a hardware security key. At a shared front desk, staff use their own phone or
  key, never a passkey saved under a shared computer login, so no two staff members share a factor.
- **Email codes** come from a secure random source, carry at least 20 bits, and are single use, short-lived, and
  bound to the attempt that asked for them (`v5.0.0-6.5.1`, `v5.0.0-6.5.3` to `v5.0.0-6.5.5`). Their rate limits,
  documented in #20, key on the attempt and the requester, not only the account, so they cannot lock out the
  account's owner (`v5.0.0-6.1.1`, `v5.0.0-6.3.1`).

### Passkey domains

Each realm's passkeys belong to its own relying party ID, and each realm accepts a passkey only when its signed
origin exactly matches one of that realm's hosts.

- **Customers** use Tetsu's main domain ([#102](https://github.com/heavymedl-labs/tetsu/issues/102)). Sign-in
  pages carry each business's name and colors, and a customer account's name includes its business, such as
  "Lee Park · Anvil Row". A browser may offer every Tetsu customer passkey a person holds, and a business's sign-in
  accepts only its own accounts.
- **Staff** use their own host, as a relying party ID of that host alone, so no customer page can ask for a staff
  passkey (`v5.0.0-3.5.4`).
- **Operators** use a separate domain, with hardware keys whose attestation is verified against the pinned roots
  of the chosen key maker, at least two keys each.
- **The demo** mirrors this layout under a domain of its own, never under Tetsu's main domain, with its operator
  host under that same domain, since it holds no real data. Its passkeys can never answer a real host ([ADR-0018](0018-demo-businesses-as-data.md)). Sign-ups and guest booking stay off
  there, and everywhere, until the Level 2 review (ADR-0029).
- No real person enrolls a passkey before Tetsu's domain exists. #102 provides three: the main domain, the
  operator domain, and the demo domain.
- [#8](https://github.com/heavymedl-labs/tetsu/issues/8) decides which hosts these are. When a business uses its
  own domain, its sign-in is decided by its own ADR, through a standard protocol with exact-match return addresses
  (`v5.0.0-3.7.2`), never one of our own.
- Before #20 ships, a test enrolls two customer accounts with one email, and one person as both staff and
  customer, on iOS, Android, macOS with Safari, Windows with Chrome, Linux with Firefox, a shared Windows desk,
  the cross-device flow, a security key, 1Password, Bitwarden, iCloud Keychain, and Google Password Manager. It
  passes if every passkey still signs in after the others are enrolled and each shows its business. If it fails,
  a revision of this ADR weighs a customer passkey domain per business, which fixes each business's address.
- Sign-in never happens in a frame, as [ADR-0031](0031-front-end-model-revised.md) already says.

### Sessions

- **Each session is a server-side record of its realm, its identity, and its business:** a customer account's
  business, or the staff role in use. Every request checks it and fails closed (`v5.0.0-8.4.1`).
- **A new session token is issued** on every sign-in and when a staff member switches business (`v5.0.0-7.2.4`).
  Removing an account or a staff role ends the sessions it covers (`v5.0.0-7.4.2`).
- #20 sets the timeouts, with the justification NIST asks for (`v5.0.0-7.1.1`), and how one host keeps several
  businesses' customer sessions. Tokens for the API and native apps are
  [#12](https://github.com/heavymedl-labs/tetsu/issues/12)'s and [#14](https://github.com/heavymedl-labs/tetsu/issues/14)'s.

### Operators and the demo

- **A documented command creates an operator account,** and the maintainer runs it. It issues a single-use
  enrollment link only for an account with no credential, so it can never rebind an existing one
  (`v5.0.0-6.4.1`). It prints the link only to the terminal that ran it, never to CI logs or a seed file, and no
  account exists without a person enrolling it (`v5.0.0-6.3.2`).
- **On the demo, the maintainer owns every demo business through seed data,** created by the same "create a
  business with its owner" use case that business sign-up
  ([#23](https://github.com/heavymedl-labs/tetsu/issues/23)) needs, and enrolls through the same command. They
  sign in like anyone else, and no code bypasses sign-in in any environment (`v5.0.0-6.3.4`, `v5.0.0-15.2.3`).
  Tests sign in with a virtual authenticator and an email sink, in test code only. This is safe because demo
  businesses are fictional, and no real person's data arrives before the Level 2 review.

### Building it

- **Sign-in sits behind a port** ([ADR-0003](0003-architecture.md)). Replacing it for a realm keeps that realm's
  passkeys only if the replacement serves the same hosts.
- **Each crate,** such as `webauthn-rs` for passkeys and `openidconnect` for Google, needs a `Proposed` ADR before
  it is adopted, as ADR-0010 asks. Google sign-in comes through standard OpenID Connect behind the port, so no
  provider's own service enters the binary.
- **Passkeys need a small script,** which ADR-0031 allows. CONSTRAINTS.md's exception E5 must be closed first,
  with TypeScript tests, coverage, and boundaries in force, so the script meets the Tier 1 bar. The content security policy gains `script-src 'self'` and
  `connect-src 'self'`, and `form-action` allows Tetsu and Google's sign-in (`v5.0.0-3.4.3`).
- **SAML comes later through a broker,** such as WorkOS SSO or a self-hosted Ory Polis, which speaks OpenID Connect
  to Tetsu, so Tetsu never parses SAML. A business's identity provider is linked to a staff member by its issuer
  and subject after a passkey sign-in, admits them only to that business, and does not replace their passkey until
  an ADR decides whether to trust that provider's multi-factor (`v5.0.0-6.8.4`).

The work meets at least `v5.0.0-6.1.1`, `v5.0.0-6.1.3`, `v5.0.0-6.3.1`, `v5.0.0-6.3.3`, `v5.0.0-6.3.4`,
`v5.0.0-6.3.2`, `v5.0.0-6.4.1`, `v5.0.0-6.5.1`, `v5.0.0-6.5.3` to `v5.0.0-6.5.5`, `v5.0.0-6.8.1`, `v5.0.0-6.8.2`, `v5.0.0-7.2.4`, `v5.0.0-7.4.2`, `v5.0.0-7.5.1`, `v5.0.0-3.4.3`, `v5.0.0-3.7.2`,
`v5.0.0-3.5.4`, `v5.0.0-8.2.2`, `v5.0.0-8.4.1`, `v5.0.0-15.2.3`, and `v5.0.0-16.3.1` (every authentication attempt
logged, in every realm). #20 names every requirement that applies in chapters V2, V3, V6, V7, V8, V9, V10, V11,
V12, V13, V14, V15, and V16, and the fresh review checks for any it misses.

## Alternatives considered

- **One customer account across businesses,** as Mindbody tried in 2025. It contradicts #9's own forces and
  caused sign-in failures there.
- **A separate staff account per business,** as Slack does. Staff at two businesses would juggle two logins.
- **A hosted identity provider for everything,** such as WorkOS, Clerk, or Auth0. They keep one shared user pool,
  which fits per-business customer accounts poorly, and a passkey made on a vendor's domain stays with the vendor
  unless it runs on Tetsu's hosts.
- **A self-hosted identity server,** such as Keycloak or Ory Kratos: a second service to run, back up, and watch,
  for one maintainer.
- **An authenticator app as a fallback.** With email no factor, it is one factor, and anyone could choose it.
- **Trusting Google's multi-factor claim.** Saves a step only when Google sends the claim, which it does not
  promise, and adds a second path to test.
- **Email codes alone for customers.** ADR-0029 left this relaxation to #9, and a guardian's account holds a
  minor's waiver, so customers get the same passkey as everyone.
- **Passwords with a second factor.** Brings hashing, breach checks, resets, and credential stuffing for no gain
  over passkeys.
- **Staff and customers sharing one passkey domain.** Simpler, but then any host under it could ask for a staff
  passkey.
- **Sign-in on each business's own domain, or one passkey shared across domains.** The first loses every
  customer's passkeys when a business changes domain. The second, WebAuthn's related origins, is capped at five
  registrable domain labels in Chrome.

## Consequences

Customers and staff sign in with one tap, nobody keeps a password, and someone without a passkey can still book
as a guest. Tetsu owns the most security-critical code, kept by one maintainer, so it meets
[ADR-0005](0005-testing-and-performance.md)'s full Tier 1 bar, including a benchmark of the session check, which
runs on every request.

Before #20, alongside #102:

- **Account recovery,** in #105, whose ADR also records any relaxation of `v5.0.0-6.3.3` it needs. This
  supersedes ADR-0029's rule that #9's ADR decides both, so the relaxation sits where the recovery it serves is
  decided. `v5.0.0-6.4.3` covers resetting a password, which Tetsu has none of; for a lost
  passkey the rules are `v5.0.0-6.4.4` and `v5.0.0-6.3.3`, whose relaxation ADR-0029 allows with a documented
  rationale and controls. That leaves room for the maintainer's forgiving path for customers.
- **An email provider** for codes, with a `Proposed` ADR.

Also decided elsewhere: hosts in #8, operator access to businesses in #24, and the API and app tokens in #12
and #14. The demo seed format gains the maintainer's owner role in a new format version
([ADR-0019](0019-seed-format-versions.md)), applied to existing demo businesses by a demo reset or an update path
that #20 chooses.

**Trade-offs.** A person whose passkeys sync through Google relies on that Google account, which Tetsu cannot
control. The cross-device flow needs Bluetooth, so a front desk without it needs security keys. A guest who loses
a booking link asks the business. The maintainer's own database access sits outside these rules, under ADR-0029
and [#27](https://github.com/heavymedl-labs/tetsu/issues/27).

**Weakest evidence.** No study measures how many customers of a booking service drop off when a passkey is
required at sign-up, which is why enrollment is measured and guests can book. How passkey managers handle several
Tetsu accounts is untested, which is why the device test comes first.

**Revisit** when enrollment data shows customers dropping off, when the device test fails, or when a business
needs SAML.

## Sources

- [Identity and sign-in research](../research/identity.md) and its notes, read 2026-11-11.
- [Passkey domains research](../research/passkey-domains.md) and its notes, read 2026-11-11.
- OWASP, "Application Security Verification Standard 5.0.0", chapters V3, V6, V7, V8, V15, and V16,
  <https://github.com/OWASP/ASVS/tree/v5.0.0_release/5.0/en>.
- NIST, "SP 800-63B-4: Authentication and Authenticator Management", July 2025,
  <https://pages.nist.gov/800-63-4/sp800-63b.html>.
- W3C, "Web Authentication: An API for accessing Public Key Credentials, Level 3", 2026,
  <https://www.w3.org/TR/webauthn-3/>.
- FIDO Alliance, "Passkey Index 2026", <https://fidoalliance.org/passkey-index-2026/>.
