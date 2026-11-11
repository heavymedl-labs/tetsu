# Multi-tenant identity architecture patterns for B2B2C SaaS

Context: Tetsu is Rust/axum, server-rendered, on Postgres. Tenants are served at `/<slug>` on one host today and may get custom domains later. It has three realms: platform operators, business staff (who may belong to several businesses), and end customers whose accounts belong to each business.

Source quality: items marked "(not re-fetched)" come from well-known standards I did not retrieve in this session. Check them before quoting.

## 1. Global user with memberships vs per-tenant pools vs hybrid

### Takeaway

The major vendors (Auth0, Clerk, WorkOS, Keycloak Organizations) default to one shared user pool, with organizations as membership links between users and tenants. Isolation between organizations is left to the application. Per-tenant pools (Cognito pool per tenant, a Clerk application per customer, Keycloak realm per tenant, an Entra external tenant) give strong isolation but cost a lot to operate, and users must sign up separately for each tenant. For a B2B2C booking product, a hybrid fits best: global staff and operator identities with memberships, plus customer identities scoped to one business.

### Cited Findings

- Clerk: "Organizations within one instance share a user pool." "If customers need isolated user pools or independent instance-level settings, use separate Clerk applications." Clerk names "Platforms" as the scenario where businesses "in turn, serve their own users", which is Tetsu's shape. Clerk for Platforms provisions an application per customer through APIs. — [Clerk multi-tenant architecture](https://clerk.com/docs/guides/how-clerk-works/multi-tenant-architecture)
- In Clerk's isolated model, "each organization has its own pool of users. A user must create a separate account for each organization." In the shared model, a user "can create one account and belong to multiple Organizations" with separate roles in each. — [Clerk architecture scenarios](https://clerk.com/docs/guides/multi-tenant-architecture)
- Auth0 documents two models, "users isolated by organization" and "users shared between organizations". In the shared model, "any user may belong to multiple organizations and should be able to use the same identity to navigate between organizations." — [Auth0 Multiple Organization Architecture](https://auth0.com/docs/get-started/architecture-scenarios/multiple-organization-architecture); [Auth0 Organizations overview](https://auth0.com/docs/manage-users/organizations/organizations-overview)
- Auth0 warns that the tenant dashboard "does not provide for organizational-level isolation" of user accounts. — [Auth0 profile management, single-IdP organizations](https://auth0.com/docs/get-started/architecture-scenarios/multiple-organization-architecture/single-identity-provider-organizations/profile-management.md)
- A third party (Scalekit) describes the Auth0 tenant as the security boundary. Org members are tenant-level users linked to an organization, not users created inside it. Clerk, a competitor, says isolation in both Clerk and Auth0 is a modeling choice the application must enforce. Both are vendor claims. — [Scalekit review](https://www.scalekit.com/blog/auth0-review-b2b-saas); [Clerk vs Auth0](https://clerk.com/articles/multi-tenant-b2b-auth-clerk-or-auth0)
- AWS Cognito, user pool per tenant: "maximum isolation", per-tenant configuration such as MFA and password policy, and "you can create multiple profiles for the same user. However, each user must sign up individually for each tenant." The level of effort "is high", and you must build the UI that routes users to the right pool. — [AWS Cognito user-pool multi-tenancy](https://docs.aws.amazon.com/cognito/latest/developerguide/bp_user-pool-based-multi-tenancy.html)
- Keycloak: Organizations became GA in Keycloak 26, inside a single realm. Realm-per-tenant has no hard limit, but the practical ceiling is a few hundred realms per cluster before you shard. These figures come from forums and vendor blogs, not official Keycloak docs. — [Skycloak realm scaling](https://skycloak.io/blog/keycloak-realm-scaling-how-many-realms-per-cluster/); [Phase Two multi-tenancy options](https://phasetwo.io/blog/multi-tenancy-options-keycloak/); [Keycloak forum](https://forum.keycloak.org/t/maximum-limit-of-realms/8189)
- Microsoft: External ID uses a distinct external tenant for consumer or business-customer apps. The multitenant guidance suggests External ID when customers "are individuals instead of organizations". I found no Microsoft guidance recommending one external tenant per SaaS customer. — [Entra External ID overview](https://learn.microsoft.com/entra/external-id/customers/overview-customers-ciam); [Azure Architecture Center, multitenant identity](https://learn.microsoft.com/azure/architecture/guide/multitenant/approaches/identity)
- WorkOS: users are global, and organizations hold memberships. Membership can be automatic through verified domains, JIT provisioning, or invitation (see section 4). — [WorkOS users and organizations](https://workos.com/docs/authkit/users-organizations.md)

### Inferences

- Staff fit the shared-pool-with-memberships model. One person works at two salons, and vendors converge on one identity with a role per membership.
- End customers fit the isolated-pool model. "Accounts belong to each business" is what Cognito and Clerk call the isolated or Platforms model. A customer of salon A and salon B has two unrelated customer accounts, even with the same email. Business A cannot see that the person is a customer of B, and deleting or exporting data stays per business.
- Hybrid in one database: a global `staff_user` (or `person`) table with `membership(tenant_id, user_id, role)`, and a separate `customer_account(tenant_id, …)` table with a key unique per tenant. Operators can be a third table or a flagged staff identity with a separate privileged session. Keeping the realms in separate tables stops a customer credential from ever satisfying a staff check. That control is the essence of ASVS 8.4.1 below.
- Vendors run per-tenant pools at high operational cost because each pool is a separate deployed resource. In one Postgres schema, a per-tenant pool is just a `tenant_id` column plus a uniqueness constraint, so the cost largely disappears.

### Gaps

- I could not retrieve current numeric limits for Auth0 Organizations (members per org, orgs per tenant). The architecture-scenarios URL returned 404.
- I did not confirm whether Clerk for Platforms is GA or priced as of late 2026.

## 2. Session and cookie scoping

**Takeaway:**

ASVS 5.0 requires `__Host-` cookies by default (3.3.3, L2), separate hostnames for separate applications (3.5.4), a fresh session token on every authentication (7.2.4), re-authentication before sensitive changes (7.5.1), and cross-tenant controls (8.4.1). On one host with `/<slug>` paths, the cookie cannot isolate tenants, because Path is not a security boundary. The server must bind each session to a realm and tenant and check that binding on every request.

**Cited Findings:**

- ASVS 5.0 V3.3: 3.3.1 (L1) sets `Secure`, and uses `__Secure-` if `__Host-` is not used. 3.3.2 (L2) sets SameSite by purpose. 3.3.3 (L2) uses the `__Host-` prefix "unless they are explicitly designed to be shared with other hosts". 3.3.4 (L2) sets HttpOnly on session tokens. 3.3.5 (L3) caps name and value at 4096 bytes. — [ASVS 5.0 V3 Web Frontend Security](https://github.com/OWASP/ASVS/blob/master/5.0/en/0x12-V3-Web-Frontend-Security.md)
- ASVS 3.5.4 (L2): "separate applications are hosted on different hostnames" so the same-origin policy and host-scoped cookies apply. 3.5.1 to 3.5.3 cover CSRF: anti-forgery tokens or non-safelisted headers, sensitive actions on non-safe methods, or strict Sec-Fetch-* validation. — [ASVS 5.0 V3](https://github.com/OWASP/ASVS/blob/master/5.0/en/0x12-V3-Web-Frontend-Security.md)
- ASVS 5.0 V7 (paraphrased from the source):
  - Token issuance: 7.2.1 verifies tokens at a trusted backend. 7.2.3 requires CSPRNG reference tokens of at least 128 bits. 7.2.4 (L1) generates a new token on authentication, including re-authentication, and ends the old one. This is the session-fixation control.
  - Timeouts: 7.3.1 and 7.3.2 set inactivity and absolute timeouts.
  - Termination: 7.4.1 makes server-side invalidation real. 7.4.2 kills all sessions when an account is disabled or deleted. 7.4.3 offers to end other sessions after a factor changes. 7.4.5 lets admins end sessions.
  - Re-authentication and session control: 7.5.1 (L2) requires full re-authentication before changing authentication or recovery attributes. 7.5.2 lets users see and end their sessions. 7.5.3 (L3) asks for step-up before highly sensitive operations.
  - Federation: 7.1.3 and 7.6.x document federated session lifetimes. 7.6.2 says no silent session creation without user action.
  - V7 itself has no explicit "session binding" requirement and defers cookie rules to V3.
  - Source: [ASVS 5.0 V7 Session Management](https://github.com/OWASP/ASVS/blob/master/5.0/en/0x16-V7-Session-Management.md)
- ASVS 8.4.1 (L2): "multi-tenant applications use cross-tenant controls to ensure consumer operations … will never affect tenants with which they do not have permissions to interact." It is the only multi-tenant requirement. Related: 8.2.2 (L1) data-level access against IDOR and BOLA, 8.3.1 (L1) enforcement at a trusted service layer, and 8.3.2 (L3) authorization changes applied immediately. — [ASVS 5.0 V8 Authorization](https://github.com/OWASP/ASVS/blob/master/5.0/en/0x17-V8-Authorization.md)
- The `__Host-` prefix requires `Secure`, `Path=/`, and no `Domain` attribute. This makes it host-only and incompatible with path scoping per tenant. Cookie Path does not isolate across paths on the same origin, because script on any path of the origin can read or set cookies for other paths. — (not re-fetched) [MDN Set-Cookie](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Set-Cookie); [RFC 6265 §8.6 "Weak Integrity"/§8.5](https://www.rfc-editor.org/rfc/rfc6265#section-8.5)

**Inferences:**

- Use one `__Host-` session cookie for the host. Its opaque reference token keys a server-side session row holding `realm` (operator, staff, or customer), `subject_id`, `tenant_id` (required for customer sessions), `auth_time`, and `amr` or step-up time. Each request's tenant comes from the URL slug, or from the Host header once custom domains exist. If it does not match the session's tenant for customer sessions, or the staff member has no membership there, the request fails closed as unauthenticated for that tenant. Never "switch" the session silently. This is how 8.4.1 applies to identity.
- Path-scoped cookies (`Path=/salon-a`) look tempting for per-tenant customer sessions, but they rule out `__Host-` (ASVS 3.3.3) and give no isolation. Better options are a single cookie whose server-side session holds several tenant-bound customer sessions, or cookie names keyed by tenant ID (`__Host-c_<tenant>`). Either way, isolation comes from the server-side check.
- Each tenant is not a "separate application" under 3.5.4, since one codebase serves them all. Still, all tenants' pages share one origin today. Any tenant-supplied HTML or script, such as custom themes or embeds, could reach other tenants' sessions, so tenant content must not run script on the main origin.
- Keep operator sessions apart. Use a separate cookie, ideally a separate hostname such as `admin.` (3.5.4), with short timeouts and step-up (7.5.3). A staff or customer cookie must never satisfy operator checks.
- Rotate the session ID on login, on step-up, and when a staff member changes active business, if the session caches tenant-scoped authorization (7.2.4).
- Cross-tenant session confusion risks:
  1. A customer session for tenant A is accepted at `/tenant-b` because the code checks only "logged in".
  2. A "remember me" or magic-link token minted for A is redeemed at B.
  3. A staff member's cached role from tenant A is applied after navigating to B.
  Every token (login link, reset, invite) should carry and verify `tenant_id` and `realm`.

**Gaps:**

- I did not fetch the OWASP Multi-Tenant Security Cheat Sheet, which may or may not exist under that name as of 2026.
- ASVS V6 (Authentication) and V10 (OAuth/OIDC) were not reviewed in this pass. V10 has relevant redirect-URI and `sub`/`iss` requirements.

## 3. Custom domains for tenants

**Takeaway:**

Cookies cannot cross registrable domains, so each custom domain needs its own session cookie, set after a redirect handshake with a central auth domain or after a local sign-in. Substack does a cross-domain auth redirect that mints a separate session cookie on the custom domain. Shopify's new customer accounts put sign-in on a Shopify-hosted subdomain of the merchant (for example `account.<shop>`), using emailed one-time codes and Shopify-provisioned TLS. Customer accounts there are per store.

**Cited Findings:**

- Shopify's new customer accounts use a one-time 6-digit code by email, with no password. Merchants can connect their own domain for the customer accounts page, and Shopify provisions the TLS certificate. — [Shopify Help: new customer accounts](https://help.shopify.com/en/manual/customers/customer-accounts/new-customer-accounts); [Shopify Help: customer login](https://help.shopify.com/en/manual/customers/customer-accounts/customer-login)
- Third-party guides add more detail. The account domain is a subdomain such as `account.` of the store's custom primary domain, which must not be myshopify.com. Only one account domain applies across all markets. Legacy (password) customer accounts were deprecated in 2026. These are secondary sources. — [EasySell guide](https://easysellapp.com/blogs/wiki/shopify-new-customer-accounts-setup-2026); [Ecommerce Partners](https://ecommercepartners.com/2024/10/01/shopifys-new-customer-accounts-advantages-and-limitations/)
- Substack on custom domains: the session is a `connect.sid` cookie set on the custom domain after Substack's cross-domain auth redirect. It is separate from the `substack.sid` cookie on substack.com, and the custom domain rejects the substack.com cookie. This comes from a third-party tool developer, not Substack docs. — [substack-mcp PR #166](https://github.com/conorbronsdon/substack-mcp/pull/166); [substack-mcp README](https://github.com/conorbronsdon/substack-mcp)
- Substack requires custom domains on a subdomain such as `www.`. — [Substack support](https://support.substack.com/hc/en-us/articles/360051222571-How-do-I-set-up-my-custom-domain-on-Substack)
- Clerk "satellite domains" give sign-in once across several domains of one application. The primary domain handles auth, and satellites sync through redirects. — [Clerk changelog 2023-09-01](https://clerk.com/changelog/2023-09-01)
- AWS's Cognito pool-per-tenant guidance has you "redirect users to their tenant-specific instance" and build the routing UI. — [AWS Cognito](https://docs.aws.amazon.com/cognito/latest/developerguide/bp_user-pool-based-multi-tenancy.html)

**Inferences:**

- Two workable patterns for Tetsu:
  - **(a) Local sign-in per domain.** Each custom domain serves its own sign-in pages, such as email one-time code or magic link, and sets its own host-only `__Host-` cookie. This fits per-business customer accounts well, because customers never need SSO across businesses.
  - **(b) Central auth host plus a one-time handoff code** (the Substack and Clerk satellite pattern). The custom domain redirects to `auth.tetsu…`, which authenticates and redirects back with a short-lived, single-use code bound to the target host and tenant. The custom domain exchanges it server-side for a local session. Use (b) only if a cross-domain identity is needed, which is mainly for staff.
- With OIDC social login on custom domains, each domain needs a registered redirect URI. Many IdPs cap or reject dynamic redirect URIs, so a central callback host with signed `state` that carries the return host is the common workaround. The return host must be checked against the tenant's verified domains to avoid open redirects. This is my inference, not a source finding.
- Certificates for tenant domains usually mean on-demand ACME, such as Caddy's on-demand TLS or cert-manager, gated by a check that the domain belongs to a verified tenant. Not researched here.
- Magic-link and code emails must name the tenant's domain and be redeemable only on it.

**Gaps:**

- I found no source for Ghost members sign-in on custom domains. It is believed to be per-site magic links with cookies on the site domain, but this is unverified.
- No source for Webflow memberships or user accounts on custom domains (Webflow retired its User Accounts feature; status unverified).
- No primary source for Shopify's current sender domain or cookie mechanics for the account subdomain.

## 4. Account linking risks

**Takeaway:**

Never link or merge identities on an email match alone. Pre-hijacking research found 35 of 75 popular services vulnerable. nOAuth showed that Entra `email` claims can be set by an attacker. The Google Workspace domain-resale issue showed that `email` and `hd` outlive the person. Key federated identities on `(iss, sub)`, or `(tid, oid)` for Entra, and link only after proving control of both sides.

**Cited Findings:**

- Sudhodanan and Paverd (Microsoft), "Pre-hijacked accounts", USENIX Security 2022. They tested 75 popular services and found at least 35 vulnerable to pre-hijacking. The attack classes are Classic-Federated Merge, Unexpired Session Identifier, Trojan Identifier, Unexpired Email Change, and Non-Verifying IdP. Reported counts per class (13 CFM, 19 unexpired session, 12 Trojan, 11 email change, 1 non-verifying IdP) come from press summaries and should be checked against the paper. The core fix is that a service merging classic and federated accounts must ensure "the user currently controls both accounts". — [USENIX page](https://www.usenix.org/conference/usenixsecurity22/presentation/sudhodanan); [arXiv 2205.10174](https://arxiv.org/abs/2205.10174v1); [The Register](https://www.theregister.com/2022/05/25/web_pre_hijacking/)
- nOAuth (Descope, June 2023): an attacker sets the victim's email on a user in their own Entra tenant. Apps that key on the `email` claim log the attacker in as the victim. — [Descope](https://descope.com/blog/post/noauth)
- Microsoft's remediation guidance is to identify users by `tid` + `oid`, not email. Apps registered after June 2023 don't receive unverified email claims by default. The optional `xms_edov` claim reports whether the email domain owner is verified. — [Microsoft Learn: migrate off email claim authorization](https://learn.microsoft.com/en-us/entra/identity-platform/migrate-off-email-claim-authorization)
- Semperis (2025) found 9 of 104 Entra gallery apps still vulnerable. It also reported that `xms_edov` was flagged as "not a valid optional claim" via Graph, which raises doubt about its status. — [Semperis](https://www.semperis.com/blog/noauth-abuse-alert-full-account-takeover/); [Infosecurity Magazine](https://www.infosecurity-magazine.com/news/microsoft-noauth-flaw-2025/)
- Descope docs: linking "regardless of email verification status" allows nOAuth-style takeover. Link only if the email is verified. — [Descope: unverified OAuth emails](https://docs.descope.com/auth-methods/oauth/customize/handle-oauth-provider-unverified-emails)
- Okta's response to nOAuth. — [Okta Security](https://sec.okta.com/articles/2023/08/saying-no-thanks-noauth/)
- Google domain resale (Truffle Security, January 2025): buying a defunct startup's domain and re-creating employee mailboxes gave access to their accounts on Slack, Notion, HR systems and others. Those apps keyed on `email` and `hd` rather than `sub`. Truffle claimed `sub` was inconsistent in about 0.04% of logins and counted over 100,000 purchasable failed-startup domains. Google first closed the report as won't-fix, reopened it, and later said `sub` "is a strong and appropriate protection". A rebuttal argues it's the relying party's fault. — [Truffle Security](https://trufflesecurity.com/blog/millions-at-risk-due-to-google-s-oauth-flaw); [SecurityWeek](https://www.securityweek.com/google-oauth-flaw-leads-to-account-takeover-when-domain-ownership-changes/); [Authress rebuttal](https://dev.to/authress/are-millions-of-accounts-vulnerable-due-to-googles-oauth-flaw-33f)
- Domain-based auto-join (WorkOS): users with a verified-domain email are auto-added via domain policy, and this is on by default. Domains are verified by DNS TXT. WorkOS says to "restrict any email-based JIT user provisioning to within the organization that matches this ID" to prevent cross-org impersonation. One vendor using auto-join warns that "anyone with an email on the verified domain who signs in will become a member". — [WorkOS JIT](https://workos.com/docs/authkit/jit-provisioning); [WorkOS domains](https://workos.com/docs/sso/domains); [WorkOS blog, org access](https://workos.com/blog/how-workos-decides-organization-access); [QA.tech](https://docs.qa.tech/configuration/organization-access.md)

**Inferences:**

- For Tetsu: store `federated_identity(realm, tenant_id NULL for staff, issuer, subject)` with a unique key on `(issuer, subject[, tenant_id])`. Email is a contact attribute, not a key.
- To link a Google login to an existing password or code account, require the user to be signed in to the existing account first. Alternatively, require a fresh proof of control through the existing account's factor. Never auto-merge on `email_verified`.
- Defeat pre-hijacking at sign-up. Don't create a usable account until the email is verified. When verification completes, expire every session and pending token created before it. Expire email-change tokens.
- Domain auto-join for staff (for example "anyone @chainsalon.com joins") is risky for small businesses: domains lapse, and personal-domain email is shared. Prefer explicit invitations. If auto-join is ever added, require DNS verification and periodic re-checks.

**Gaps:**

- I did not verify whether Google changed `sub` stability or published guidance after January 2025.
- I did not confirm the current status of `xms_edov` (deprecated or supported).

## 5. Tenant isolation of identity data in Postgres

**Takeaway:**

Put `tenant_id` on every tenant-scoped identity table: customer accounts, customer sessions, login tokens, and memberships. Scope uniqueness such as email per tenant for customers, and keep it global for staff. RLS is a useful second layer only when the app role is not the owner, has no BYPASSRLS, and tenant context is set per transaction.

**Cited Findings:**

- Table owners bypass RLS unless `FORCE ROW LEVEL SECURITY` is set. Superusers and BYPASSRLS roles always bypass it. These failures are silent. — [Bytebase RLS footguns](https://www.bytebase.com/blog/postgres-row-level-security-footguns/); [Tessl reference](https://tessl.io/registry/testland/row-level-security-postgres-reference/files/references/policy-and-bypass-patterns.md); official background: [PostgreSQL docs, Row Security Policies](https://www.postgresql.org/docs/current/ddl-rowsecurity.html) (not re-fetched)
- With pooled connections, a session-level `SET app.tenant_id` leaks to the next borrower. Use `set_config('app.tenant_id', $1, true)` or `SET LOCAL` inside the transaction so it reverts at commit or rollback. — [DEV (mvpfactory)](https://dev.to/software_mvp-factory/postgresql-row-level-security-for-multi-tenant-saas-1lgp); [telemetry-landing](https://telemetry-landing.pages.dev/blog/postgres-row-level-security-in-practice/)
- Tests that connect as `postgres` (superuser) hide RLS failures, so test as the app role. Views can bypass RLS unless created with `security_invoker` (PG15+). — [DEV RLS traps](https://dev.to/wenceslaudev/postgres-rls-multi-tenancy-two-traps-that-silently-disable-your-policies-5gn8); [skills.lc RLS](https://www.skills.lc/troykelly/codex-skills/troykelly-codex-skills-skills-postgres-rls-skill-md) (community sources)

**Inferences:**

- Customer email uniqueness: `UNIQUE (tenant_id, lower(email))`, or use citext. This lets the same person hold independent accounts at two businesses, matching the Cognito and Clerk isolated-pool model. Staff: `UNIQUE (lower(email))` globally, with `membership UNIQUE (tenant_id, user_id)`.
- Use composite foreign keys `(tenant_id, customer_id)`, so a booking cannot reference a customer from another tenant even through an app bug. Postgres enforces this without RLS.
- Login and reset tokens should be stored hashed with `tenant_id` and `realm`, and lookups should filter on both. This is the database side of the session binding in section 2.
- Sign-in lookups happen before tenant context is known for staff. A login path that bypasses RLS should be a narrow, separately reviewed function, such as a `SECURITY DEFINER` function returning only an ID and hash.
- Email case and Unicode normalization affect uniqueness and account confusion. Not researched here.

**Gaps:**

- No primary benchmark on RLS overhead for identity-sized tables. Most sources are blogs.

## 6. Guardians, dependents, accounts without email, and shared family emails

**Takeaway:**

The most mature prior art is healthcare patient-portal "proxy access". The guardian has their own verified account and is granted access to a dependent's record. Young children have no login at all, teens can get their own accounts at an age threshold (often 12 or 13), and the proxy is converted or revoked at 18. Most portals require each login holder to have a unique email.

**Cited Findings:**

- Proxies are invited by email and access the child's record through their own portal account. — [Children's Mercy](https://www.childrensmercy.org/your-visit/patient-portal/accessing-accounts-for-patients-over-18-years); [LCMC Health](https://www.manningchildrens.org/patients-visitors/lcmc-health-patient-portal/proxy-access/)
- Proxy access for children aged 0 to 11 or 12 requires parental rights or guardianship documentation, sometimes checked in person. — [Baptist Health form](https://res.cloudinary.com/baptisthealth/image/upload/v1611176823/BaptistJax/images/pdf/Patient_Portal_Access_Request_Form.pdf); [LCMC](https://www.manningchildrens.org/patients-visitors/lcmc-health-patient-portal/proxy-access/)
- Teens can request their own account from 13 at Brown Health. Munson stops guardian proxy access at 13. UCLA runs a separate adolescent (12 to 17) proxy model for privacy. — [Brown Health](https://www.brownhealth.org/node/289136); [Munson](https://www.munsonhealthcare.org/patient-portals/proxy-access); [UCLA Health](https://www.uclahealth.org/patient-resources/myuclahealth/sign-up-myuclahealth/adolescent-proxy-access)
- At 18, new adults are asked to confirm their own email. — [UT Physicians](https://www.utphysicians.com/adult-proxy/)
- A practice-management portal (Sessions Health) requires each portal user to have their own email address. — [Sessions Health](https://support.sessionshealth.com/article/410-client-portal-for-minors)
- Guardian access to teen portals raises privacy problems. — [TechTarget HealthTech Security](https://www.techtarget.com/healthtechsecurity/news/366595037/Guardian-Patient-Portal-Access-Impacts-Adolescent-Patient-Privacy)

**Inferences:**

- Model separate entities for Tetsu: a `customer_account`, which signs in, has a verified contact, and is tenant-scoped, and a `client_profile`, the person receiving the service, which may have no email. Link them with `account_profile(account_id, profile_id, relationship: self|guardian|carer)`. A booking references the profile and records the booking account.
- When a family shares one email, make it one account with several profiles, not several accounts on the same email. This keeps uniqueness on `(tenant_id, email)` and avoids login ambiguity.
- Moving a dependent to their own account (coming of age) should create a new account and grant `self` on the existing profile. Revoking the guardian link should be explicit and audited. Data ownership and consent rules vary by jurisdiction (COPPA, GDPR Art. 8) and were not researched here.

**Gaps:**

- I found no booking-SaaS vendor documentation (for example Mindbody, Fresha, Jane App, or Acuity) describing family or dependent models. Mindbody and Jane App are known to have family or relationship features, but this is unverified in this session.
- Legal age thresholds and consent requirements were not researched.
