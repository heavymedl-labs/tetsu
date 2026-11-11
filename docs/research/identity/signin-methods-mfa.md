# Sign-in methods, UX and security trade-offs, and ASVS 5.0.0 multi-factor fit

Scope: server-rendered multi-tenant web app targeting ASVS 5.0.0 Level 2, staff and consumer audiences. Research date 2026-11-11.

Source note: ASVS text below was pulled from the `v5.0.0_release` tag of OWASP/ASVS (`5.0/en/`). V6 and V7 on `master` are byte-identical to the tag; V10 on `master` differs, so V10 quotes are from the tag. Links point at the tag.

ASVS file URLs used:

- V6: <https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x15-V6-Authentication.md>
- V7: <https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x16-V7-Session-Management.md>
- V10: <https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x19-V10-OAuth-and-OIDC.md>
- V11 / Appendix C: <https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x20-V11-Cryptography.md>, <https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x92-Appendix-C_Cryptography.md>
- Glossary: <https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x90-Appendix-A_Glossary.md>

## What ASVS 5.0.0 V6/V7 requires, and which method each requirement touches

### Takeaway

At L2, 6.3.3 requires MFA or a combination of single factors "to access the application", for every pathway (6.3.4), and reset must not bypass it (6.4.3). ASVS defines factors in NIST terms (know/have/are) and points to NIST SP 800-63B, so a method's factor status is read through NIST: email is not a factor, SMS is a restricted factor, and TOTP, lookup codes and WebAuthn are "something you have".

### Cited Findings

**Level framing:**

- V6.3 intro: "L2 applications must force the use of multi-factor authentication (MFA). L3 applications must use hardware-based authentication, performed in an attested and trusted execution environment (TEE). This could include device-bound passkeys, eIDAS Level of Assurance (LoA) High enforced authenticators, authenticators with NIST Authenticator Assurance Level 3 (AAL3) assurance, or an equivalent mechanism." — [ASVS V6](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x15-V6-Authentication.md)
- V6.3 intro: "any attempt to relax these requirements should be accompanied by a clear plan on how the risks around authentication will be mitigated". Also: "at the time of release, NIST SP 800-63 considers email as not acceptable as an authentication mechanism." — [ASVS V6](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x15-V6-Authentication.md)
- V6.2 intro: "From L2 onwards, multi-factor authentication mechanisms are required, where passwords may be one of those factors." — [ASVS V6](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x15-V6-Authentication.md)
- Glossary: "Multi-factor authentication (MFA) – Authentication which includes two or more of the single factors." "Single-factor authenticator – ... something you know (memorized secrets, passwords, passphrases, PINs), something you are (biometrics, fingerprint, face scans), or something you have (OTP tokens, a cryptographic device such as a smart card)." — [ASVS Glossary](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x90-Appendix-A_Glossary.md)

**V6.1 documentation (all methods):**

- 6.1.1 (L1): "Verify that application documentation defines how controls such as rate limiting, anti-automation, and adaptive response, are used to defend against attacks such as credential stuffing and password brute force. The documentation must make clear how these controls are configured and prevent malicious account lockout." — [ASVS V6](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x15-V6-Authentication.md)
- 6.1.2 (L2): a documented "list of context-specific words ... permutations of organization names, product names, system identifiers, project codenames, department or role names". — same
- 6.1.3 (L2): "if the application includes multiple authentication pathways, these are all documented together with the security controls and authentication strength which must be consistently enforced across them." — same

**V6.3 general (all methods):**

- 6.3.1 (L1): "Verify that controls to prevent attacks such as credential stuffing and password brute force are implemented according to the application's security documentation." — same
- 6.3.2 (L1): default accounts ("root", "admin", "sa") absent or disabled. — same
- 6.3.3 (L2): "Verify that either a multi-factor authentication mechanism or a combination of single-factor authentication mechanisms, must be used in order to access the application. For L3, one of the factors must be a hardware-based authentication mechanism which provides compromise and impersonation resistance against phishing attacks while verifying the intent to authenticate by requiring a user-initiated action (such as a button press on a FIDO hardware key or a mobile phone). Relaxing any of the considerations in this requirement requires a fully documented rationale and a comprehensive set of mitigating controls." — same
- 6.3.4 (L2): "if the application includes multiple authentication pathways, there are no undocumented pathways and that security controls and authentication strength are enforced consistently." — same
- 6.3.5 (L3): notify users of suspicious authentication attempts, including "partially successful authentication (only one of multiple factors)". — same
- 6.3.6 (L3): "Verify that email is not used as either a single-factor or multi-factor authentication mechanism." — same
- 6.3.7 (L3): notify users "after updates to authentication details, such as credential resets or modification of the username or email address." — same
- 6.3.8 (L3): "valid users cannot be deduced from failed authentication challenges, such as by basing on error messages, HTTP response codes, or different response times. Registration and forgot password functionality must also have this protection." — same

**V6.2 passwords:**

- 6.2.1 (L1) "at least 8 characters in length although a minimum of 15 characters is strongly recommended"; 6.2.2 users can change password; 6.2.3 change requires current and new password; 6.2.4 check against "at least, the top 3000 passwords which match the application's password policy"; 6.2.5 no composition rules; 6.2.6 type=password with optional reveal; 6.2.7 "'paste' functionality, browser password helpers, and external password managers are permitted"; 6.2.8 verify "exactly as received ... without any modifications such as truncation or case transformation". — [ASVS V6](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x15-V6-Authentication.md)
- L2 additions: 6.2.9 "passwords of at least 64 characters are permitted"; 6.2.10 no periodic rotation, "stays valid until it is discovered to be compromised or the user rotates it"; 6.2.11 context-word list enforced; 6.2.12 "Verify that passwords submitted during account registration or password changes are checked against a set of breached passwords." — same
- 11.4.2 (L2): "passwords are stored using an approved, computationally intensive, key derivation function (also known as a 'password hashing function'), with parameter settings configured based on current guidance." — [ASVS V11](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x20-V11-Cryptography.md)
- Appendix C approved password hashes: argon2id (RFC 9106) "t = 1: m ≥ 47104 (46 MiB), p = 1"; "t = 2: m ≥ 19456 (19 MiB), p = 1"; "t ≥ 3: m ≥ 12288 (12 MiB), p = 1"; scrypt p=1 N ≥ 2^17, r=8; bcrypt "cost ≥ 10"; PBKDF2-HMAC-SHA-512 "iterations ≥ 210,000"; PBKDF2-HMAC-SHA-256 "iterations ≥ 600,000"; PBKDF2-HMAC-SHA-1 legacy. — [ASVS Appendix C](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x92-Appendix-C_Cryptography.md)

**V6.4 lifecycle and recovery:**

- 6.4.1 (L1): initial passwords or activation codes "securely randomly generated ... expire after a short period of time or after they are initially used ... must not be permitted to become the long term password." 6.4.2 (L1): no password hints or secret questions. — [ASVS V6](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x15-V6-Authentication.md)
- 6.4.3 (L2): "Verify that a secure process for resetting a forgotten password is implemented, that does not bypass any enabled multi-factor authentication mechanisms." — same
- 6.4.4 (L2): "Verify that if a multi-factor authentication factor is lost, evidence of identity proofing is performed at the same level as during enrollment." — same
- 6.4.6 (L3): administrators can start a reset but cannot "change or choose the user's password". — same

**V6.5 general MFA (lookup codes, TOTP, out-of-band):**

- V6.5 intro: lookup secrets and TOTP "considered 'something you have'". — same
- 6.5.1 (L2) "only successfully usable once"; 6.5.2 (L2) lookup secrets under 112 bits "hashed with an approved password storage hashing algorithm that incorporates a 32-bit random salt"; 6.5.3 (L2) CSPRNG for lookup secrets, OOB codes and TOTP seeds; 6.5.4 (L2) "minimum of 20 bits of entropy (typically 4 random alphanumeric characters or 6 random digits is sufficient)"; 6.5.5 (L2) "Out of band requests must have a maximum lifetime of 10 minutes and for TOTP a maximum lifetime of 30 seconds." — same
- L3: 6.5.6 any factor revocable; 6.5.7 biometrics only as secondary factors; 6.5.8 TOTP checked against a trusted time source. — same

**V6.6 out-of-band:**

- V6.6 intro: "Unsafe out-of-band authentication mechanisms such as e-mail and VOIP are not permitted. PSTN and SMS authentication are currently considered to be 'restricted' authentication mechanisms by NIST and should be deprecated in favor of Time based One-time Passwords (TOTPs), a cryptographic mechanism, or similar." It also flags push bombing. — same
- 6.6.1 (L2): PSTN/SMS OTP "offered only when the phone number has previously been validated, alternate stronger methods (such as Time based One-time Passwords) are also offered, and the service provides information on their security risks to users. For L3 applications, phone and SMS must not be available as options." — same
- 6.6.2 (L2): OOB codes "bound to the original authentication request". 6.6.3 (L2): code-based OOB "protected against brute force attacks by using rate limiting. Consider also using a code with at least 64 bits of entropy." 6.6.4 (L3): push rate limiting, number matching. — same

**V6.7 cryptographic (WebAuthn, smart cards):**

- V6.7 intro: "Cryptographic authentication mechanisms include smart cards or FIDO keys". Only two L3 requirements: 6.7.1 certificates protected from modification; 6.7.2 "challenge nonce is at least 64 bits in length". — same

**V6.8 identity providers:**

- 6.8.1 (L2): users cannot be spoofed across IdPs; "register and identify the user using a combination of the IdP ID (serving as a namespace) and the user's ID in the IdP." — same
- 6.8.2 (L2): always validate signatures on assertions, "rejecting any assertions that are unsigned or have invalid signatures." 6.8.3 (L2): "SAML assertions are uniquely processed and used only once within the validity period". — same
- 6.8.4 (L2): "if an application uses a separate Identity Provider (IdP) and expects specific authentication strength, methods, or recentness for specific functions, the application verifies this using the information returned by the IdP. For example, if OIDC is used, this might be achieved by validating ID Token claims such as 'acr', 'amr', and 'auth_time' (if present). If the IdP does not provide this information, the application must have a documented fallback approach that assumes that the minimum strength authentication mechanism was used (for example, single-factor authentication using username and password)." — same

**V7 sessions (all methods):**

- 7.1.1 (L2) document inactivity timeout and absolute lifetime "and that the documentation includes justification for any deviations from NIST SP 800-63B re-authentication requirements." 7.1.2 (L2) concurrent session policy. 7.1.3 (L2) document federated session coordination. — [ASVS V7](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x16-V7-Session-Management.md)
- 7.2.1–7.2.4 (L1): backend verification; dynamic tokens; reference tokens "at least 128 bits of entropy" from a CSPRNG; "generates a new session token on user authentication, including re-authentication, and terminates the current session token." — same
- 7.3.1/7.3.2 (L2): inactivity timeout and absolute maximum lifetime "according to risk analysis and documented security decisions." — same
- 7.4.1 (L1) termination invalidates backend session; 7.4.2 (L1) "terminates all active sessions when a user account is disabled or deleted (such as an employee leaving the company)"; 7.4.3 (L2) option to terminate other sessions after any factor change, including reset; 7.4.4 (L2) visible logout; 7.4.5 (L2) admins can terminate sessions for a user or all users. — same
- 7.5.1 (L2): "requires full re-authentication before allowing modifications to sensitive account attributes which may affect authentication such as email address, phone number, MFA configuration, or other information used in account recovery." 7.5.2 (L2): users view and terminate sessions after re-authenticating "with at least one factor". 7.5.3 (L3): step-up for highly sensitive operations. — same
- 7.6.1 (L2): RP/IdP session lifetime "requiring re-authentication as necessary such as when the maximum time between IdP authentication events is reached." 7.6.2 (L2): "creation of a session requires either the user's consent or an explicit action". — same
- Cookies, V3.3: 3.3.1 (L1) Secure plus `__Host-`/`__Secure-` prefix; 3.3.2 (L2) SameSite by purpose; 3.3.3 (L2) `__Host-` unless shared; 3.3.4 (L2) HttpOnly for session tokens. — [ASVS V3](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x12-V3-Web-Frontend-Security.md)
- 2.4.1 (L2): anti-automation against "excessive calls to application functions ... quota exhaustion ... overuse of costly resources" (relevant to email/SMS send endpoints). — [ASVS V2](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x11-V2-Validation-and-Business-Logic.md)

### Inferences

- ASVS 6.3.3 is written as "in order to access the application", not "for privileged users". At L2 consumer accounts need MFA too, unless the documented relaxation clause in 6.3.3 is used. That clause is the only route to a single-factor consumer login at L2, and it demands "a fully documented rationale and a comprehensive set of mitigating controls".
- Account enumeration (6.3.8) is L3 only, so it is good practice at L2, not a hard requirement. Rate limiting (6.3.1, 6.1.1) is L1 and applies to every method, including OTP verification (6.6.3).
- ASVS has no passkey-specific L1/L2 requirements beyond 6.3.3; WebAuthn falls under V6.7, whose two requirements are L3. Correctness at L2 comes from 6.3.3 plus general V6.3 rules.

### Gaps

- ASVS 5.0.0 does not say in normative text whether a passkey with user verification alone satisfies 6.3.3. The inference below relies on NIST, which ASVS defers to.
- No ASVS requirement addresses SCIM or provisioning directly; 7.4.2 is the nearest.

## Method-by-method assessment

**Takeaway:**

For ASVS L2: a passkey with user verification (UV) counts as MFA on its own (a NIST multi-factor cryptographic authenticator); a password needs a second factor (TOTP, passkey, or lookup codes; SMS only with 6.6.1 conditions); email OTP and magic links are not factors at all (usable for email verification and as one recovery code); a federated login satisfies MFA only if the IdP asserts it (`amr`/`acr`) or the tenant contractually enforces it, otherwise 6.8.4 forces you to assume single-factor.

**Cited Findings:**

**Passwords (Argon2id):**

- OWASP Password Storage Cheat Sheet Argon2id minimums: "m=47104 (46 MiB), t=1, p=1 (Do not use with Argon2i)", "m=19456 (19 MiB), t=2, p=1 (Do not use with Argon2i)", "m=12288 (12 MiB), t=3, p=1", "m=9216 (9 MiB), t=4, p=1", "m=7168 (7 MiB), t=5, p=1"; bcrypt "maximum length input length of 72 bytes"; pepper "should be stored in 'secrets vaults' or HSMs". — [OWASP Password Storage Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html)
- Discrepancy: Cheat Sheet says PBKDF2-HMAC-SHA512 "220,000 iterations"; ASVS Appendix C says "≥ 210,000". — [Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html); [ASVS Appendix C](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x92-Appendix-C_Cryptography.md). Argon2id values agree.
- NIST 800-63B-4 §3.1.1.2: single-factor passwords "a minimum of 15 characters in length"; passwords "only used as part of multi-factor authentication processes ... SHALL require them to be a minimum of eight characters"; "Verifiers and CSPs SHALL NOT impose other composition rules"; "verifiers SHALL compare the prospective secret against a blocklist"; "the entire password SHALL be subject to comparison". — [NIST SP 800-63B-4](https://pages.nist.gov/800-63-4/sp800-63b.html)
- NIST §3.2.2 rate limiting: limit consecutive failed attempts on one authenticator "to no more than 100"; "The limit of 100 attempts is an upper bound, and agencies MAY impose lower limits." — [NIST SP 800-63B-4](https://pages.nist.gov/800-63-4/sp800-63b.html)
- Breached-password check via HIBP Pwned Passwords: `GET https://api.pwnedpasswords.com/range/{first 5 SHA-1 hex chars}`, prefix not case sensitive, matching done client side; no API key, "no rate limit"; `Add-Padding: true` pads responses to 800–1,000 entries (padding entries have count 0); `?mode=ntlm` option; response is `SUFFIX:COUNT` lines; no attribution required for Pwned Passwords. — [HIBP API v3](https://haveibeenpwned.com/API/v3)
- UX: Microsoft reports 98% success for passkey sign-in attempts vs 32% for passwords, and passkey sign-in "eight times faster" than password plus MFA. — [Microsoft Security blog, Dec 2024](https://microsoft.com/en-us/security/blog/2024/12/12/convincing-a-billion-users-to-love-passkeys-ux-design-insights-from-microsoft-to-boost-adoption-and-security); reported in [Redmond Mag, May 2025](https://redmondmag.com/articles/2025/05/02/microsoft-marks-first-world-passkey-day-with-push-for-passwordless-future.aspx). Microsoft also cites 7,000 password attacks per second. — same
- Google (Mar–Apr 2023 data): password success 13.8% vs local passkey 63.8%; passkey sign-in 14.9 s vs 30.4 s for passwords. Android Police notes Google did not define "success rate". — [Google Security Blog](https://security.googleblog.com/2023/05/making-authentication-faster-than-ever.html); [Android Police](https://www.androidpolice.com/google-passkeys-data-faster-dependable-future/)

**Passkeys / WebAuthn (synced vs device-bound, UV):**

- NIST §2.2.2: "A multi-factor authenticator requires two factors to execute a single authentication event". §3.1.7.1: "Each authentication event SHALL require input and verification of the local activation factor." §3.1.7.3: if a flag "indicates that an activation factor was not used, the authentication SHALL be treated as single-factor." — [NIST SP 800-63B-4](https://pages.nist.gov/800-63-4/sp800-63b.html)
- NIST Appendix B (syncable authenticators, normative): "Verifiers SHALL indicate that UV is preferred and SHALL inspect responses to confirm the value of the UV flag." "If the user is not verified, agencies SHALL treat the authenticator as a single-factor cryptographic authenticator." "Syncable authenticators may be offered as a new option for applications that seek to implement up to AAL2." "User access to authentication keys in the sync fabric SHALL be protected by AAL2-equivalent MFA." "All authentication transactions SHALL perform private-key operations on the local device". — [NIST SP 800-63B-4](https://pages.nist.gov/800-63-4/sp800-63b.html)
- NIST Appendix B attestation: "The unavailability of attestations SHOULD NOT block the use of syncable authenticators for broad public-facing applications." Phishing resistance "Achieved" because keys are "constrained to the domain in which it was created". — same
- NIST §2.3.2: "syncable authenticators SHALL NOT be used at AAL3"; "The cryptographic authenticator used at AAL3 SHALL have a non-exportable private key". — same
- NIST AAL2: "Verifiers SHALL offer at least one phishing-resistant authentication option at AAL2". — same. NIST names WebAuthn as a standard providing "phishing resistance through verifier name binding". NIST 800-63B-4 does not use the word "passkey". — same
- ASVS L3 example list includes "device-bound passkeys" (so synced passkeys are not L3). — [ASVS V6](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x15-V6-Authentication.md)
- FIDO Passkey Index Oct 2026 (15 consumer RPs incl. Amazon, Google, Microsoft, PayPal, Shopify, TikTok, JP Morgan Chase, CVS, WhatsApp; unweighted averages; comparisons with 2025's 9 participants are "directional"): 90% of active accounts eligible; 42% of eligible accounts enrolled (~38% of all active accounts); 36% of successful sign-ins use a passkey (26% in 2025); sign-in success 93% passkeys vs 78% "passwords and traditional MFA such as SMS OTP and authenticator apps" (n=12); 2.8× faster, 27 s saved per sign-in (n=9); 8 of 8 measuring report fewer successful phishing attacks; all four reporting savings saw lower SMS OTP costs; two participants show password/OTP before passkey even to enrolled users, and one estimates passkey-first "would roughly double its passkey sign-in share"; 9 of 13 plan passwordless accounts in 12 months. — [FIDO Passkey Index Oct 2026](https://fidoalliance.org/wp-content/uploads/2026/10/Passkey-Index-October-2026.pdf)
- 2025 Index (9 participants): 93% passkey success vs 63% other methods; 36% of eligible accounts enrolled; 26% of sign-ins. Baseline changed from 63% to 78% in 2026 with a different participant set. — [FIDO 2025 announcement](https://fidoalliance.org/fido-alliance-launches-passkey-index-revealing-significant-passkey-uptake-and-business-benefits/); [FIDO Passkey Index Oct 2025](https://fidoalliance.org/wp-content/uploads/2025/10/FIDO-Passkey-Index-October-2025.pdf)
- Caveat: the 93% is among enrolled users, not all users. — [guptadeepak.com commentary](https://guptadeepak.com/passkeys-success-rate-vs-enrollment-gap/)

**Email one-time codes and magic links:**

- NIST: "Email SHALL NOT be used for out-of-band authentication because it may be vulnerable to" access with only a password, interception, and rerouting (DNS spoofing). The ban does not cover "Confirmation codes that are sent to validate email addresses or are issued as recovery codes". — [NIST SP 800-63B-4](https://pages.nist.gov/800-63-4/sp800-63b.html)
- ASVS: V6.6 intro "e-mail and VOIP are not permitted" as OOB; 6.3.6 (L3) bans email as single or multi-factor. — [ASVS V6](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x15-V6-Authentication.md)
- NIST recovery §4.2: four classes (saved recovery codes, issued recovery codes, recovery contacts, repeated identity proofing). Saved codes "SHALL include at least 64 bits from an approved random bit generator". Issued codes go to recovery addresses and are valid at most "24 hours when sent to an email address". AAL2 recovery needs "Two recovery codes obtained using different methods from the set (i.e., saved, issued, and recovery contacts)" or one recovery code plus a bound single-factor authenticator. "account recovery SHALL cause a notification to be sent to the subscriber or their designee." — [NIST SP 800-63B-4](https://pages.nist.gov/800-63-4/sp800-63b.html)
- Link scanners: mail security scanners (Microsoft Safe Links cited) fetch links before the user, which can consume one-time tokens; reported mitigations include an interstitial "second click" in the browser and scanner detection. IBM reports Microsoft changed Safe Links to avoid triggering one-time token patterns, but scope is unverified. — [IBM community](https://community.ibm.com/community/user/blogs/akshat-rastogi/2025/06/20/safe-links-and-one-time-tokens-in-v10010); [Supabase discussion #41618](https://github.com/orgs/supabase/discussions/41618); [Stytch docs](https://stytch.com/docs/b2b/guides/magic-links/overview); [Trusona](https://www.trusona.com/?p=4800). Evidence is vendor docs and forum posts.

**TOTP:**

- ASVS treats TOTP as "something you have"; 6.5.1 single use; 6.5.3 CSPRNG seed; 6.5.5 "for TOTP a maximum lifetime of 30 seconds". — [ASVS V6](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x15-V6-Authentication.md)
- NIST AAL2: two-factor combinations "SHALL include one physical authenticator" used with a password; single-factor OTP is one of them. — [NIST SP 800-63B-4](https://pages.nist.gov/800-63-4/sp800-63b.html)
- TOTP is not phishing resistant; NIST AAL2 requires offering at least one phishing-resistant option. — same

**SMS / PSTN:**

- NIST §3.1.3.3: "Use of the PSTN for out-of-band verification is restricted"; "verifiers SHALL ensure that alternative authenticator types are available to all subscribers"; "SHOULD consider risk indicators (e.g., device swap, SIM change, number porting". — [NIST SP 800-63B-4](https://pages.nist.gov/800-63-4/sp800-63b.html)
- ASVS 6.6.1 conditions above; banned at L3. — [ASVS V6](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x15-V6-Authentication.md)

**Social OIDC (Google, Apple, Microsoft):**

- ASVS 6.8.4 requires verifying `acr`/`amr`/`auth_time` when strength matters, else a documented fallback assuming single-factor. — [ASVS V6](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x15-V6-Authentication.md)
- Google: "The amr claim is included in the ID token only when information is available on the authentication method used, it may not be present even when requested." Values include `mfa`, `pwd`, `swk`, `hwk`, `sms`, `tel`; `mfa` "confirms the authentication event met Google's requirements for Multi-Factor Authentication". Request via `claims` parameter with `"amr": {"essential": true}`. — [Google Sign in with Google security bundle](https://developers.google.com/identity/siwg/security-bundle); [Google Developers blog](https://developers.googleblog.com/en/enhance-security-and-trust-new-session-metadata-in-sign-in-with-google/)
- Auth0 policy for comparison: if `amr` is absent or lacks `mfa`, treat the login as not MFA. — [Auth0 step-up docs](https://auth0.com/docs/secure/multi-factor-authentication/step-up-authentication/configure-step-up-authentication-for-web-apps.md)
- ASVS V6.8 intro: accept IdPs "as per the risk profile of the IdP's strength of identity proofing". — [ASVS V6](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x15-V6-Authentication.md)
- V10 client (RP) requirements, all L2: 10.1.2 `state`/PKCE `code_verifier`/`nonce` "not guessable, are specific to the transaction, and are securely bound to both the client and the user agent session"; 10.5.1 nonce check against ID Token replay; 10.5.2 identify the user by `sub`, "which cannot be reassigned"; 10.5.3 reject metadata whose issuer does not "exactly match the pre-configured issuer URL"; 10.5.4 check `aud` equals `client_id`; 10.5.5 back-channel logout token typed `logout+jwt`, has `event` claim, no `nonce`. — [ASVS V10](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x19-V10-OAuth-and-OIDC.md)

**Enterprise SAML 2.0 / OIDC SSO:**

- 6.8.2 signature validation and 6.8.3 SAML one-time assertion processing (L2); 6.8.1 namespace users by IdP; 6.8.4 strength checks; 7.1.3 and 7.6.1 document and enforce RP/IdP session coordination. — [ASVS V6](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x15-V6-Authentication.md); [ASVS V7](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x16-V7-Session-Management.md)

**SCIM provisioning:**

- ASVS 7.4.2 (L1): "terminates all active sessions when a user account is disabled or deleted (such as an employee leaving the company)." 7.4.5 (L2) admin session termination. — [ASVS V7](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x16-V7-Session-Management.md)

**Re-authentication timing:**

- NIST AAL2: "A definite reauthentication overall timeout SHALL be established, which SHOULD be no more than 24 hours at AAL2." "The inactivity timeout SHOULD be no more than 1 hour." After inactivity but within the overall timeout, reauthentication may use only a password or biometric plus the session secret. Session secrets at least 64 bits from an approved RBG. — [NIST SP 800-63B-4](https://pages.nist.gov/800-63-4/sp800-63b.html). ASVS 7.1.1 requires justifying deviations from these. — [ASVS V7](https://github.com/OWASP/ASVS/blob/v5.0.0_release/5.0/en/0x16-V7-Session-Management.md)

**Inferences:**

Per-method verdict for ASVS 5.0.0 L2 (6.3.3):

| Method | Factor status | Meets 6.3.3 alone? | Main ASVS IDs | Notes |
| --- | --- | --- | --- | --- |
| Password (Argon2id) | Know | No | 6.2.1–6.2.12, 11.4.2, 6.3.1, 6.4.3 | Needs a second factor. With MFA, NIST allows an 8-character minimum; ASVS still "strongly recommends" 15. |
| Passkey, synced, UV required | Have + know/are (multi-factor cryptographic) | Yes | 6.3.3, 6.3.4, 7.2.4 | NIST Appendix B: AAL2-capable, phishing resistant. Not L3. Must set `userVerification: "required"` (or check the UV flag and downgrade). |
| Passkey, device-bound, UV | Same | Yes, and meets L3 example | 6.3.3, 6.7.x (L3) | Hardware security keys; attestation needed for L3. |
| Passkey without UV | Have only | No | 6.3.3 | NIST: treat as single-factor. Pair with a password. |
| Email OTP | Not a factor | No, and not a second factor either | V6.6 intro, 6.3.6 (L3), 6.4.3, 6.5.x | Allowed for email address verification and as one issued recovery code (NIST 4.2.2). |
| Magic link | Not a factor | No | Same as email OTP; 6.5.1, 6.5.5, 6.6.2 | Same status, plus scanner consumption and cross-device breakage. |
| TOTP | Have | No; second factor with password | 6.5.1, 6.5.3, 6.5.5, 6.5.8 (L3) | Phishable; NIST AAL2 still needs a phishing-resistant option offered. |
| Lookup/recovery codes | Have | No; second factor or recovery | 6.5.1–6.5.4 | NIST saved recovery codes ≥ 64 bits. |
| SMS OTP | Have (restricted) | No; second factor only under 6.6.1 | 6.6.1–6.6.3 | Must also offer TOTP or stronger and explain risk. Banned at L3. |
| Social OIDC | Depends on IdP assertion | Only if `amr`/`acr` shows MFA, else assume single factor | 6.8.1–6.8.4, 10.1.2, 10.5.1–10.5.5, 7.6.1 | Google `amr` may be absent; fallback is local second factor (passkey/TOTP) after IdP login. |
| Enterprise SAML/OIDC SSO | Depends on IdP | Same, via `amr`/`acr` or SAML AuthnContext, or documented tenant contract | 6.8.2–6.8.4, 7.1.3, 7.6.1 | A contractual "tenant IdP enforces MFA" is a documented 6.3.3 relaxation, not proof. |
| SCIM | Not authentication | n/a | 7.4.2, 7.4.5 | Deprovisioning must kill sessions. |

Further inferences:

- A synced passkey with UV meets 6.3.3 by NIST's definition (AAL2-capable multi-factor cryptographic authenticator). This is the only consumer-friendly method that satisfies L2 MFA in one step, and it is phishing resistant. ASVS does not say this in its own words; it follows from ASVS deferring to NIST.
- Email OTP/magic link as the primary consumer login fails L2 even with nothing else, because email is neither a factor nor an allowed OOB channel. It can serve as one part of recovery, but 6.4.3 means a reset via email must not bypass an enrolled passkey/TOTP; NIST AAL2 recovery wants two recovery codes from different methods, or one plus a bound factor.
- Email OTP codes beat magic links on cross-device use (read on phone, type on desktop) and on scanner consumption, since scanners cannot type codes. Both still need 6.5.1 single use, 6.5.5 ≤ 10 minutes, 6.6.2 request binding and 6.6.3 rate limiting by analogy.
- For federated login, the safe L2 design is: trust IdP MFA only when `amr` contains an MFA value (or SAML AuthnContext shows it) with fresh `auth_time`; otherwise require a local passkey or TOTP after the IdP step, and document the fallback (6.8.4).
- Breached-password checks (6.2.12) via the HIBP range API leak only a 5-hex-character SHA-1 prefix; use `Add-Padding`. It is an external dependency, so the app needs a documented fail-open/fail-closed policy; a local copy of the corpus is the alternative.
- UX data consistently favors passkeys (FIDO 93% vs 78%; Microsoft 98% vs 32%; Google 63.8% vs 13.8%), but all are self-reported by large vendors with undefined or differing metrics. The FIDO finding that showing password/OTP first halves passkey use argues for passkey-first sign-in.

**Gaps:**

- Apple (Sign in with Apple) and Microsoft (Entra/consumer MSA) `amr`/`acr` behavior was not verified in this session. Treat as unknown until checked against their docs.
- SAML AuthnContextClassRef mapping to MFA was not sourced here.
- No authoritative, independent conversion or abandonment study for magic links vs email codes was found; scanner evidence is vendor and forum material. Shopify has no published passkey conversion data that I found; its blog claims Shop Pay users sign into customer accounts "up to 50% more" with sign in with Shop, which is not passkey-specific ([Shopify blog](https://www.shopify.com/blog/shop-sign-in-features)).
- Exact NIST 800-63B-4 section numbers were reported by the fetch tool for most quotes, but the tool noted some headings are unnumbered on the HTML page; verify numbering against the PDF before citing by number.
- OWASP Authentication, Forgot Password and Session Management cheat sheets and W3C WebAuthn Level 3 / passkeys.dev were not fetched in this session.
