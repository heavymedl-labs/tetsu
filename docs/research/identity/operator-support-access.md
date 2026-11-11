# Operator and support access to customer tenants

Scope: how SaaS vendors let their own staff (operators, support) reach customer tenants safely, for a multi-tenant booking SaaS where tenant isolation is Tier 1 and every operator access must be audited and visible to the business. Researched 2026-11-11. Budget-limited: about 16 tool calls, so several vendors are gaps (see each section).

## Access patterns: consent, time-limited grants, approval, separate consoles

### Takeaway

The mature pattern is "no standing access": the operator files a scoped, ticket-linked request, a second internal person approves, the customer approves (or pre-grants a time window), access auto-expires within hours, and every operator action lands in the customer's own audit log under a distinct operator identity. Microsoft Customer Lockbox and Google Access Approval are the reference designs; Atlassian, Zendesk and Shopify show lighter-weight SaaS versions.

### Cited Findings

**Microsoft Customer Lockbox (Microsoft 365):**

- Engineer request includes tenant name, service request number, expected start time, estimated duration and target service; a Microsoft Support manager approves first, then the customer's designated approver is emailed — [Microsoft Learn: Customer Lockbox requests](https://learn.microsoft.com/en-us/purview/customer-lockbox-requests)
- Only people with the customer-side "Customer Lockbox access approver" role (or Global Administrators) can approve; Microsoft cannot change the approver list because it is controlled by the customer's own Entra ID group membership — [Microsoft Learn](https://learn.microsoft.com/en-us/purview/customer-lockbox-requests)
- Unapproved requests expire after 12 hours; approved access lasts at most 4 hours (engineer may request less) and is revoked automatically — [Microsoft Learn](https://learn.microsoft.com/en-us/purview/customer-lockbox-requests)
- After approval the engineer acts "by using pre-approved cmdlets" (a constrained tool surface, not free-form access) — [Microsoft Learn](https://learn.microsoft.com/en-us/purview/customer-lockbox-requests)
- Internally, Microsoft's "Lockbox" requires a second person, who "can't be the requestor", to approve any elevation; "no Microsoft personnel have permission to access customer content on an ongoing basis" (least privilege + just-in-time) — [Microsoft Learn](https://learn.microsoft.com/en-us/purview/customer-lockbox-requests)
- Notification emails deliberately contain no sign-in links (anti-phishing) — [Microsoft Learn](https://learn.microsoft.com/en-us/purview/customer-lockbox-requests)
- Licensing: included in M365/O365 E5 (G5 for government) or via add-on, i.e. sold as a premium compliance feature — [Microsoft Learn](https://learn.microsoft.com/en-us/purview/customer-lockbox-requests)
- No built-in alert on approval; Microsoft points customers to Defender for Cloud Apps policies — [Microsoft Learn](https://learn.microsoft.com/en-us/purview/customer-lockbox-requests)

**Google Cloud Access Approval / Access Transparency:**

- Google administrators need approval from an authorized customer administrator before accessing Customer Data; approval via console or API; active approvals "may be revoked at any time" — [Google Cloud: Access Approval overview](https://docs.cloud.google.com/assured-workloads/access-approval/docs/overview)
- Access Transparency must be enabled first; its logs capture actions Google personnel take on Customer Data, and Access Approval layers control on top — [Google Cloud](https://docs.cloud.google.com/assured-workloads/access-approval/docs/overview)
- Scoping: enable per project, folder or organization, and per service — [Google Cloud](https://docs.cloud.google.com/assured-workloads/access-approval/docs/overview)
- Break-glass exception: requests "may be auto-approved without customer action to address time-sensitive outages", logged in an "auto approved" state; auto-approval is disabled under EU Data Boundary with Access Justifications and Sovereign Controls — [Google Cloud](https://docs.cloud.google.com/assured-workloads/access-approval/docs/overview)
- Notification via preconfigured email or Pub/Sub; each approval is cryptographically signed, with a Google-managed key or a customer key in Cloud KMS / Cloud EKM — [Google Cloud](https://docs.cloud.google.com/assured-workloads/access-approval/docs/overview)

**Atlassian Cloud:**

- Support does not access data without explicit permission; tickets show a "Consent Requested" tag with Approve / Reject — [Atlassian Cloud support data access request FAQ](https://support.atlassian.com/atlassian-cloud/kb/atlassian-cloud-support-data-access-request-faq/)
- Access is revoked manually or within 24 hours after ticket closure, and can be revoked at any time; with Atlassian Guard support restrictions, a Guard admin must also approve every grant (two-party customer approval) — [Atlassian FAQ](https://support.atlassian.com/atlassian-cloud/kb/atlassian-cloud-support-data-access-request-faq/) (summarized via search snippet; not fetched in full)
- Partners raising tickets may approve only if they hold admin or billing/technical contact role — [Atlassian Statuspage: customer data access grants](https://support.atlassian.com/statuspage/docs/customer-data-access-grants-information/) (via search snippet)
- In government cloud, support engineers may reach data "through the database and site front end as a user or admin" — [Atlassian Government Cloud support](https://support.atlassian.com/organization-administration/docs/atlassian-government-cloud-support-details/) (via search snippet)

**Zendesk "account assumption" (customer pre-grants a window):**

- Admin enables it under Admin Center > Security > Advanced, choosing one day, one week, one month, one year, or indefinitely; it switches off automatically at expiry — [Zendesk help 115001753608](https://support.zendesk.com/hc/en-us/articles/115001753608) (via search snippet)
- All admins are emailed when it is switched on; trial accounts always have it enabled — [Zendesk help 4408830808474](https://support.zendesk.com/hc/en-us/articles/4408830808474); [Zendesk help 115001753608](https://support.zendesk.com/hc/en-us/articles/115001753608) (via search snippet)
- Datadog ships a detection rule flagging "Zendesk account assumption is enabled", i.e. security teams treat a standing grant as a risk — [Datadog rule](https://docs.datadoghq.com/security/default_rules/zendesk-account-assumption-settings)

**Shopify collaborator accounts (third-party staff, owner-scoped):**

- Owner can require a 4-digit collaborator request code (Settings > Users and permissions); partner requests specific permissions; owner may grant all, some, or none before accepting; only the owner or staff with "Manage collaborator requests" can approve — [Shopify Partners blog](https://www.shopify.com/partners/blog/shopify-collaborator-account); [shopify.dev collaborations](https://shopify.dev/docs/apps/build/dev-dashboard/stores/collaborations) (via search snippet; most other results were agency blogs)
- If the partner's organization already has an app installed on the store, it may request access without a code — [shopify.dev](https://shopify.dev/docs/apps/build/dev-dashboard/stores/collaborations) (via search snippet)

### Inferences

- For Tetsu, a "support grant" object (ticket reference, scope, requested duration, internal approver, business approver, expiry, revocation) maps directly to the Lockbox/Access Approval model and gives a single place to hang audit and notification.
- Read-only by default with a constrained action set (Lockbox's "pre-approved cmdlets") is safer than full "log in as". If impersonation exists, it should run under a distinct operator identity, never the business user's own session or credentials (see ASVS 8.3.3 and 6.4.6 below).
- Long or indefinite pre-grants (Zendesk) are convenient but are flagged by detectors; cap grant length (hours, not months) and make expiry automatic.
- A Google-style auto-approved break-glass path is worth having for outages, but must be logged distinctly and shown to the business after the fact.
- Customer approval links should not be in emails (Microsoft's anti-phishing note); show the request in-app.

### Gaps

- Intercom, Stripe and GitHub support-access practices: searches found no primary documentation (GitHub results were only audit-log docs). Not verified.
- AWS: no AWS Lockbox-equivalent researched; I did not fetch AWS docs. Treat any claim of an AWS "Access Approval" product as unverified.
- Google Access Transparency justification codes and log fields were not fetched.
- Shopify's own help center page on collaborators returned an unrelated page; details come from Shopify's partner blog and shopify.dev via snippets.

## Securing operator accounts themselves

**Takeaway:**

Every incident below was an attack on operator identity, not tenant isolation logic. Phishing-resistant FIDO2/WebAuthn keys, device-bound sessions, a separate hardened support environment, and human approval for high-impact actions are the recurring lessons; ASVS 8.4.2 (L3) codifies "multiple layers" for admin interfaces.

**Cited Findings:**

- NYDFS found Twitter's admin tools were protected by no more than SMS-style MFA and said a hardware MFA policy could have helped prevent the 2020 attack — [Dark Reading on NYDFS report](https://www.darkreading.com/threat-intelligence/twitter-hack-analysis-drives-calls-for-greater-security-regulation/d/d-id/1339193) (via search snippet)
- Retool: SMS phish plus deepfake voice call obtained an MFA code; Google Authenticator cloud sync turned MFA into "single-factor-authentication"; Retool recommends FIDO2 hardware keys and says "anything that can be done by an employee can be socially engineered out of that employee too" — [Retool blog](https://retool.com/blog/mfa-isnt-mfa)
- Retool's internal support instance required VPN, SSO and MFA, yet was still abused; Retool added human approval for important internal actions — [Retool blog](https://retool.com/blog/mfa-isnt-mfa)
- Okta 2023: service account credentials saved into an employee's personal Google profile in Chrome on a managed laptop; remediation included blocking personal Google profiles in Chrome on managed laptops and network-location session token binding for admins (re-authenticate on network change) — [Okta RCA, Nov 2023](https://sec.okta.com/articles/2023/11/unauthorized-access-oktas-support-case-management-system-root-cause)
- Okta later shipped Zero Standing Privileges for admins, MFA for protected admin actions, anonymizer blocking via Dynamic Zones, IP binding for sessions and the Admin Console, and allowlisted network zones for APIs — [Okta closure post, Feb 2024](https://sec.okta.com/harfiles)
- After Lapsus$/Sitel, Okta reportedly began managing devices of third parties that access its support tools — [Technewsday](https://technewsday.com/?p=21932) (headline only; not verified)
- ASVS 5.0.0 6.3.3: MFA required (L2); "For L3, one of the factors must be a hardware-based authentication mechanism which provides compromise and impersonation resistance against phishing attacks" — [ASVS 5.0.0 V6](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x15-V6-Authentication.md)
- ASVS 5.0.0 8.4.2 (L3): admin interfaces need "continuous consumer identity verification, device security posture assessment, and contextual risk analysis", with network location not the sole factor — [ASVS 5.0.0 V8](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x17-V8-Authorization.md)

**Inferences:**

- Tetsu operators should use WebAuthn/passkeys only (no SMS, no TOTP), ideally hardware-bound, on a separate operator realm and hostname (ASVS 3.5.4 supports separate hostnames for separate apps).
- No shared or service accounts in support tooling (Okta 2023 root cause); short absolute session lifetimes; re-auth on IP/network change; step-up before each grant use.
- Avoid outsourced support with full tool access, or contractually require device management (Okta/Sitel).

**Gaps:**

- Specific session lengths or idle timeouts used by vendors for operator consoles: not found in fetched sources.
- Separate IdP practice for staff vs customers: no vendor source fetched.

## Audit design: what to log, tamper resistance, exposure and notification

**Takeaway:**

Lockbox shows the target: operator actions appear in the customer's own audit log under a distinctive actor ("Microsoft Operator") with time and IP, alongside the approval record. ASVS V16 requires who/when/where/what metadata, immutability, and shipping to a logically separate system. Okta's 2023 miss shows that incomplete event coverage (file views logged differently from downloads) delays detection.

**Cited Findings:**

- Lockbox approval record: date, approver IP, service account, activity `Set-AccessToCustomerDataRequest`, request GUID; engineer actions logged with user "Microsoft Operator", IP of the engineer's machine, activity name, within 4 hours of approval — [Microsoft Learn](https://learn.microsoft.com/en-us/purview/customer-lockbox-requests)
- Customers can cross-check notification emails against request history in the admin center — [Microsoft Learn](https://learn.microsoft.com/en-us/purview/customer-lockbox-requests)
- Google signs each approval cryptographically, optionally with a customer-held key — [Google Cloud](https://docs.cloud.google.com/assured-workloads/access-approval/docs/overview)
- Okta missed suspicious downloads for about 14 days because viewing files in the Files tab produced different log events than the ones monitored, and logs lacked the final hours of access; detection came from customers (1Password Sep 29, BeyondTrust Oct 2) — [Okta RCA](https://sec.okta.com/articles/2023/11/unauthorized-access-oktas-support-case-management-system-root-cause)
- ASVS 5.0.0 16.2.1: each log entry includes "when, where, who, what"; 16.3.2: log failed authorization, and for L3 "all authorization decisions, including logging when sensitive data is accessed"; 16.4.2: logs "protected from unauthorized access and cannot be modified"; 16.4.3: logs sent "to a logically separate system" so a breach does not compromise them; 16.1.1: a log inventory including who can access logs and retention; 16.2.5: no credentials, hash/mask session tokens — [ASVS 5.0.0 V16](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x25-V16-Security-Logging-and-Error-Handling.md)
- GitHub enterprise audit logs are retained indefinitely by default unless an owner configures otherwise; default UI view is three months — [GitHub Docs: enterprise audit log](https://docs.github.com/en/enterprise-server@3.19/admin/monitoring-activity-in-your-enterprise/reviewing-audit-logs-for-your-enterprise/accessing-the-audit-log-for-your-enterprise) (via search snippet)

**Inferences:**

- Log at least: grant requested/approved/denied/expired/revoked; every operator read of tenant data (not only writes), every action, operator identity, tenant, ticket, IP, device, timestamp UTC (ASVS 16.2.2).
- Store operator-access events append-only in a separate store from tenant data, and render a tenant-filtered view to the business ("Tetsu staff accessed your account").
- Notify business owners in-app and by email (without sign-in links) on request, approval, first use and expiry.
- Hash-chaining or signing log entries is one way to meet 16.4.2; Google's signed approvals are precedent. No source fetched on log hash chains specifically.

**Gaps:**

- Vendors' exact retention periods for operator-access logs shown to customers (beyond GitHub's default) not found.
- Whether Microsoft/Google logs are hash-chained or WORM-stored internally: not documented in fetched pages.

## Incidents involving support/admin tooling and lessons

**Takeaway:**

Support tools are a prime target because they hold cross-tenant power. Attackers reached them through phished staff (Twitter, Mailchimp, Retool), outsourced support workstations (Okta 2022), and leaked service credentials (Okta 2023). Snowflake 2024 was customer credentials without MFA, not operator tooling, but carries the same MFA lesson.

**Cited Findings:**

- **Twitter, July 2020:** phone calls posing as IT help desk about VPN issues; first victim lacked needed access, so attackers learned processes and targeted others; over 1,000 employees had access to the internal tools; Twitter restricted access afterwards — [Dark Reading](https://www.darkreading.com/threat-intelligence/twitter-hack-analysis-drives-calls-for-greater-security-regulation/d/d-id/1339193); [TechTarget](https://www.techtarget.com/searchsecurity/news/252490684/Twitter-hackers-posed-as-IT-staff-used-VPN-issues-as-a-lure) (via search snippets; NYDFS report itself not fetched)
- **Okta / Lapsus$ via Sitel, Jan 2022:** attacker controlled a Sitel support engineer's workstation for 25 minutes on Jan 21; reached two active customer tenants in the "SuperUser" application; initial estimate was 366 customers; Okta ended its Sitel relationship and apologised for slow disclosure (Sitel report not received until Mar 17) — [The Record](https://therecord.media/okta-says-two-customers-breached-during-january-security-incident/); [Malwarebytes](https://www.malwarebytes.com/blog/news/2022/03/okta-admits-366-customers-may-have-been-impacted-by-lapsus-breach); [GovInfoSecurity](https://www.govinfosecurity.com/okta-hackers-accessed-just-2-customer-tenants-in-breach-a-18917)
- **Okta support system, Sep 28 – Oct 17, 2023:** service account in support case system; 134 customers had files accessed; some HAR files held session tokens; 5 customers' sessions hijacked (1Password, BeyondTrust, Cloudflare among them) — [Okta RCA](https://sec.okta.com/articles/2023/11/unauthorized-access-oktas-support-case-management-system-root-cause). Stroz Friedberg's independent investigation confirmed Okta's conclusions; Okta changed Help Center data retention and how access is provisioned to customer admins — [Okta closure post](https://sec.okta.com/harfiles)
- **Mailchimp, Jan 2023:** employee credentials from social engineering used to enter an internal customer support and account administration tool; 133 accounts affected (including WooCommerce); intrusion Jan 11; primary contacts notified within 24 hours. Earlier Aug 2022 incident reported as 214 or 216 accounts (sources differ) — [The Record](https://therecord.media/more-than-100-mailchimp-accounts-accessed-via-social-engineering-cyberattack/); [The Hacker News](https://thehackernews.com/2023/01/mailchimp-suffers-another-security.html)
- **Retool, Aug 27, 2023:** 27 cloud customers (all crypto) affected; attacker used the internal support instance to change user emails and reset passwords; on-prem unaffected — [Retool blog](https://retool.com/blog/mfa-isnt-mfa)
- **Snowflake, 2024 (UNC5537):** about 165 organizations notified; no platform flaw; accounts lacked MFA and had unrotated credentials, mostly from infostealer infections back to 2020; Mandiant said MFA would have prevented it. Note: "over 100 tenants accessed" vs "165 notified" are different counts — [SiliconANGLE](https://siliconangle.com/2024/06/10/mandiant-finds-165-snowflake-customers-targeted-hacking-campaign/); [Duo Decipher](https://duo.com/decipher/mandiant-165-snowflake-customers-potentially-exposed-in-wider-campaign); [SecurityWeek](https://www.securityweek.com/snowflake-attacks-mandiant-links-data-breaches-to-infostealer-infections/)

**Inferences:**

- The "change email + reset password" capability (Retool, Mailchimp) is the account-takeover primitive. Operators should never change a business's sign-in email or credentials directly; ASVS 6.4.6 (admins may start a reset but not choose the password) applies.
- Breadth of who holds support access matters (Twitter's 1,000+); keep the operator set small and grants per-ticket.
- Support artifacts customers upload (HAR files) can carry live tokens; sanitize or reject them, and expire operator-visible uploads.
- Customer-visible logs let customers detect abuse (1Password and BeyondTrust caught Okta 2023 first).

**Gaps:**

- Cloudflare's follow-on (Thanksgiving 2023 compromise of its Atlassian environment with credentials not rotated after the Okta incident) is from my background knowledge; no source was fetched. Verify against Cloudflare's own blog before citing.
- Mailchimp's own post-mortem blog was not fetched; figures come from press.
- NYDFS Twitter report not fetched directly.

## ASVS 5.0.0 on administrative interfaces and privileged access

**Takeaway:**

ASVS 5.0.0 has one requirement aimed at admin interfaces (8.4.2, L3) and one for multi-tenancy (8.4.1, L2); the rest come from general authorization, session, authentication and logging requirements. V13 (Configuration) speaks to service accounts and least privilege rather than admin consoles. Text below is from the `v5.0.0_release` tag.

**Cited Findings:**

Source for V8: [ASVS 5.0.0 V8 Authorization](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x17-V8-Authorization.md)

- **8.4.1 (L2):** "Verify that multi-tenant applications use cross-tenant controls to ensure consumer operations will never affect tenants with which they do not have permissions to interact."
- **8.4.2 (L3):** "Verify that access to administrative interfaces incorporates multiple layers of security, including continuous consumer identity verification, device security posture assessment, and contextual risk analysis, ensuring that network location or trusted endpoints are not the sole factors for authorization even though they may reduce the likelihood of unauthorized access."
- **8.2.1 (L1) / 8.2.2 (L1):** function-level and data-specific access restricted to consumers with explicit permissions (IDOR/BOLA).
- **8.3.1 (L1):** enforce authorization at a trusted service layer.
- **8.3.2 (L3):** authorization changes apply immediately (relevant to revoking a grant).
- **8.3.3 (L3):** access is based on "the originating subject's (e.g. consumer's) permissions, not on the permissions of any intermediary or service acting on their behalf" (relevant to impersonation: the operator's grant, not the business user's rights, should be checked).
- **8.1.3 / 8.1.4 / 8.2.4 (L3):** document and enforce contextual attributes (time, location, IP, device) and step-up actions, at session start and during the session.

Source for V6/V7: [ASVS 5.0.0 V6](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x15-V6-Authentication.md), [ASVS 5.0.0 V7](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x16-V7-Session-Management.md)

- **6.3.2 (L1):** no default accounts such as "root", "admin", "sa".
- **6.3.3 (L2; hardware phishing-resistant factor at L3)** as quoted above.
- **6.3.4 (L2):** no undocumented authentication pathways; strength enforced consistently (an impersonation path is an authentication pathway).
- **6.4.6 (L3):** "administrative users can initiate the password reset process for the user, but that this does not allow them to change or choose the user's password."
- **6.8.4 (L2):** with a separate IdP, verify required strength/recency via claims such as `acr`, `amr`, `auth_time`.
- **7.3.1 / 7.3.2 (L2):** inactivity timeout and absolute lifetime set by documented risk analysis; **7.1.1 (L2)** document them with justification against NIST SP 800-63B.
- **7.4.5 (L2):** administrators can terminate sessions for one user or all users.
- **7.5.3 (L3):** further authentication before "highly sensitive transactions or operations".

Source for V16: [ASVS 5.0.0 V16](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x25-V16-Security-Logging-and-Error-Handling.md)

- **16.1.1, 16.2.1, 16.2.2, 16.2.5, 16.3.1, 16.3.2, 16.4.2, 16.4.3** (all L2) as quoted in the audit section.

Source for V13 and V3: [ASVS 5.0.0 V13](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x22-V13-Configuration.md), [ASVS 5.0.0 V3](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x12-V3-Web-Frontend-Security.md)

- **13.2.1 (L2):** backend component communication uses individual service accounts, short-term tokens or certificates, "not unchanging credentials such as passwords, API keys, or shared accounts".
- **13.2.2 (L2):** backend components use least-privileged accounts; **13.3.2 (L2):** secret access follows least privilege.
- **3.5.4 (L2):** separate applications on different hostnames for same-origin isolation (supports a separate operator console host).

**Inferences:**

- If Tetsu targets L2 overall, 8.4.1 and V16 are mandatory; adopting 8.4.2, 6.3.3-L3 and 7.5.3 for the operator console only is a reasonable "L3 for privileged paths" posture.
- 8.3.3 and 6.3.4 argue that impersonation must be a documented, separately authorized pathway, checked against the operator's grant.

**Gaps:**

- V13 lines were grepped from an initial download whose tag I did not confirm (the V8/V6/V7/V16 text was re-fetched from `v5.0.0_release`). V13 and V3 IDs/wording should be re-checked against the release tag.
- ASVS 5.0.0 has no requirement I found that explicitly mandates customer consent or customer-visible operator logs; that requirement comes from Tetsu's own invariants, not ASVS.
