# Identity for Tetsu: build vs self-host vs hosted

Pages read on 2026-11-11 unless noted. Claims marked "(prior knowledge, not re-read)" come from the researcher's training data. Verify them before they decide anything.

## Hosted providers: pricing, features, fit with server-rendered pages and no JS

### Takeaway

At Tetsu's scale, WorkOS AuthKit costs nothing per user up to 1M MAU. Each SAML connection costs $125/mo, which a B2B customer's plan can pay for. That makes it the cheapest hosted choice with the least lock-in. Its hosted, redirect-based sign-in also fits server-rendered pages with no JS. Clerk and Stytch are cheap per user but built around JS components. Auth0's B2B plans cost hundreds to thousands of dollars a month from the start.

### Cited Findings

**WorkOS** ([pricing](https://workos.com/pricing), read 2026-11-11)

- AuthKit user management: first 1M MAU $0, then $2,500/mo per extra 1M MAU. MFA and passkeys are included at no extra price. — [WorkOS pricing](https://workos.com/pricing)
- SSO per connection: $125 each for 1–15, $100 for 16–30, $80 for 31–50, $65 for 51–100, custom at 101+. Directory Sync (SCIM) uses the same tiers. — [WorkOS pricing](https://workos.com/pricing)
- Custom domain $99/mo. Audit-log streaming $125/mo per SIEM connection. Retention $99/mo per 1M events. Radar bot protection: 1,000 checks free, then $100 per 50K. Staging environments are free. — [WorkOS pricing](https://workos.com/pricing)
- Cost estimate: $0 at 1k, 10k and 100k MAU, plus $99 for a custom domain, plus $125 per SAML customer.

**Clerk** ([pricing](https://clerk.com/pricing), read 2026-11-11)

- Hobby is $0 for 50,000 MRU ("monthly retained users") per app. **MFA and passkeys are not on Hobby.** — [Clerk pricing](https://clerk.com/pricing)
- Pro is $25/mo ($20/mo billed annually) with 50,000 MRU included. Overage is $0.02 per MRU from 50k to 100k, $0.018 to 1M, then $0.015 and $0.012. — [Clerk pricing](https://clerk.com/pricing)
- Business is $300/mo and adds a SOC 2 report and 10 seats. — [Clerk pricing](https://clerk.com/pricing)
- B2B add-on: 100 monthly retained orgs and 20 members per org free. "Enhanced" is $100/mo, then about $1 per extra org. — [Clerk pricing](https://clerk.com/pricing)
- Enterprise SSO (SAML/OIDC): 1 connection included on Pro. Extra connections cost $75 each for 2–15 and $60 for 16–100. Directory Sync uses the same tiers, and its billing starts 2027-01-01. — [Clerk pricing](https://clerk.com/pricing)
- Cost estimate, since MFA forces Pro: about $25/mo at 1k and at 10k. At 100k it is about $25 + 50,000 × $0.02 = $1,025/mo, before B2B and SSO add-ons.

**Auth0 (Okta Customer Identity)** ([pricing](https://auth0.com/pricing), read 2026-11-11)

- Free covers up to 25,000 MAU with passkeys, 5 orgs and 1 enterprise SSO connection. Pro and Enterprise MFA are not on Free. — [Auth0 pricing](https://auth0.com/pricing)
- Monthly prices at 1k / 10k MAU: B2C Essentials $70 / $700. B2C Professional $240 / $1,600. B2B Essentials $300 / $2,100. B2B Professional $800 / $2,400. — [Auth0 pricing](https://auth0.com/pricing)
- Self-serve tops out at 50k (B2C Essentials), 20k (B2C Pro and B2B Essentials), and 10k (B2B Pro). 100k MAU needs a sales contract. — [Auth0 pricing](https://auth0.com/pricing)
- B2B Essentials includes 3 enterprise connections, with extras at $100/mo each. B2B Professional includes 5. Enterprise MFA is a $100/mo add-on on B2B Essentials. — [Auth0 pricing](https://auth0.com/pricing)

**Stytch (now Twilio):**

- Twilio closed its acquisition of Stytch on 2025-11-14 for $104.1M cash. It was announced 2025-10-30. Stytch, Inc. stays the contracted processor. — [Twilio FY2025 10-K](https://www.sec.gov/Archives/edgar/data/1447669/000144766926000021/twlo-20251231.htm); [Stytch changelog](https://stytch.com/docs/resources/changelog); [Twilio blog](https://twilio.com/en-us/blog/company/news/twilio-to-acquire-stytch)
- Free tier: 10,000 MAU "and AI agents", unlimited B2B orgs, 5 SSO/SCIM connections, then $125 per connection. Custom branding is $99. The page does not list a per-MAU price above 10k ("volume discounts"). — [Stytch pricing](https://stytch.com/pricing)

**AWS Cognito:**

- SAML/OIDC federated users: 50 MAU free, then $0.015/MAU on every tier. — [AWS Cognito pricing](https://aws.amazon.com/cognito/pricing)
- Direct and social sign-in: Essentials is $0.015/MAU after 10,000 free, and Plus is $0.02/MAU with no free tier. — [Frontegg](https://frontegg.com/guides/aws-cognito-pricing); [SSOJet](https://ssojet.com/blog/how-does-the-pricing-of-aws-cognito-compare-to-its-alternatives). Contradicted by [Wring](https://wring.co/blog/aws-cognito-pricing-guide), which says 50,000 free. The AWS table was not confirmed directly.
- Cost estimate, using the 10k free figure: $0 at 1k and 10k, and about $1,350/mo at 100k on Essentials. Federated SAML users add $0.015 each.

**Microsoft Entra External ID:**

- The core offer is free for the first 50,000 MAU. Premium add-ons such as SMS and ID Governance have no free tier. The per-MAU rate above 50k was not visible. — [Microsoft Learn](https://learn.microsoft.com/en-gb/entra/external-id/external-identities-pricing); [Azure pricing](https://azure.microsoft.com/pricing/details/microsoft-entra-external-id/)

**UI and server-rendered fit (prior knowledge, not re-read):**

- WorkOS AuthKit is a hosted page. The app redirects to it and handles a code callback on the server, so it works with no app JS. See [WorkOS AuthKit docs](https://workos.com/docs/authkit).
- Clerk's main path uses JS components and a session refreshed by its frontend SDK. Clerk's Feb 2025 postmortem says the client refreshes the session token once a minute, which creates a "strong uptime dependency" on Clerk. — [Clerk postmortem Feb 6 2025](https://clerk.com/blog/postmortem-feb-6-2025-service-outage)
- Auth0 Universal Login, Cognito Managed Login and Entra all offer hosted redirect pages that work with plain OIDC code flow.

### Inferences

- WorkOS plus a Rust OIDC client fits Tetsu's ports and its no-JS CSP best. Tetsu keeps its own session cookie after an OIDC code exchange, and the vendor is just an OIDC issuer. Swapping vendors then means changing the issuer config, plus moving users.
- Charging $125/connection fits a "SAML on enterprise tier" price, as Linear and others do.
- Requiring MFA for every account removes Clerk's free tier and Auth0's Free plan.

### Gaps

- Descope and Google Identity Platform/Firebase pricing were not fetched this session. Firebase/GCIP (prior knowledge): 50k MAU free on Identity Platform, with SAML/OIDC charged per MAU after 50 free. Verify at [cloud.google.com/identity-platform/pricing](https://cloud.google.com/identity-platform/pricing).
- Stytch's per-MAU price above 10k, and Entra's rate above 50k, were not on the pages.
- Data residency and password-hash export were not verified per vendor. Prior knowledge: Auth0 exports hashes only through a support ticket. Cognito does not export hashes at all, so users must reset passwords or move through a lazy migration Lambda. WorkOS imports hashes from bcrypt, scrypt, PBKDF2 and Argon2. Clerk offers export. If Tetsu is passwordless (passkeys, email OTP, Google), hash export matters less.
- No vendor's uptime SLA numbers were collected, except Stytch Enterprise's stated 99.99%. — [Stytch pricing](https://stytch.com/pricing)

## Self-hosted identity servers: licensing, burden, multi-tenancy, SAML

**Takeaway:**

Self-hosting adds a second always-on stateful service, which a lone maintainer on Render must patch and monitor. Licenses have moved away from permissive: Zitadel went to AGPL in v3. Rauthy and Kanidm are Rust, but they are single-org IdPs, not multi-tenant CIAM. Ory Polis (Apache 2.0, formerly BoxyHQ SAML Jackson) is the self-host option most worth considering, as a SAML-only broker.

**Cited Findings:**

- Zitadel moved from Apache 2.0 to AGPL 3.0 with v3.0 (released around 2025-03-31). Older versions keep their old licenses, SDKs stay Apache 2.0, and a commercial license exists for anyone who cannot use AGPL. Using its APIs without modifying the code triggers no reciprocal duties. — [Zitadel blog](https://zitadel.com/blog/apache-to-agpl); [Zitadel license FAQ](https://zitadel.com/license-faq)
- Ory acquired BoxyHQ in May 2025 and relaunched SAML Jackson as Ory Polis, an SSO service for SAML and OIDC IdPs. Jackson was Apache 2.0. An Ory Enterprise License for Polis adds features, 24/7 support and CVE fixes. On Ory Network, SAML needs the Enterprise tier. — [Ory blog](https://www.ory.com/blog/introducing-ory-polis-for-enterprise-single-sign-on); [boxyhq/jackson](https://github.com/boxyhq/jackson); [Ory Polis docs](https://ory.com/docs/polis); [Ory community archive](https://archive.ory.sh/t/26852713/anyone-know-the-ory-position-of-saml-support-for-self-hosted)
- Rauthy is a Rust OIDC/OAuth2 IdP with MFA, FIDO2/WebAuthn passwordless sign-in and HA deployments. Passkey-only accounts must pass user verification. It has NLnet funding. — [Rauthy docs](https://sebadob.github.io/rauthy); [GitHub sebadob/rauthy](https://github.com/sebadob/rauthy); [NLnet](https://nlnet.nl/project/Rauthy/)

### Inferences (prior knowledge, not re-read; verify on the repos)

- Ory Kratos and Hydra are Apache 2.0, written in Go, and need Postgres. Kratos's self-service flows are API-first and need a UI you write yourself. Server-rendered forms work. Kratos has no native SAML in OSS. Multi-tenancy in OSS means one deployment per tenant, while Ory Network offers projects. [github.com/ory/kratos](https://github.com/ory/kratos)
- Keycloak is Apache 2.0 and a CNCF project. It runs on Java/Quarkus and needs roughly 512MB–1GB+ of RAM. It has full SAML IdP and SP support, plus Organizations (GA in v26). It is heavy for a small Render instance, and its frequent CVEs need prompt upgrades. [keycloak.org](https://www.keycloak.org)
- Authentik is MIT licensed, with an enterprise edition. It runs on Python/Django with Postgres and needed Redis until recently. It supports SAML and is aimed at workforce and homelab SSO, not customer identity.
- SuperTokens has an Apache 2.0 core with paid add-ons for MFA and multi-tenancy. It has no Rust SDK, so a Rust app talks to its core over HTTP.
- Authelia is a forward-auth portal and an OIDC provider. It is not CIAM and has no SAML.
- Kanidm is Rust, MPL-2.0, and is the home of webauthn-rs. It is a single-org IdP for people and devices, with OIDC but no SAML, and is not for multi-tenant customer accounts.
- Two Render services would be needed: the IdP plus its database. Some, such as Keycloak, need persistent memory sizing. That roughly doubles the ops surface Tetsu runs today.

**Gaps:**

- Star counts, release cadence and minimum resources were not fetched this session.
- The license file on the current ory/polis repo was not confirmed directly.

## Building in Rust: crates, maturity, SAML risk, size

**Takeaway:**

Passkeys, email OTP, TOTP, Argon2 and OIDC (Google) can be built from mature, maintained Rust crates. webauthn-rs is the anchor, and its README claims a SUSE audit. SAML is the exception. The only Rust SAML crate (samael) is pre-1.0, wraps the C library xmlsec1, and has docs.rs build failures. SAML signature handling keeps producing critical bypasses, even in mature libraries such as ruby-saml. Do not hand-roll SAML.

**Cited Findings:**

- The webauthn-rs README says it "has passed a security audit performed by SUSE product security". No public report, date or scope was found. — [docs.rs webauthn-rs 0.4.5 README](https://docs.rs/crate/webauthn-rs/0.4.5/source/README.md)
- Latest webauthn-rs is 0.5.5. GHSA-22w3-693w-x895 (2026-05-06) is an origin validation mismatch when subdomains are allowed. That option is off by default. It is rated low (CVSS 2.3) and fixed in webauthn-rs-core 0.5.5. — [GitLab advisory](https://advisories.gitlab.com/cargo/webauthn-rs-core/GHSA-22w3-693w-x895/); [docs.rs webauthn-rs](https://docs.rs/crate/webauthn-rs/latest)
- samael calls itself a work in progress. Its `xmlsec` feature uses a modified rust-xmlsec, which binds the C library xmlsec1. 0.0.21 failed to build on docs.rs, and the 0.0.22 docs point back to 0.0.18. Its test vectors include an `ancestor_attack_signed.xml` case. No RustSec advisory was found. — [samael README](https://docs.rs/crate/samael/0.0.20/source/README.md); [docs.rs 0.0.22](https://docs.rs/crate/samael/0.0.22); [test vectors](https://docs.rs/crate/samael/0.0.21/source/test_vectors/)
- ruby-saml shows how SAML fails in a mature library:
  - CVE-2025-25291 (DOCTYPE) and CVE-2025-25292 (namespaces), March 2025, are auth bypasses from a parser differential between REXML and Nokogiri during signature checks. CVSS is 9.8 for -25291. Fixed in 1.12.4 and 1.18.0. — [NVD CVE-2025-25291](https://nvd.nist.gov/vuln/detail/CVE-2025-25291); [NVD CVE-2025-25292](https://nvd.nist.gov/vuln/detail/cve-2025-25292); [Wiz](https://www.wiz.io/vulnerability-database/cve/cve-2025-25291)
  - CVE-2025-66567 (late 2025) is an incomplete fix for -25292, rated CVSS 9.1. — [Averlon](https://research.averlon.ai/vulnerability-intelligence/cve/CVE-2025-66567)
  - CVE-2024-45409 (2024) was an XML signature wrapping bypass in ruby-saml (prior knowledge, CVSS 10.0). It affected GitLab. — [NVD](https://nvd.nist.gov/vuln/detail/CVE-2024-45409)

**Inferences:**

- Reuse map (crate facts are prior knowledge; check versions on crates.io):
  - `openidconnect` and `oauth2` (ramosbugs) are mature, widely used and handle the Google sign-in flow.
  - `webauthn-rs` handles passkeys. It is an opinionated, safe API, and Kanidm uses it in production.
  - `argon2` (RustCrypto password-hashes) is needed only if passwords are offered at all.
  - `totp-rs` handles TOTP.
  - `tower-sessions`, backed by a Postgres store, or a small session table written in-house, handles sessions.
  - `axum-login` is optional. It is a thin glue layer, and a hand-written extractor may be simpler and easier to audit.
  - `passkey-rs` (1Password) is mostly an authenticator and client library, not a relying-party server. Prefer webauthn-rs for the server.
- Minimum to build: user, credential and session tables; email OTP with hashed, single-use, expiring codes and rate limits; a passkey register and sign-in flow; a Google OIDC callback that checks state, nonce and PKCE; TOTP or a passkey as the second factor; recovery codes; session rotation on privilege change; and an audit log. That is roughly 2–4k lines of Rust plus tests. It sits in ASVS V6/V7 territory and is Tier 1 under ADR-0005.
- Email OTP and magic links by themselves are not MFA under ASVS 5.0 L2. MFA for every account means a passkey with user verification, or OTP plus TOTP or a passkey.
- Passkeys work without JS. Doubtful: the WebAuthn API is JS-only (`navigator.credentials`). Passkeys therefore need a small same-origin script, which a strict CSP can allow with `script-src 'self'`. That is a real conflict with Tetsu's "no JavaScript yet" stance and is worth raising.

**Gaps:**

- No public SUSE audit report for webauthn-rs was found.
- No audit was found for samael, openidconnect or tower-sessions.
- Crate download counts and release dates were not fetched.

## Failure modes, breaches and outages

**Takeaway:**

Hosted identity concentrates risk in the vendor's support and ops: Okta's 2023 support breach, and Clerk's repeated outages, some caused by GCP. Building in-house concentrates risk in your own crypto and XML handling. A SAML bug is critical and easy to miss.

**Cited Findings:**

- Okta, October 2023: stolen service-account credentials, saved by an employee to a personal Google account, gave access to the support system. HAR files with session tokens from 134 customers were taken, and 5 customers' sessions were hijacked. On 2023-11-29, Okta said the attacker had also downloaded names and emails of **all** support-system users. Auth0/CIC's support system was not affected. — [BleepingComputer](https://www.bleepingcomputer.com/news/security/okta-breach-134-customers-exposed-in-october-support-system-hack/); [Krebs](https://krebsonsecurity.com/2023/11/okta-breach-affected-all-customer-support-users/); [Okta 8-K exhibit](https://www.sec.gov/Archives/edgar/data/1660134/000166013423000065/okta-10312023_ex992.htm)
- Clerk 2025-02-06: a bad update query took about 3,700 customers down for 26 minutes. Client retries nearly overloaded its systems. — [Clerk postmortem](https://clerk.com/blog/postmortem-feb-6-2025-service-outage)
- Clerk 2025-06-26: a full outage from 06:16 to 07:01 UTC came from a Google Cloud Run failure. It was the third major disruption since May 2025 that Clerk tied to GCP. — [Clerk postmortem](https://clerk.com/blog/postmortem-jun-26-2025-service-outage)
- Clerk 2025-09-14 to 18 had a database incident. 2026 postmortems cover a DNS provider outage on 2026-02-10, a system outage on 2026-02-19, and a Cloud SQL migration outage on 2026-03-10. — [Clerk Sept 2025](https://clerk.com/blog/2025-09-18-database-incident-postmortem); [Feb 10 2026](https://clerk.com/blog/2026-02-10-dns-outage-postmortem); [Feb 19 2026](https://clerk.com/blog/2026-02-19-system-outage-postmortem); [Mar 10 2026](https://clerk.com/blog/2026-03-10-service-outage-postmortem)

**Inferences:**

- With a hosted IdP behind a redirect and Tetsu's own session cookie, an outage at the IdP blocks only new sign-ins. Users who are already signed in keep working. Clerk's model of short-lived tokens refreshed every minute couples every request to the vendor.
- Lock-in comes mainly from user IDs and stored credentials. Keep Tetsu's own user ID as the primary key and store the provider's subject as an external identity row.
- Passkeys are bound to the relying-party ID (the domain), so they can move between vendors only when the domain is yours. A vendor-hosted domain, such as WorkOS without the $99 custom domain, strands passkeys on that domain.

**Gaps:**

- No Auth0, WorkOS, Stytch or Descope incident history was collected this session. No Auth0-specific incident was verified this session. WorkOS's status page is at [status.workos.com](https://status.workos.com).

## Hybrid: build the core, buy SAML

**Takeaway:**

Hybrid is the strongest fit. Build passkeys, email OTP, TOTP and Google OIDC in Rust behind a port. Add SAML later through a broker that exposes each customer's IdP to Tetsu as plain OIDC. That broker can be WorkOS SSO at $125 per connection, or self-hosted Ory Polis under Apache 2.0. Tetsu then never parses SAML XML.

**Cited Findings:**

- WorkOS sells SSO and Directory Sync per connection, separate from AuthKit: $125 each for connections 1–15. — [WorkOS pricing](https://workos.com/pricing)
- Ory Polis (formerly SAML Jackson) is an SSO service that brokers SAML and OIDC IdPs. It is open source, with a paid enterprise license for extra features and CVE support. — [Ory Polis docs](https://ory.com/docs/polis); [Ory blog](https://www.ory.com/blog/introducing-ory-polis-for-enterprise-single-sign-on)
- SAML parser-differential bypasses keep recurring in mature libraries, from 2024 through late 2025. — [NVD CVE-2025-25291](https://nvd.nist.gov/vuln/detail/CVE-2025-25291); [Averlon CVE-2025-66567](https://research.averlon.ai/vulnerability-intelligence/cve/CVE-2025-66567)

**Inferences:**

- Port shape: an `IdentityProvider`-style port with adapters for "local" (passkey, OTP, TOTP), "oidc" (Google, and any broker), and later "saml-broker" through the same OIDC adapter. The broker choice stays reversible.
- Mapping the three realms:
  - Operators: local passkeys only, with no social sign-in, plus an optional SAML connection for the operator org later.
  - Business staff: local plus Google, with SAML per business through the broker, and one person linked to many businesses through memberships.
  - Per-business customers: local passkey or email OTP, separate cookie scope and session table, no SAML.
- WorkOS as broker costs $0 until the first SAML customer and needs no ops. Polis costs one more Render service plus Postgres or another DB, but avoids per-connection fees and a US vendor dependency.
- If even the build is too much for one maintainer, the fallback is WorkOS AuthKit for staff and operators via OIDC redirect, at $0 below 1M MAU, with customer accounts local. Passkey portability then depends on paying for a custom domain.

**Gaps:**

- Polis's resource footprint (Node.js) and its Postgres or other DB needs on Render were not checked.
- Whether WorkOS SSO alone (without AuthKit) returns a standard OIDC flow that `openidconnect` can consume was not re-read in docs. Prior knowledge: WorkOS SSO has its own `/sso/authorize` and profile API, and AuthKit acts as an OIDC-like provider.
