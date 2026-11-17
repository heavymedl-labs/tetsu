# Account recovery, MFA lifecycle, breached-credential defenses, and identity incidents for a multi-tenant booking SaaS (ASVS 5.0.0 L2)

Research date: 2026-11-11. Sources are linked inline. Where only secondary coverage was found, that is flagged.

## What ASVS 5.0.0 V6.3, V6.4 (and adjacent V6.2, V6.5, V6.6) require exactly

### Takeaway

At L2, ASVS 5.0.0 requires MFA (or a combination of single-factor mechanisms) for every account (6.3.3), a password reset that does not bypass enabled MFA (6.4.3), and lost-factor recovery backed by identity evidence at the same level used at enrollment (6.4.4). Notification of credential changes (6.3.7), user enumeration resistance (6.3.8), and admin-initiated reset (6.4.6) are L3, not L2.

### Cited Findings

All quotes are verbatim from `5.0/en/0x15-V6-Authentication.md` at tag `v5.0.0` — [OWASP ASVS v5.0.0 source](https://github.com/OWASP/ASVS/blob/v5.0.0/5.0/en/0x15-V6-Authentication.md) (fetched raw from raw.githubusercontent.com).

V6.3 General Authentication Security:

- **6.3.1 (L1)** "Verify that controls to prevent attacks such as credential stuffing and password brute force are implemented according to the application's security documentation."
- **6.3.2 (L1)** "Verify that default user accounts (e.g., "root", "admin", or "sa") are not present in the application or are disabled."
- **6.3.3 (L2)** "Verify that either a multi-factor authentication mechanism or a combination of single-factor authentication mechanisms, must be used in order to access the application. For L3, one of the factors must be a hardware-based authentication mechanism which provides compromise and impersonation resistance against phishing attacks while verifying the intent to authenticate by requiring a user-initiated action (such as a button press on a FIDO hardware key or a mobile phone). Relaxing any of the considerations in this requirement requires a fully documented rationale and a comprehensive set of mitigating controls."
- **6.3.4 (L2)** "Verify that, if the application includes multiple authentication pathways, there are no undocumented pathways and that security controls and authentication strength are enforced consistently."
- **6.3.5 (L3)** "Verify that users are notified of suspicious authentication attempts (successful or unsuccessful). This may include authentication attempts from an unusual location or client, partially successful authentication (only one of multiple factors), an authentication attempt after a long period of inactivity or a successful authentication after several unsuccessful attempts."
- **6.3.6 (L3)** "Verify that email is not used as either a single-factor or multi-factor authentication mechanism."
- **6.3.7 (L3)** "Verify that users are notified after updates to authentication details, such as credential resets or modification of the username or email address."
- **6.3.8 (L3)** "Verify that valid users cannot be deduced from failed authentication challenges, such as by basing on error messages, HTTP response codes, or different response times. Registration and forgot password functionality must also have this protection."

V6.4 Authentication Factor Lifecycle and Recovery:

- **6.4.1 (L1)** "Verify that system generated initial passwords or activation codes are securely randomly generated, follow the existing password policy, and expire after a short period of time or after they are initially used. These initial secrets must not be permitted to become the long term password."
- **6.4.2 (L1)** "Verify that password hints or knowledge-based authentication (so-called "secret questions") are not present."
- **6.4.3 (L2)** "Verify that a secure process for resetting a forgotten password is implemented, that does not bypass any enabled multi-factor authentication mechanisms."
- **6.4.4 (L2)** "Verify that if a multi-factor authentication factor is lost, evidence of identity proofing is performed at the same level as during enrollment."
- **6.4.5 (L3)** "Verify that renewal instructions for authentication mechanisms which expire are sent with enough time to be carried out before the old authentication mechanism expires, configuring automated reminders if necessary."
- **6.4.6 (L3)** "Verify that administrative users can initiate the password reset process for the user, but that this does not allow them to change or choose the user's password. This prevents a situation where they know the user's password."
- The V6.4 section intro points to NIST SP 800-63**-3** §5.1.1.2 and §6.1.2.3, not 800-63-4 — [ASVS v5.0.0 V6](https://github.com/OWASP/ASVS/blob/v5.0.0/5.0/en/0x15-V6-Authentication.md).

Adjacent L2 requirements that shape recovery and MFA:

- **6.2.12 (L2)** "Verify that passwords submitted during account registration or password changes are checked against a set of breached passwords." **6.2.4 (L1)** requires a check against "at least, the top 3000 passwords". **6.2.10 (L2)** forbids periodic rotation; passwords stay valid "until it is discovered to be compromised or the user rotates it." — [ASVS v5.0.0 V6](https://github.com/OWASP/ASVS/blob/v5.0.0/5.0/en/0x15-V6-Authentication.md)
- **6.5.1 (L2)** lookup secrets (recovery codes), OOB codes, and TOTPs "are only successfully usable once." **6.5.2 (L2)** lookup secrets under 112 bits "(19 random alphanumeric characters or 34 random digits)" must be hashed with an approved password hash with a 32-bit salt; at 112 bits or more a standard hash is allowed. **6.5.3 (L2)** CSPRNG generation. **6.5.4 (L2)** minimum 20 bits of entropy ("4 random alphanumeric characters or 6 random digits"). **6.5.5 (L2)** OOB requests max lifetime 10 minutes, TOTP 30 seconds. **6.5.6 (L3)** any factor can be revoked on loss. — [ASVS v5.0.0 V6](https://github.com/OWASP/ASVS/blob/v5.0.0/5.0/en/0x15-V6-Authentication.md)
- **6.6.1 (L2)** SMS/phone OTP only when "the phone number has previously been validated, alternate stronger methods (such as Time based One-time Passwords) are also offered, and the service provides information on their security risks to users." **6.6.2 (L2)** OOB codes bound to the originating request. **6.6.3 (L2)** OOB codes rate limited; "Consider also using a code with at least 64 bits of entropy." **6.6.4 (L3)** push-bombing rate limits / number matching. — [ASVS v5.0.0 V6](https://github.com/OWASP/ASVS/blob/v5.0.0/5.0/en/0x15-V6-Authentication.md)
- **6.8.4 (L2)** when an external IdP is used and strength matters, verify via `acr`/`amr`/`auth_time`, else assume the weakest method — [ASVS v5.0.0 V6](https://github.com/OWASP/ASVS/blob/v5.0.0/5.0/en/0x15-V6-Authentication.md)

### Inferences

- 6.3.3's "combination of single-factor authentication mechanisms" wording leaves room for passkey-only accounts (a passkey is itself multi-factor: device possession plus user verification) and for password + email-link combinations at L2; 6.3.6's ban on email as an authentication factor is only L3. An L2 design can therefore use email for reset and as a second single-factor mechanism, but this is a weaker posture and would not survive an L3 upgrade.
- 6.4.3 + 6.4.4 together mean: an emailed reset link may restore the *password* but must still demand the enrolled second factor; losing the second factor needs a separate path whose evidence matches enrollment. For a booking SaaS that does no identity proofing at signup, "same level as enrollment" plausibly means "what was proven at enrollment": control of the verified email plus something else bound at enrollment (a recovery code, a second passkey, or for staff, a tenant owner's attestation). This is interpretation; ASVS does not define it for unproofed accounts.
- 6.3.4 is the guardrail against support-desk bypasses: a human-assisted recovery path is an "authentication pathway" and must be documented and enforce equivalent strength.
- L3 items (6.3.7 notifications, 6.3.8 enumeration, 6.4.6 admin-initiated reset) are cheap and are exactly what the incidents below punish; worth adopting even at L2.

### Gaps

- ASVS 5.0.0 does not define "identity proofing" levels for consumer apps without IAL; its intro links still point to 800-63-3. No official ASVS guidance found on how 6.4.4 applies to unproofed accounts.

## Recovery designs (email reset, recovery codes, multiple passkeys, admin- and support-assisted, delayed recovery, consumers who lose their only device)

**Takeaway:**

NIST 800-63B-4 now gives a concrete AAL2 recovery recipe (two recovery codes from different channels, or one code plus a remaining single-factor authenticator, with mandatory notification); combined with "bind at least two authenticators" nudges and trusted-contact or delayed recovery, this removes most need for human support resets, which are the most-attacked path.

**Cited Findings:**

NIST SP 800-63B-4 (final, dated 2025-07-31 per [NIST CSRC](https://csrc.nist.gov/pubs/sp/800/63/b/4/final)); quotes from [800-63B-4 HTML](https://pages.nist.gov/800-63-4/sp800-63b.html):

- §4.1: CSPs "SHALL permit the binding of multiple authenticators to a subscriber account"; §4.1.2.1: CSPs "SHOULD encourage subscribers to maintain at least two separate means of authentication." Adding an authenticator requires authentication at the lower of the account's highest available AAL and the new authenticator's AAL, and the CSP "SHALL notify the subscriber via a mechanism independent of the transaction."
- §4.2: CSPs "SHALL support one or more of" four recovery classes: saved recovery codes, issued recovery codes, recovery contacts, repeated identity proofing.
- Saved recovery codes: "SHALL include at least 64 bits from an approved random bit generator"; invalidated after use and replaced, and issuing a replacement triggers a recovery notification.
- Issued recovery codes: at least six decimal digits; validity 10 minutes for SMS/voice, 24 hours for email, 21/30 days for postal. CSPs "SHALL allow the subscriber to establish at least two recovery addresses."
- Recovery contacts: trusted associates may receive issued codes; CSPs SHOULD send an annual reminder to review them.
- AAL2 recovery requires one of: two recovery codes obtained through different methods; one recovery code plus authentication with a remaining bound single-factor authenticator; or repeated identity proofing (if the account was proofed). Unproofed accounts recover with a saved code, an issued code, or a recovery contact.
- "In all cases, account recovery SHALL cause a notification to be sent to the subscriber or their designee"; CSPs "SHALL support at least two notification addresses per subscriber account."
- §3.1.3.1: "Email SHALL NOT be used for out-of-band authentication", but email confirmation codes and issued recovery codes are explicitly "not authentication" and not covered by that ban.
- §3.1.3.3: PSTN (SMS/voice) is a restricted authenticator; changing a phone number is binding a new authenticator; verifiers should consider SIM change, number porting, or device swap signals before sending a code.
- Knowledge-based authentication (security questions) is "no longer recognized as an acceptable authenticator" in SP 800-63-4 — [WorkOS summary of NIST FAQ](https://workos.com/blog/security-questions-nist-800-63b-4-account-recovery) (secondary).

OWASP Forgot Password Cheat Sheet — [OWASP](https://cheatsheetseries.owasp.org/cheatsheets/Forgot_Password_Cheat_Sheet.html):

- "Return a consistent message for both existent and non-existent accounts" and "Ensure that responses return in a consistent amount of time."
- Tokens: CSPRNG, long enough to resist brute force, bound to one user, "Single use and expire after an appropriate period."
- "Don't rely on the Host header while creating the reset URLs" (host header injection); set `Referrer-Policy: no-referrer` on the reset page.
- "Do not make a change to the account until a valid token is presented"; "Don't automatically log the user in"; send a notification; invalidate existing sessions (ask or do it automatically).
- On suspected compromise, do not trust recently changed recovery details; use previously established evidence such as saved recovery codes, and "invalidate existing sessions and outstanding password reset and recovery links or codes."
- "Accounts should not be locked out in response to a forgotten password attack."

Delayed / social recovery at consumer scale:

- Google Recovery Contacts (Oct 2025): up to 10 contacts; a person can be contact for up to 25 accounts; invite valid 7 days; intended for forgotten password, lost passkey device, or compromised account; not for child accounts — [Google blog](https://blog.google/innovation-and-ai/technology/safety-security/recovery-contacts-verify-google-account/); [9to5Google](https://9to5google.com/2025/10/15/google-account-recovery-contacts/).
- Apple Account Recovery: up to five recovery contacts; the waiting period is unpublished ("several days or longer"); a recovery request is cancelled automatically if the account is in use on a logged-in device; enabling a Recovery Key disables standard recovery and can cause permanent lockout — [The Register](https://www.theregister.com/2025/10/16/google_gmail_trusted_contacts/) and [Techlicious](https://www.techlicious.com/blog/google-recovery-contact-can-save-your-account/) (secondary; verify on Apple support pages).

Recovery at a developer platform scale (GitHub):

- After mandatory 2FA, GitHub saw a 54% reduction in recovery tickets requiring significant human intervention, and over 75% of recovery tickets came through the in-product workflow, attributed to collecting recovery details in-product and screening for risk factors — [GitHub blog, 2024-04-24](https://github.blog/security/supply-chain-security/securing-millions-of-developers-through-2fa/).

Support-assisted recovery risk (details in the incidents section):

- CISA/FBI AA23-320A: Scattered Spider calls help desks posing as employees to get staff to "reset the employee's password, and transfer the employee's MFA to a device they control," often over several calls where early calls learn the reset procedure; CISA recommends phishing-resistant MFA and strict identity verification before resetting credentials or MFA — [TechTarget on AA23-320A](https://www.techtarget.com/cybersecurity/news/366559974/CISA-FBI-issue-alert-for-ongoing-Scattered-Spider-activity) (secondary; primary is CISA AA23-320A, updated July 2025).
- Coinbase (May 2025): criminals bribed outsourced support agents for customer data (including ID images) to run impersonation scams; under 1% of monthly transacting users; estimated cost $180M–$400M; Coinbase refused a $20M ransom — [eSecurity Planet](https://www.esecurityplanet.com/news/coinbase-rejects-ransom-data-leak/); [Fortune on TaskUs](https://fortune.com/crypto/2025/05/29/coinbase-hack-the-community-taskus-bpos-teenagers).

**Inferences:**

- For staff (tenant members), the natural "recovery contact" is the tenant owner: an owner can *initiate* a staff MFA reset (ASVS 6.4.6 pattern: admin starts the process but never chooses the secret), with the staff member then enrolling a new factor via a fresh, short-lived link to their verified email, plus notification to all addresses. The owner should never see or set the credential. Owner-of-last-resort recovery remains the hard case and needs a separate, slow, notified process.
- For consumers who lose their only device: the safest low-friction path is (1) encourage synced passkeys (platform sync means "losing the device" rarely loses the credential), (2) offer saved recovery codes at enrollment, and (3) for those with neither, a delayed recovery: email-issued code + waiting period (e.g. days) + notification to all addresses + automatic cancellation if the account logs in normally (Apple's model). This approximates NIST AAL2's "two codes via different methods" only if a second channel exists; with email only, it is single-channel and should be documented as a compensating control under 6.3.3's "documented rationale" clause.
- Booking consumers have low-value accounts compared with staff; risk-tiering (staff: strict, owner-mediated; consumers: delayed self-service) is defensible, but ASVS 6.3.3 still requires MFA-equivalent access for both at L2.
- Support staff should have no tool that removes or rebinds a factor without the account holder's own action; Coinbase shows insiders and outsourced agents are a threat, and Okta and Mailchimp (below) show support tools are high-value targets.

**Gaps:**

- No primary Apple documentation on the exact waiting period was retrieved; Apple does not publish one.
- No published data found on recovery-code retention rates (how many users still have codes when needed). GitHub's 28-day checkup (below) is the closest proxy.
- Did not verify whether Google publishes its own account-recovery waiting period for passkey-only accounts.

## MFA enrollment UX (enforce at signup vs grace periods, passkey nudges, GitHub and Google rollout data)

**Takeaway:**

The best published data (GitHub) shows a staged mandatory rollout with good UX hit ~95% opt-in, raised multi-factor redundancy, and cut 2FA support tickets by a third; a post-enrollment checkup at 28 days catches broken setups. Google's auto-enrollment claimed 50% fewer compromises but the comparison was not like-for-like.

**Cited Findings:**

- GitHub announced in May 2022 that all code contributors would need 2FA by end of 2023; rollout began March 2023, staged by user group in order of impact of their privileges — [GitHub blog](https://github.blog/security/supply-chain-security/securing-millions-of-developers-through-2fa/). It started with maintainers of the top 100 npm packages in Feb 2023 — [The Register](https://www.theregister.com/2023/08/10/github_multifactor_authentication/).
- Results: "opt-in rate of nearly 95%" among contributors who got the requirement in 2023; 54% increase in 2FA adoption across all active contributors; users 47% more likely to configure two or more 2FA methods; "one-third reduction in 2FA-related support tickets"; nearly 1.4 million passkeys registered since July 2023 beta, overtaking other WebAuthn methods by Nov 2023; SMS share as 2FA fell by almost 25% from early 2023 to early 2024 — [GitHub blog](https://github.blog/security/supply-chain-security/securing-millions-of-developers-through-2fa/).
- A verification checkup 28 days after 2FA setup "helped 25% of users successfully reconfigure their accounts" — [GitHub blog](https://github.blog/security/supply-chain-security/securing-millions-of-developers-through-2fa/). (The GitHub post does not state grace-period length.)
- Analyst caveat: effectiveness is hard to measure; GitHub had token leaks in the same period — [The New Stack](https://thenewstack.io/githubs-2fa-push-boosts-adoption-among-developers/).
- Google auto-enrolled over 150 million users in 2SV from Oct 2021 and required it for over 2 million YouTube creators; it reported ~50% fewer compromises among enrolled users. Google later said this compared enrolled vs unenrolled users rather than a controlled measurement and gave no absolute numbers; users could generally turn 2SV off — [TechTarget](https://www.techtarget.com/searchsecurity/news/252513218/Google-2-step-verification-led-to-50-fewer-account-hacks); [TechSpot](https://www.techspot.com/news/93323-google-finds-50-percent-reduction-user-account-breaches.html).
- NIST: CSPs "SHOULD encourage subscribers to maintain at least two separate means of authentication" — [NIST 800-63B-4](https://pages.nist.gov/800-63-4/sp800-63b.html).
- Roku made 2FA mandatory for all accounts (reported as ~80M users) only after two credential-stuffing incidents in 2024 — [SC Media](https://scmagazine.com/news/roku-activates-2fa-for-80m-users-after-breach-of-576k-accounts).

**Inferences:**

- For a new product (no legacy users), enforcing MFA at signup avoids a later migration; GitHub's numbers suggest the cost is acceptable if enrollment is well designed and support tooling for recovery exists first.
- Prompting a second factor (a second passkey or recovery codes) immediately and again at a checkup (GitHub: 28 days) directly reduces future recovery load.
- A passkey-first signup (passkey satisfies 6.3.3 on its own) sidesteps the "password + separate second factor" enrollment friction for consumers; nudging passkeys after password login is the migration path for anyone who started with a password.

**Gaps:**

- No public data found on conversion or drop-off effects of mandatory MFA at consumer signup for booking/e-commerce. GitHub's audience is developers, which may not generalize to consumers booking appointments.
- GitHub's grace-period length (reported elsewhere as 45 days) was not confirmed from the primary post; treat as unverified.

## Credential stuffing defenses (rate limiting, bot detection without JavaScript, breached-password checks, login notifications, session revocation, passwordless)

**Takeaway:**

MFA is "by far the best defense"; everything else is layered friction. Breached-password checks via HIBP's k-anonymity range API satisfy ASVS 6.2.12 without sending the password. Non-JS defenses that work are multi-window rate limits, connection (TLS) fingerprinting, proof-of-work/latency degradation, and notifying users when a correct password fails MFA.

**Cited Findings:**

OWASP Credential Stuffing Prevention Cheat Sheet — [OWASP](https://cheatsheetseries.owasp.org/cheatsheets/Credential_Stuffing_Prevention_Cheat_Sheet.html):

- MFA is "by far the best defense against the majority of password-related attacks"; can be applied conditionally (new device, unusual location, anonymizer) or as step-up.
- IP blocking "should not be used as the sole or primary defense"; rate limit across multiple time windows and "not rely on a single predictable volume limit"; mitigations "should be temporary"; residential proxy networks "may defeat both IP block-lists and rate limiting."
- Connection fingerprinting (e.g. JA3) "may provide more accurate results than other defenses" and can expose scripts claiming to be mobile browsers.
- Requiring JavaScript / blocking headless browsers works but "will reduce the accessibility of the website."
- Degradation (proof-of-work, added latency) constrains request rate "without resorting to user tracking or profiling."
- Multi-step login or requiring a CSRF token first "doubles the number of requests that the attacker must make."
- Leaked password checks: "A well known free service for this is Pwned Passwords"; can self-host or use the API.
- Notifications: generally do not notify on a simple wrong password, but do notify when a correct password fails MFA; show recent login history and let users end sessions.

HIBP Pwned Passwords range API:

- Client SHA-1-hashes locally and sends only the first 5 hex characters; server returns all suffixes for that prefix; client matches locally; no authorization needed — [HIBP API docs](https://haveibeenpwned.com/API/v3).
- `Add-Padding: true` header pads responses with random-count zero-occurrence entries to hide response size — [Troy Hunt](https://www.troyhunt.com/enhancing-pwned-passwords-privacy-with-padding/).
- The service still sees the prefix and requester IP; the anonymity set is the suffix bucket (example: ~1,977 suffixes for prefix 5BAA6) — [orthogonal.info](https://orthogonal.info/check-breached-password-without-sending-hibp-k-anonymity/) (secondary). The dataset can be downloaded and self-hosted — [OWASP](https://cheatsheetseries.owasp.org/cheatsheets/Credential_Stuffing_Prevention_Cheat_Sheet.html).

Incident evidence on what stuffing exploits:

- Midnight Blizzard deliberately kept password-spray attempts low to evade volume-based blocks — [BleepingComputer](https://www.bleepingcomputer.com/news/security/microsoft-reveals-how-hackers-breached-its-exchange-online-accounts/amp/).
- Snowflake: at least 79.7% of accounts used had prior credential exposure, mostly infostealer logs dating back to 2020 — [SecurityWeek](https://www.securityweek.com/snowflake-attacks-mandiant-links-data-breaches-to-infostealer-infections/).

**Inferences:**

- Because Tetsu-style server-rendered pages may avoid JavaScript, the no-JS toolbox is: per-account and per-IP limits over several windows, TLS/HTTP fingerprinting at the edge, proof-of-work or added latency after failures, CSRF-token-first multi-step login, and MFA itself. Low-and-slow spraying (Midnight Blizzard) defeats volume limits, so MFA everywhere is the real control.
- Infostealer-sourced credentials are valid and fresh-looking, so breached-password checks at registration don't stop them; only MFA or passkeys do. Passkeys remove the reusable secret entirely.
- Session revocation on password reset and on factor change (OWASP) is needed because stolen session cookies/tokens bypass MFA (Okta HAR files below).
- Periodic re-checking of stored password hashes against new breach corpuses is not required by ASVS 6.2.12 (registration and change only) but ASVS 6.2.10 allows forcing a change "until it is discovered to be compromised."

**Gaps:**

- No primary benchmark found for false-positive rates of JA3/JA4 fingerprinting for small SaaS.
- HIBP's own padding documentation page was not retrieved directly; padding details come from Troy Hunt's post.

## Notable incidents and their identity lessons

**Takeaway:**

Nearly every incident maps to one of: no MFA on some account (Snowflake, Microsoft test tenant, 23andMe, Roku), MFA that is phishable or fatigable (Uber), recovery or support paths that bypass MFA (MGM/Caesars, Mailchimp, Coinbase), or support tooling that leaks session tokens (Okta), or unauthenticated endpoints that enumerate identities (Authy).

**Cited Findings:**

- **23andMe (2023)**: credential stuffing with reused passwords hit ~14,000 accounts (<0.1% of ~14M customers); through the DNA Relatives sharing feature this exposed ~5.5M DNA Relatives profiles plus ~1.4M Family Tree profiles, ~6.9M people total; confirmed in an SEC filing on 2023-12-01 — [Axios](https://www.axios.com/2023/12/04/23andme-customers-stolen-data); [SC Media](https://scmagazine.com/news/23andme-confirms-nearly-7-million-customers-affected-in-data-leak). Lesson: in a multi-tenant or shared-data product, one account's compromise exposes everything that account can see; mandatory MFA and limiting cross-account visibility bound the blast radius.
- **Snowflake / UNC5537 (2024)**: stolen infostealer credentials used against 100+ (later ~165 notified) customer instances with no MFA, some credentials unrotated ~4 years, no network allow-lists; Mandiant "assesses MFA would have prevented compromise" and saw no MFA bypass; MFA was per-user opt-in — [Dark Reading](https://darkreading.com/cloud-security/snowflake-cloud-accounts-rampant-credential-issues); [SecurityWeek](https://www.securityweek.com/snowflake-attacks-mandiant-links-data-breaches-to-infostealer-infections/); [Decipher](https://duo.com/decipher/mandiant-165-snowflake-customers-potentially-exposed-in-wider-campaign). Lesson: a platform that leaves MFA as tenant opt-in will be blamed for tenant breaches; enforce MFA platform-wide and make it impossible for a tenant to disable.
- **Roku (2024)**: ~15,000 accounts then ~576,000 more via credential stuffing; fraudulent purchases in fewer than 400 cases using stored payment methods; Roku then reset passwords and enabled 2FA for all accounts — [Security Affairs](https://securityaffairs.com/161765/data-breach/roku-second-data-breach.html); [SC Media](https://scmagazine.com/news/roku-activates-2fa-for-80m-users-after-breach-of-576k-accounts). Lesson: consumer accounts with stored payment methods are monetizable; step-up auth on purchase and MFA from day one avoids a reactive rollout.
- **Okta support system (Oct 2023)**: attacker used a service account stored in the support case system, whose credentials an employee had saved to a personal Google profile on a managed laptop; accessed files for 134 customers (Sep 28–Oct 17), some HAR files containing session tokens, used to hijack five customer sessions — [Okta root cause](https://sec.okta.com/articles/2023/11/unauthorized-access-oktas-support-case-management-system-root-cause/); [BleepingComputer](https://www.bleepingcomputer.com/news/security/okta-breach-134-customers-exposed-in-october-support-system-hack/). Lessons: support tooling is in scope for authentication rigor (service accounts with MFA or no passwords); never ask customers for artifacts containing live session tokens; bind sessions so replayed tokens fail.
- **MGM Resorts / Caesars (Sep 2023)**: a ~10-minute vishing call to the help desk, impersonating an employee, reportedly led to a password/MFA reset on a highly privileged (Okta super admin) account; MGM's disruption cost ~$100M; Caesars reportedly paid ~$15M — [Kolide](https://www.kolide.com/blog/what-everyone-got-wrong-about-the-mgm-hack); [Built In](https://builtin.com/articles/lessons-from-mgm-caesars-cyberattacks). Exact request made to the help desk is not public. Lesson: human-assisted MFA reset is the weakest link; resets for privileged accounts need out-of-band verification through an already-bound channel or a second approver.
- **Scattered Spider generally (CISA AA23-320A, updated July 2025)**: SIM swaps, help-desk impersonation over multiple calls, MFA transfer to attacker devices — [TechTarget](https://www.techtarget.com/cybersecurity/news/366559974/CISA-FBI-issue-alert-for-ongoing-Scattered-Spider-activity); [SC World](https://www.scworld.com/news/scattered-spiders-latest-ttps-added-to-updated-cisa-joint-advisory). Lesson: don't let a phone number or a support call substitute for a bound factor.
- **Twilio / Authy (Jul 2024)**: an unauthenticated API endpoint let attackers check lists of phone numbers and get Authy account data; ~33.4M rows leaked (account ID, phone number, status, device count); Twilio then required auth on the endpoint — [Malwarebytes](https://www.malwarebytes.com/blog/news/2024/07/authy-phone-numbers-accessed-by-cybercriminals-warns-twilio); [heise](https://heise.de/en/news/MFA-app-Authy-Countless-phone-numbers-tapped-via-unsecured-API-9789971.html). Lesson: enumeration endpoints (signup, "is this number/email registered?", reset) leak identity data at scale; apply ASVS 6.3.8 and rate limits even at L2.
- **Mailchimp (Jan 2023)**: phished employee/contractor credentials gave access to an internal support and account-administration tool; 133 customer accounts accessed; detected 2023-01-11; affected accounts suspended and contacts notified within 24 hours. Also prior incidents in Apr 2022 (~300–319 accounts) and Aug 2022 (~214–216, 0ktapus) — [The Record](https://therecord.media/more-than-100-mailchimp-accounts-accessed-via-social-engineering-cyberattack/); [The Hacker News](https://thehackernews.com/2023/01/mailchimp-suffers-another-security.html). Lesson: admin/support consoles that can act on any tenant must use phishing-resistant MFA and least privilege; they are a cross-tenant breach path.
- **Uber (Sep 2022)**: contractor password likely from infostealer malware; attacker spammed MFA push prompts and messaged on WhatsApp posing as Uber IT, saying accepting would stop them; once in, found PowerShell scripts with admin credentials — [InfoQ](https://www.infoq.com/news/2022/09/Uber-breach-mfa-fatigue); [KPMG](https://assets.kpmg.com/content/dam/kpmg/in/pdf/2022/09/27-september-2022-lessons-to-learn-from-the-uber-security-breach.pdf). Lesson: push MFA without number matching or rate limits is fatigable (ASVS 6.6.4); phishing-resistant passkeys avoid it.
- **Microsoft / Midnight Blizzard (Nov 2023–Jan 2024)**: low-volume password spray compromised a legacy non-production test tenant account without MFA, then a legacy test OAuth app with elevated access to corporate tenant, reaching senior leadership email — [BleepingComputer](https://www.bleepingcomputer.com/news/security/microsoft-reveals-how-hackers-breached-its-exchange-online-accounts/amp/); [The Register](https://www.theregister.com/2024/01/27/microsoft_cozy_bear_mfa). Lesson: MFA must cover every account, including test, demo, seed, and service accounts (ASVS 6.3.2, 6.3.4); demo tenants must not hold privileges in production.
- **Coinbase (May 2025)**: bribed outsourced support agents leaked customer data used for impersonation scams; $180M–$400M estimated cost — [eSecurity Planet](https://www.esecurityplanet.com/news/coinbase-rejects-ransom-data-leak/). Lesson: support staff access to identity data and account controls is an insider-threat surface; minimize what support can see and do.

**Inferences:**

- For a multi-tenant booking SaaS, the highest-risk accounts are platform operator/support accounts (Mailchimp, Okta) and tenant owners (MGM pattern); consumer accounts are the stuffing target (23andMe, Roku).
- The demo deployment should hold no accounts with real privileges and must also require MFA if it has login (Microsoft lesson).

**Gaps:**

- MGM/Caesars specifics (exact reset performed, ransom paid) are from secondary press and contested; neither company published a technical post-mortem.
- Midnight Blizzard's exact permissions on the test OAuth app were never published by Microsoft.
- The original Authy phone-number source is unknown; 33M count comes from the attacker's dump.

## Email deliverability for codes/links, and SMS risks

**Takeaway:**

Reset and recovery mail must be authenticated with SPF, DKIM, and an aligned DMARC record, or major inboxes will reject or junk it; Gmail treats 5,000+/day as bulk. SMS adds SIM-swap risk (Scattered Spider) and toll-fraud cost (SMS pumping), is a NIST "restricted" authenticator, and under ASVS 6.6.1 needs a stronger alternative offered alongside.

**Cited Findings:**

- Gmail/Yahoo 2024 sender rules: Gmail bulk threshold is 5,000+ messages/day to Gmail addresses; bulk senders need SPF, DKIM, and DMARC (p=none accepted) with From: alignment; spam complaint rate must stay below 0.3%; one-click unsubscribe exempts transactional mail. Sources disagree on whether all senders need both SPF and DKIM or just one — [Mailtrap](https://mailtrap.io/blog/gmail-yahoo-email-sender-requirements/); [OneSignal](https://documentation.onesignal.com/docs/new-google-and-yahoo-email-updates); [Bird](https://bird.com/docs/knowledge-base/deliverability/gmail-yahoo-requirements) (all secondary; verify against Google's "Email sender guidelines").
- Microsoft Outlook began enforcing similar DMARC requirements for high-volume senders in May 2025 — [Courier](https://www.courier.com/blog/email-sender-requirements) (secondary).
- NIST 800-63B-4: email codes are acceptable for recovery (24-hour validity for issued codes) but not as an out-of-band authenticator — [NIST 800-63B-4](https://pages.nist.gov/800-63-4/sp800-63b.html).
- SIM swap is a standard Scattered Spider technique ahead of help-desk calls — [TechTarget on AA23-320A](https://www.techtarget.com/cybersecurity/news/366559974/CISA-FBI-issue-alert-for-ongoing-Scattered-Spider-activity).
- SMS pumping: fraudsters trigger OTP SMS to premium or colluding number ranges. Twitter's (X's) losses (Musk cited ~$60M/year, unverified) were a reported factor in restricting SMS 2FA to paid users — [Twilio blog](https://www.twilio.com/en-us/blog/sms-pumping-fraud-solutions). Twilio claims Verify Fraud Guard blocked 569M fraud attempts and saved $62.7M (Jun 2022–Oct 2024) — [Twilio](https://www.twilio.com/en-us/blog/twilio-verify-fraud-guard-a-powerful-defense-against-sms-pumping-fraud) (vendor claims, unaudited).
- ASVS 6.6.1 (L2): SMS only with a pre-validated number, a stronger alternative offered, and risk disclosure; disallowed at L3 — [ASVS v5.0.0 V6](https://github.com/OWASP/ASVS/blob/v5.0.0/5.0/en/0x15-V6-Authentication.md).
- GitHub's SMS 2FA share fell ~25% as passkeys rose — [GitHub blog](https://github.blog/security/supply-chain-security/securing-millions-of-developers-through-2fa/).

**Inferences:**

- Skipping SMS entirely removes SIM-swap, pumping, and per-message cost, and simplifies ASVS 6.6.x compliance; email + passkeys + TOTP + recovery codes covers the need.
- Use a dedicated transactional sending subdomain with its own DKIM key and aligned DMARC so marketing reputation (if any) never affects reset mail; keep reset/recovery mail free of tracking links to avoid link rewriting that breaks single-use tokens.

**Gaps:**

- Did not research or compare Postmark, Amazon SES, and Resend (pricing, transactional/marketing stream separation, deliverability data). No independent deliverability benchmarks found; this needs a separate evaluation under ADR-0010.
- Google's own sender-guidelines page was not fetched; the details above rely on vendor summaries that disagree on minor points.
