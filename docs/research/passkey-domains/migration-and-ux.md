# Passkey domain migration and consumer passkey UX, for Tetsu

Research date: 2026-11-11. About 16 tool calls. Many sources are passkey vendor blogs (Corbado, MojoAuth, Authsignal, SSOJet, Turnkey, Descope) or identity provider docs (Okta, Auth0). They are flagged where used. They sell passkey products and tend to overstate adoption and understate friction.

## Domain moves, rebrands, and choosing an RP ID up front

### Takeaway

The RP ID is cryptographically bound into every passkey at creation and cannot be changed later. The one big public domain move, X retiring twitter.com in 2025, forced every passkey and security key user to re-enroll by a deadline, and users reported lockouts and login loops. The only escape hatches are moving to a parent domain of the old RP ID, or keeping the old RP ID and using Related Origin Requests (ROR), which caps at 5 distinct registrable-domain labels.

### Cited Findings

- **X (twitter.com to x.com).** twitter.com began redirecting to x.com in May 2024. On about 24 October 2025 X required every account using a passkey or hardware security key for 2FA to re-enroll it on x.com by 10 November 2025, or be locked until they re-enrolled or switched 2FA method — [Dataconomy](https://dataconomy.com/2025/11/13/passkey-migration-to-x-com-leaves-many-x-users-stuck-in-login-loops); [Security Affairs](https://securityaffairs.com/183928/security/x-warns-users-to-re-enroll-passkeys-and-yubikeys-for-2fa-by-nov-10.html)
- X said the re-enrollment was due to the domain migration, not a security incident. Authenticator-app users were unaffected — [TechRadar](https://www.techradar.com/pro/security/unsurprisingly-x-users-locked-out-in-the-cold-after-botched-authentication-switch); [Tom's Guide](https://www.tomsguide.com/computing/online-security/x-will-lock-you-out-of-your-account-if-you-dont-take-this-security-step-heres-what-to-do)
- After the deadline, many users reported errors and persistent login loops when trying to re-enroll, and some were locked out. X did not respond to press. No report describes how locked-out users recovered — [Dataconomy](https://dataconomy.com/2025/11/13/passkey-migration-to-x-com-leaves-many-x-users-stuck-in-login-loops); [TechRadar](https://www.techradar.com/pro/security/unsurprisingly-x-users-locked-out-in-the-cold-after-botched-authentication-switch)
- Note: X used passkeys as a second factor on top of a password here. That is why a fallback (switch 2FA method) existed at all. A passkey-only service has no such fallback on a domain move. (Inference from the above.)
- **Binding.** "The browser cryptographically ties passkeys to the RP ID when you create them." The RP ID may be the exact hostname or a parent domain that is eTLD+1 or higher. Public suffixes and IP addresses are not allowed — [web.dev, RP ID](https://web.dev/articles/webauthn-rp-id)
- An eTLD+1 RP ID works across subdomains. A narrower one such as `login.example.com` does not work on `shop.example.com`. Standard RP IDs do not support cross-site setups — [web.dev, RP ID](https://web.dev/articles/webauthn-rp-id)
- **Changing the RP ID invalidates existing passkeys**, except when the new RP ID is a parent of the old one, or ROR is configured — [Corbado (vendor), DE](https://www.corbado.com/de/blog/webauthn-relying-party-id-rpid-passkeys); same point: "if you change the RP ID, existing Passkeys (FIDO2 WebAuthn) enrollments don't work" — [Okta help](https://help.okta.com/oie/en-us/content/topics/identity-engine/authenticators/passkeys-relying-party-id.htm); [Okta support](https://support.okta.com/help/s/article/passkey-implementation-and-architecture-for-multiple-domains)
- **Related Origin Requests.** The RP ID domain serves `/.well-known/webauthn` listing other origins allowed to use its RP ID. Even on a secondary site the primary RP ID must be used for create and get. "You must choose one RP ID and use it across all sites" — [web.dev, ROR](https://web.dev/articles/passkey-related-origin-requests); [web.dev, RP ID](https://web.dev/articles/webauthn-rp-id)
- ROR "is designed to be used when federation is not possible"; consider OIDC first. Supported patterns are ccTLD variants and alternate branding. WebAuthn requires clients to support at least 5 labels and no known client supports more, so treat 5 as the maximum. Detect support with `getClientCapabilities()` `relatedOrigins` — [passkeys.dev, related origins](https://passkeys.dev/docs/advanced/related-origins/)
- passkeys.dev on choosing up front: "pick the most commonly used and/or understood domain for the common RP ID", typically the brand's primary .com. For sites that already have several RP IDs, each must host the well-known file, the database must record which RP ID each passkey uses, and sign-in needs an identifier-first flow with a backend lookup — [passkeys.dev](https://passkeys.dev/docs/advanced/related-origins/)
- Removing an origin from `/.well-known/webauthn` stops passkeys working from that origin; they still work on the RP ID domain itself — [Okta FAQ](https://support.okta.com/help/s/article/frequently-asked-questions-about-sharing-passkeys-with-associated-domains-in-okta)
- ROR browser support reported as Chrome/Edge 128 and Safari 18; one vendor claims Firefox later. Not verified against a primary matrix — [MojoAuth (vendor)](https://mojoauth.com/blog/related-origin-requests-passkeys-across-cctlds); check [passkeys.dev device support](https://passkeys.dev/device-support/)
- ROR does not merge or link accounts; it only lets one credential work across origins that share a user directory — [MojoAuth (vendor)](https://mojoauth.com/blog/related-origin-requests-passkeys-across-cctlds)
- Vendor headline framing: "Choosing Your RP ID: The Passkey Decision You Can't Undo" — [MojoAuth (vendor)](https://mojoauth.com/blog/webauthn-rp-id-passkey-decision)
- **Multi-tenant precedents.** Okta advises against using a vendor-owned domain as the RP ID, "to prevent credentials from different organizations from overlapping in the passkey manager of the end-user" — [Okta help](https://help.okta.com/oie/en-us/content/topics/identity-engine/authenticators/passkeys-relying-party-id.htm). Auth0 with multiple custom domains scopes passkeys per domain: one enrolled on login.brand1.com cannot be used on login.brand2.com — [Auth0 docs](https://auth0.com/docs/customize/custom-domains/multiple-custom-domains/passkeys)
- If tenants share an RP ID, a passkey valid for one tenant is usable at another tenant's origin under that RP ID — [Duende Software](https://duendesoftware.com/blog/20251014-deep-dive-into-relying-party-id-and-origin-with-passkeys)
- **Shopify** is the closest consumer precedent: one platform passkey (Shop passkey) signs customers into shop.app, Shop Pay checkout, and merchant customer accounts — [Shop help center](https://help.shop.app/shop/shop-pay-settings/security/passkeys); [Shopify Engineering](https://shopify.engineering/supporting-passkeys-in-shop-authentication-flows). User-reported traffic shows rpId `accounts.shopify.com` for customer account sign-in (not official) — [Shopify community](https://community.shopify.com/t/cannot-login-using-passkeys-on-firefox-using-bitwarden/659008)

### Inferences

- For Tetsu, a domain change after launch means every customer re-enrolls, as at X, unless the new RP ID is a parent domain of the old one or Tetsu keeps the old domain alive forever as the RP ID and lists new origins via ROR (max 5 labels). Per-business custom domains cannot scale through ROR.
- Choosing a short, stable, Tetsu-owned registrable domain as the RP ID (say `tetsu.app` rather than `book.tetsu.app` or a tenant subdomain) keeps the parent-domain escape hatch and lets every business subdomain share it. Using a narrower RP ID now and broadening later is safe; the reverse is not.
- Shopify's model (one platform account and RP ID across thousands of merchants) shows a consumer platform can put merchants' customers under a platform-branded passkey. Okta's warning applies to B2B tenants with separate user directories; for Tetsu it bites if a customer is a separate account per business. A per-business account under one RP ID would list many "tetsu.app" passkeys in the customer's manager.

### Gaps

- No other documented consumer rebrand with passkeys (beyond X) was found. No post-mortem from X.
- No first-party Google, Apple, or Microsoft doc specifically on rebrand or RP ID migration; guidance comes from web.dev and passkeys.dev (both Google-authored or FIDO-community) and vendors.
- Official Shopify docs on its RP ID were not found.

## How passkey providers show the RP to users, and whether the business name can appear

**Takeaway:**
Providers label a passkey by the RP ID (the domain) and `user.name`. `rp.name` and `user.displayName` are largely not shown, and Apple does not even accept `displayName` for platform passkeys. So a customer will see "tetsu.app" (or whatever the RP ID is), not the business name. The only reliably visible field the RP controls is `user.name`, which is meant to be the username.

**Cited Findings:**

- Apple engineer (Sept 2024): "The `name` field is meant to be the equivalent of what would be a username associated with a password ... that is the field we've chosen to support." "Because the `displayName` is never used in the UI, it's also not an option in the API for passkeys" — [Apple Developer Forums](https://developer.apple.com/forums/thread/762984)
- A 2023 WebAuthn WG post with screenshots reports Chrome and Safari display `user.name` when showing passkeys (may be dated) — [public-webauthn list](https://lists.w3.org/Archives/Public/public-webauthn/2023Feb/0049.html)
- `rp.name` does not appear in the passkey popup in current browser UIs; `displayName` also does not appear; set them sensibly anyway in case UIs change — [Turnkey docs (vendor)](https://docs.turnkey.com/authentication/passkeys/options.md)
- Providers keep the username and display name saved at creation; sites must call the Signal API (`signalCurrentUserDetails`) to update them, and `signalAllAcceptedCredentials` after deletion — [Chrome Signal API](https://developer.chrome.com/blog/passkeys-signal-api); [Chrome passkey management guide](https://developer.chrome.com/docs/modern-web-guidance/guides/security/passkey-management)
- RP account settings should show each passkey with an AAGUID-derived provider name or a user-chosen nickname; renaming is a server-side label — [Chrome passkey management guide](https://developer.chrome.com/docs/modern-web-guidance/guides/security/passkey-management); [web.dev AAGUID](https://web.dev/articles/webauthn-aaguid)

**Inferences:**

- With RP ID `tetsu.app`, the customer's iCloud Keychain or Google Password Manager entry reads roughly "tetsu.app / <jane@example.com>". If they booked at "Riverside Yoga", they may not recognise it. Putting the business name into `user.name` (for example "<jane@example.com> (Riverside Yoga)") would show it, but misuses the username field, breaks if one account spans businesses, and needs Signal API upkeep.
- If Tetsu has one customer account across all businesses (Shopify-like), a single "tetsu.app" passkey is coherent, and Tetsu's own sign-in page can carry the business brand before and after the OS dialog (FIDO's "handshake" messaging).

**Gaps:**

- No primary documentation found on exactly how 1Password and Bitwarden title passkey items (they likely use the RP ID or site URL, unverified). No current screenshot study of Chrome/Safari/Windows dialogs for `rp.name`.
- No official multi-brand display guidance from Apple, Google, or FIDO was found.

## Consumer enrollment and drop-off

**Takeaway:**
Across large consumer sites passkeys are widely offered but a minority of accounts have one. Passkey share of sign-ins was 26% in 2025 and 36% in 2026 per FIDO's index, with sign-in success 93% versus 78% for other MFA. Enrollment rises sharply when the prompt is automatic and placed right after account creation or verification. There is no data on infrequent, low-stakes services like bookings, and no published data on requiring passkeys at sign-up.

**Cited Findings:**

- FIDO Passkey Index 2025 (Oct 2025, confidential data from 9 members incl. Amazon, Google, LY Corp, Mercari): 93% of accounts eligible, 36% of eligible accounts enrolled, 26% of sign-ins by passkey — [FIDO 2025 press](https://fidoalliance.org/fido-alliance-launches-passkey-index-revealing-significant-passkey-uptake-and-business-benefits/); [PDF](https://fidoalliance.org/wp-content/uploads/2025/10/FIDO-Passkey-Index-October-2025.pdf)
- FIDO Passkey Index 2026 (15 Oct 2026): 36% of successful sign-ins use a passkey (up from 26%); enrollment grew but more slowly than usage; passkey sign-in success 93% vs 78% for traditional MFA; nearly three times faster, saving 27 seconds on average; all measuring participants saw fewer phishing successes and lower SMS OTP costs — [FIDO Passkey Index 2026](https://fidoalliance.org/passkey-index-2026/); [PDF](https://fidoalliance.org/wp-content/uploads/2026/10/Passkey-Index-October-2026.pdf). FIDO members self-report; large, frequent-use services only.
- Conflicting figure: one outlet reports 90% eligible, 42% enrolled; not reconciled with the index — [Biometric Update](https://www.biometricupdate.com/202510/new-benchmarking-tool-shows-passkeys-boost-conversion-success-by-30)
- FIDO "State of Passkeys 2026" (May 2026): 5 billion passkeys in active use — linked from [FIDO Passkey Index 2026](https://fidoalliance.org/passkey-index-2026/)
- eBay (Authenticate 2025, via vendor summary): auto-triggered biometric prompt doubled adoption rate (+102%); 75% of new passkeys came from the automatic in-flow prompt — [Corbado (vendor)](https://www.corbado.com/blog/passkey-adoption-case-studies-authenticate-2025.md)
- Amedia (Chrome case study): prompting right after WebOTP verification, before other choices, got 48% of new users to create a passkey, a 100% increase over a choice-based flow — [Chrome Developers](https://developer.chrome.com/blog/amedia-passkey-webotp-case-study)
- Vendor advice: automatic prompts on mobile get 30 to 50% higher acceptance; desktop prompts should be user-triggered; cool down about 30 days after repeated skips; first prompt has the strongest pull — [Corbado (vendor)](https://www.corbado.com/blog/passkey-creation-best-practices/passkey-user-prompts-increase-adoption); [Corbado benchmark](https://www.corbado.com/passkey-benchmark-2026/passkey-enrollment-rate.md)
- Shopify's Shop analysed for passkey sign-up and login patterns (vendor analysis) — [Corbado (vendor)](https://www.corbado.com/blog/shopify-passkeys)

**Inferences:**

- Booking customers sign in rarely, often on whichever device is at hand. Synced passkeys (iCloud, Google, third-party managers) are what make this work; a device-bound passkey on a shared or old device would strand them. Hybrid QR flow covers a desktop with no passkey but adds steps.
- Making a passkey the only path at sign-up for a one-off yoga booking is untested in public data and likely to cost conversions; ASVS L2 MFA could instead be met by a passkey offered as the primary option with a second factor fallback. This is a judgment, not evidence.

**Gaps:**

- No public enrollment or drop-off figures for Kayak, Best Buy, Dashlane, or Amazon specifically were retrieved in this pass. No data at all on bookings or small-business customer bases, or on mandatory passkey at sign-up.
- No independent academic usability study was retrieved.

## Recovery when a consumer loses all passkeys, and how domain choice interacts

**Takeaway:**
Synced passkeys make total loss rarer, but every service still needs a recovery path, and recovery is now the weakest link. Common fallbacks (password, email, SMS) are phishable. A domain change creates a forced mass "recovery" event for every user, as X showed.

**Cited Findings:**

- Synced passkeys survive device loss within the user's ecosystem; Apple and Google offer their own account recovery — [Authsignal (vendor)](https://authsignal.com/blog/articles/what-happens-when-your-passkey-device-is-lost-understanding-recovery-and-device-sync); [Corbado FAQ (vendor)](https://corbado.com/faq/what-if-passkey-device-lost)
- A second passkey on another device or a security key keeps recovery phishing-resistant — [MojoAuth (vendor)](https://mojoauth.com/blog/passkey-recovery-methods-compared.md); [SSOJet (vendor)](https://ssojet.com/ciam-qna/how-to-handle-passkey-recovery-when-users-lose-devices)
- Recovery should be at least as strong as primary authentication (attributed to NIST SP 800-63B); recovery is described as the cheapest attack path, including a phishing kit that hijacks iCloud Keychain recovery — [Corbado, Authenticate 2026 (vendor)](https://www.corbado.com/blog/passkey-account-recovery-authenticate-2026)
- Vendor survey Q1 2026: 69% of teams fall back to passwords when users lose all passkeys, 47% to other MFA, 35% to help desk — same Corbado source (vendor survey, methodology unknown)
- Practices: notify on all channels when recovery starts or an authenticator is added with a "this wasn't me" link; waiting periods or step-up for risky recovery — [Corbado (vendor)](https://www.corbado.com/blog/passkey-account-recovery-authenticate-2026); [SSOJet (vendor)](https://ssojet.com/blog/how-to-handle-passkey-recovery-when-users-lose-devices)
- FIDO principle: let people create a passkey during password reset or account recovery instead of a new password — [FIDO design principles](https://fidoalliance.org/design-guidelines/principles-2/)

**Inferences:**

- If Tetsu changed RP ID later, every customer would hit the recovery path at once; Tetsu's recovery (likely email) would then carry the whole user base and become the phishing target, as X's lockouts show at smaller scale.
- If businesses could bring custom domains as RP IDs, a business leaving Tetsu or changing its domain would orphan its customers' passkeys. A Tetsu-owned RP ID avoids this.

**Gaps:**

- No primary standard text on email-based recovery strength was fetched (the NIST point is second-hand via a vendor).

## FIDO Alliance UX guidelines for creation prompts and naming

**Takeaway:**
FIDO's design guidelines (built with Blink UX, 2023 and updated 2024) say to offer passkey creation alongside account tasks (creation, recovery, settings) rather than interrupting core tasks, to frame passkeys with familiar ideas (fingerprint, face, screen lock), to use consistent "handshake" messages and the passkey icon before and after the OS dialog, and to show each passkey as a card in settings. No rule on the word's capitalisation was found.

**Cited Findings:**

- 10 UX principles and 3 content principles. UX principle 1: prompt to create a passkey alongside account-related tasks (account creation, recovery, settings), not during core tasks like shopping. Others: connect the unfamiliar to the familiar; use tested passkey messages and the icon before and after the OS dialog; allow freedom and choice (keep passwords optional, offer a passkey at password reset); follow accessibility; use a consistent passkey "hero" prompt; keep the what and where of a passkey visible, not in tooltips; give passkeys equal standing with other sign-in options in settings; show each passkey as a card; plan for your own security needs — [FIDO design principles](https://fidoalliance.org/design-guidelines/principles-2/) (fetched page was a Korean rendering; principle titles summarised from translation)
- Content principles: pair passkey language with words people know; clear handshake messages ("Create a passkey" before, confirmation after); passkey messaging across the journey — same source
- Guidelines cover four journeys: account creation, sign-in, account recovery, and managing passkeys in settings; a Figma UI kit and icon set exist — [FIDO UX guidelines](https://fidoalliance.org/ux-guidelines-for-passkey-creation-and-sign-ins/); [FIDO design system](https://fidoalliance.org/design-system/ux-guidelines/)
- Lowercase "passkey" advice appears only in a vendor copy library — [MojoAuth (vendor)](https://mojoauth.com/blog/passkey-copy-library.md)

**Inferences:**

- For Tetsu, the FIDO principles fit offering a passkey right after booking-account creation (the Amedia and eBay pattern), with the business brand on Tetsu's own pre- and post-dialog screens, since the OS dialog itself will show only the RP ID and username.

**Gaps:**

- English verbatim wording of the FIDO principles was not retrieved (only a translated page). No FIDO guidance on multi-brand or white-label display.
