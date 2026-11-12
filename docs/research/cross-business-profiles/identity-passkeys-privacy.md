# Identity architecture, passkeys, and privacy for one person across many businesses

Scope: Tetsu today has one customer account per business. Each account has its own random WebAuthn user ID. Passkeys sit under Tetsu's main domain RP ID. Email or Google are the ways in. Sessions are server side, one per business, under one browser cookie. Data sits in pooled Postgres tables with row-level security. These notes weigh an optional, person-owned global profile linked to those accounts.

Sources were read on 2026-11-12. Quotes from the WebAuthn spec and EDPB guidelines come from full-text extracts of the primary documents. Some vendor docs could not be fetched. Those gaps are listed.

## Architecture patterns: global identity linked to per-business accounts, versus per-business accounts only, and how passkeys work in each

**Takeaway:**
Two shapes are common. In the first, a global identity holds the passkeys, and each per-business account links to it. In the second, each per-business account keeps its own passkeys. WebAuthn favors the first shape for one person. An authenticator keeps at most one discoverable credential per user handle per RP ID. All of Tetsu's per-business accounts share one RP ID, so a person with five businesses gets five passkeys under the same "Tetsu" entry in their credential manager, and the account picker must tell them apart.

**Cited Findings:**

- The WebAuthn user handle is "an opaque byte sequence with a maximum size of 64 bytes, and is not meant to be displayed to the user. It MUST NOT contain personally identifying information." — [W3C WebAuthn Level 3, Terminology and 5.4.3](https://www.w3.org/TR/webauthn-3/)
- "Authentication and authorization decisions MUST be made on the basis of this id member, not the displayName nor name members." The user handle "SHOULD NOT be a constant value across different user accounts." — [W3C WebAuthn Level 3, 5.4.3](https://www.w3.org/TR/webauthn-3/)
- The spec RECOMMENDS a user handle of "64 random bytes, and store this value in the user account." Hashes of PII are not allowed "unless the hash function is salted with salt values private to the Relying Party." — [W3C WebAuthn Level 3, 14.6.1](https://www.w3.org/TR/webauthn-3/)
- "An authenticator will store at most one discoverable credential per user handle per Relying Party," and the user handle tells authenticators "when to replace an existing discoverable credential with a new one." user.id "is used to overwrite existing discoverable credentials that have the same rp.id and user.id on the same authenticator." — [W3C WebAuthn Level 3, Terminology and 5.4](https://www.w3.org/TR/webauthn-3/)
- name and displayName "MAY be used by the authenticator and client in future authentication ceremonies to help the user select a credential, but are not returned to the Relying Party." — [W3C WebAuthn Level 3, 5.4](https://www.w3.org/TR/webauthn-3/)
- Under conditional mediation, the client silently discovers credentials for the RP ID. If the allow list (credentialIdFilter) is empty, every discovered credential for that RP ID is offered. — [W3C WebAuthn Level 3, 5.1.4 algorithm](https://www.w3.org/TR/webauthn-3/)
- With conditional mediation, "any available passkeys will be included in the autofill options" when the user focuses an input marked for webauthn. When allowCredentials is omitted, the browser shows a list of available passkeys. Passkeys are "identified by the user.name property set at creation time." — [web.dev, Discoverable credentials deep dive](https://web.dev/articles/webauthn-discoverable-credentials)
- When several discoverable credentials match an RP ID, platforms show a chooser. The guide advises recognizable user names and display names because the chooser relies on them. This is a secondary source. — [BEE-1008, Passkeys: Discoverable Credentials and UX Patterns](https://alivedise.github.io/backend-engineering-essentials/auth/passkeys-discoverable-credentials)
- The Signal API lets the RP keep credential managers in sync. signalUnknownCredential reports one deleted credential, after which "the authenticator then deletes or hides the credential." signalAllAcceptedCredentials sends the full list for a userId. RPs "SHOULD prefer" it "when the user is authenticated." signalCurrentUserDetails updates name and displayName for a userId. — [W3C WebAuthn Level 3, 5.1.10](https://www.w3.org/TR/webauthn-3/)
- The spec warns that signalAllAcceptedCredentials could hide valid passkeys if the RP leaves IDs out by mistake, and suggests authenticators hide rather than delete "to aid recovery." — [W3C WebAuthn Level 3, 5.1.10.3](https://www.w3.org/TR/webauthn-3/)
- adidas used Conditional Create and the Signal API to raise passkey adoption and reliability. — [web.dev case study, adidas](https://web.dev/case-studies/adidas-passkeys)
- Auth0 links identities into one profile with a primary and a secondary account. The result keeps the primary's user_id, and the secondary goes into the primary's identities array. Linking can be user-initiated ("Link accounts" button), suggested (app finds matching emails and prompts), or automatic for Token Vault connected accounts. — [Auth0, User Account Linking](https://auth0.com/docs/manage-users/user-accounts/user-account-linking)
- Auth0 does not merge metadata. The secondary account's user_metadata and app_metadata are discarded unless the app merges them through the Management API. Deleting the primary also deletes the secondary. — [Auth0, User Account Linking](https://auth0.com/docs/manage-users/user-accounts/user-account-linking)
- In Mindbody's model, the Mindbody account is an umbrella login. It links to separate client profiles that each business keeps in its own local database, and they "only communicate once they've been linked together." — [Mindbody support, account FAQ](https://support.mindbodyonline.com/s/article/I-already-have-an-account?language=en_US) (summary from search; the page did not render for direct fetch)

**Inferences:**

- Shape A: one global identity owns the passkeys. Give it a new random 64-byte user handle and register its passkeys with it. Each per-business account then points to the global identity, and signing in once yields a session that can reach every linked account. This gives one picker entry per person, which is the cleanest credential manager UX.
- Shape B: per-business accounts only, which is Tetsu today. Each account's random user handle yields its own discoverable passkey under the same RP ID. One person with N businesses sees N "Tetsu" entries, and the label (user.name, displayName) is all that tells them apart. Labels like "<alex@example.com> at Riverside Yoga" help. signalCurrentUserDetails can fix labels later.
- Shape C: a hybrid. Existing per-business passkeys stay valid and still authenticate their own account. After sign-in the server resolves the account to the linked global identity. No re-enrollment is needed.
- Sharing one user handle across per-business accounts would make each new registration overwrite the last on the same authenticator. That follows from the overwrite rule. Tetsu should not reuse a per-business user handle as the global one.
- Under one RP ID, conditional mediation on a business's page offers every Tetsu passkey on the device, including those for other businesses, unless the RP sends an allow list. The picker labels reveal to anyone holding the device which businesses the person uses. A global identity reduces this to one entry.

**Gaps:**

- Clerk, WorkOS, Stytch, and Okta CIC docs on organizations versus consumer identities were not fetched in this session. Their models (organizations as B2B tenants, users global) are known in outline, but no cited text supports details here.
- No Apple or Google doc was fetched on how many same-RP passkeys the iOS, macOS, or Google Password Manager picker shows, or how it sorts them.
- No passkeys.dev page explicitly covers one person with many accounts under one RP ID.

## Migration: adding a global identity after per-business accounts and passkeys exist

**Takeaway:**
Add the global identity as a new record that per-business accounts link to. Do not merge accounts, and do not change their user handles, so existing passkeys keep working. Link only after the person proves control of each account in the same session, for example by signing in to each one or by a fresh verified-email code. Auto-linking on a matching email is the known path to account takeover. Mindbody's 2025 universal account rollout shows the business side of the risk: sign-in breakage, and studios losing control of their own customer records.

**Cited Findings:**

- Mindbody introduced a Universal Account system around July 31, 2025. "Many clients have encountered temporary issues signing in if their studio accounts and universal accounts aren't synced." The fixes were syncing through account.mindbodyonline.com, linking the studio under "Places You Go," or creating a new studio profile whose name and email "must exactly match." — [Bay Studios notice](https://bay-studios.com/mindbody-universal-account-update/)
- A studio operator wrote on Reddit (Sep 5, 2025) that under the rollout "the studios cannot change cc numbers or send a reset password email" and "MB believes that the studio customers are MB customers not studio customers." The rollout was "partially rolled back after public backlash." This is reported by Vibefam, a competitor, quoting Reddit. Treat it with care. — [Vibefam, Mindbody review 2026](https://vibefam.com/mindbody-review-pricing-features-pros-cons-2026/)
- After a person syncs a new account, Mindbody says they may have two profiles at a business, one per email, and must ask each business to merge them. — [Mindbody support, account FAQ](https://support.mindbodyonline.com/s/article/I-already-have-an-account?language=en_US) (via search summary)
- Mindbody's studio-side Merge Duplicate Clients tool cannot be fully reversed. An Unmask Merged Clients tool separates accidental merges. — [Mindbody support, Merge Duplicate Clients](https://support.mindbodyonline.com/s/article/203259603-Merge-Duplicate-Clients-tool?language=en_US) (via search summary)
- Integrators report that duplicate emails, often from family members sharing a parent's email, block customers from signing in after Mindbody migrations. — [zingFit support](https://support.zingfit.com/en/articles/4336677-mindbody-data-migration-merging-duplicate-customers)
- Auth0 warns: "Insecurely linking accounts can allow malicious actors to access legitimate user accounts." Its guidance is that "your tenant should request authentication for both accounts before linking occurs" and "every manual account link should prompt the user to enter credentials." — [Auth0, User Account Linking](https://auth0.com/docs/manage-users/user-accounts/user-account-linking)
- Account pre-hijacking: an attacker sets up the account before the victim does. Of 75 popular services tested, at least 35 were vulnerable. The root cause is not verifying identifier ownership before the account can be used. Variants include classic-federated merge, unexpired session, trojan identifier, unexpired email change, and non-verifying IdP. — [Sudhodanan and Paverd, USENIX Security 2022, arXiv 2205.10174](https://www.emergentmind.com/papers/2205.10174); [The Register](https://www.theregister.com/2022/05/25/web_pre_hijacking/); [Ravelin](https://www.ravelin.com/blog/what-is-account-pre-hijacking)
- CVE-2026-35511 (Authorizer): an OAuth callback linked Google or GitHub logins to existing accounts with matching but unverified emails. The merge also marked the email verified and left the attacker's password working. — [SecureLayer7](https://securelayer7.net/lab/cve-2026-35511-authorizer-oauth-account-takeover-unverified-email-linking) (one vendor write-up, not checked against NVD)
- better-auth: magic-link and email-OTP sign-in adopted pre-existing accounts and marked them verified, without revoking earlier sessions or passwords. — [SecureLayer7](https://securelayer7.net/lab/better-auth-pre-account-hijacking-magic-link-email-otp)

**Inferences:**

- Do not merge. Keep each per-business account and its random user handle. Add a link row from the global identity to each account. Existing passkeys then still authenticate their account, and the server follows the link. This avoids Mindbody's irreversible-merge and duplicate-profile problems.
- Link only on explicit action, with proof for both sides. The person signs in to the global identity, then proves control of the per-business account in the same flow, by its passkey, a fresh email code, or Google. Offering "we found accounts at 3 businesses with this email, link them?" is fine as a suggestion, as Auth0's "suggested" mode does. Each link still needs proof.
- An email that a business typed in for a walk-in customer is not proof the person controls it. Never treat a business-entered email as verified for linking.
- When a link or adoption happens, revoke prior sessions and pending email changes on the adopted account. The pre-hijacking and better-auth cases show what goes wrong otherwise.
- The global identity should not take over the business's tools. Mindbody's backlash came from studios losing password resets and card updates. Tetsu should keep the business's per-account controls working whether or not a global link exists.
- Passkeys for the global identity are new registrations with a new user handle. Old per-business passkeys can stay or be retired. If retired, signalAllAcceptedCredentials can hide them from the picker.

**Gaps:**

- No official Mindbody statement on the 2025 rollout, its rollback, or its root causes was found. The Consumer Identity overview page failed to render.
- No engineering blog was found describing a passkey-preserving migration from per-tenant to global identity.

## Recovery and lifecycle when one identity spans many businesses

**Takeaway:**
The global identity becomes a high-value target, because taking it over reaches every linked business. Its recovery must be at least as strong as the strongest linked account. Unlinking and deletion should cut links, not data. The business's record stays with the business. The person's profile stays with the person. Each side's deletion should not silently destroy the other's records.

**Cited Findings:**

- Auth0: deleting a primary account also deletes the linked secondary. — [Auth0, User Account Linking](https://auth0.com/docs/manage-users/user-accounts/user-account-linking)
- Mindbody: changes made in the account portal "are automatically updated in your linked member profiles at locations where you're verified." — [Mindbody, Consumer Identity overview](https://support.mindbodyonline.com/s/article/Consumer-Identity-Overview?language=en_US) (via search summary; page did not render directly)
- signalAllAcceptedCredentials and signalUnknownCredential let the RP remove or hide passkeys after deletion, so stale entries do not linger in the picker. — [W3C WebAuthn Level 3, 5.1.10](https://www.w3.org/TR/webauthn-3/)
- EDPB example: when a group shares a database but "each entity decides independently on the access, the retention periods, the correction or deletion" of its own clients' data, and they "cannot access or use each other's data," each is a separate controller. — [EDPB Guidelines 07/2020, para 71](https://www.edpb.europa.eu/system/files/2023-10/EDPB_guidelines_202007_controllerprocessor_final_en.pdf)
- Under CCPA, a service provider that gets a deletion request must notify service providers, contractors, or third parties "who may have accessed personal information from or through" it, "unless this proves impossible or involves disproportionate effort." — [CCPA, Cal. Civ. Code 1798.105 via CPPA](https://cppa.ca.gov/regulations/pdf/cppa_act.pdf)

**Inferences:**

- Recovery of the global identity: allow several passkeys and a verified email. Avoid letting a weaker per-business method, such as an email the business typed in, recover the global identity.
- Unlinking: the person removes the link. The per-business account continues to work with its own passkeys and email. Fields already shared remain in the business's record, because they are now the business's data.
- A business deletes a customer: delete that per-business account and its link. The global profile survives. Signal the removed credential if it belonged only to that account.
- A person deletes the global profile: delete the profile and all links. Ask per business whether to also request deletion of each per-business account. Each business decides on deletion of its own record, within legal duties.
- Sessions: one cookie with per-business server sessions already fits. A global session could mint per-business sessions on demand, rather than all at once.

**Gaps:**

- No vendor doc was fetched on recovery flows for a CIAM identity that spans many tenants.

## Privacy law: GDPR roles, consent, minimization, erasure, portability; CCPA sale and share

**Takeaway:**
For per-business customer records, Tetsu is most likely a processor for each business, as today. For a person-owned global profile, Tetsu decides why and how it is kept, so Tetsu is a controller for it. When the person tells Tetsu to send fields to a business, that is most likely a transfer between separate controllers, not joint control, as long as Tetsu and the business do not jointly decide purposes and essential means. Under CCPA, a disclosure the consumer intentionally directs is not a "sale" or "share."

**Cited Findings:**

- Controllers set purposes and "essential means." Essential means "are closely linked to the purpose and the scope of the processing," such as which data, for how long, and who has access. "Non-essential means" can be left to the processor. — [EDPB Guidelines 07/2020, paras 38 to 41](https://www.edpb.europa.eu/system/files/2023-10/EDPB_guidelines_202007_controllerprocessor_final_en.pdf)
- "A processor infringes the GDPR, however, if it goes beyond the controller's instructions and starts to determine its own purposes and means of the processing. The processor will then be considered a controller in respect of that processing" (Article 28(10)). — [EDPB Guidelines 07/2020, executive summary](https://www.edpb.europa.eu/system/files/2023-10/EDPB_guidelines_202007_controllerprocessor_final_en.pdf)
- Joint controllership can arise from a common decision or from "converging decisions" that "complement each other" and where each party's participation is "inseparable, i.e. inextricably linked." — [EDPB Guidelines 07/2020, executive summary and paras 54 to 55](https://www.edpb.europa.eu/system/files/2023-10/EDPB_guidelines_202007_controllerprocessor_final_en.pdf)
- "The use of a common data processing system or infrastructure will not in all cases lead to qualify the parties involved as joint controllers." — [EDPB Guidelines 07/2020, para 68](https://www.edpb.europa.eu/system/files/2023-10/EDPB_guidelines_202007_controllerprocessor_final_en.pdf)
- Travel agency example: an agency sending customer data to an airline and hotels makes them "three different data controllers," with no joint control. If they set up a common booking platform and "agree on the essential means," and share data for joint marketing, they become joint controllers for that platform only. — [EDPB Guidelines 07/2020, travel agency example](https://www.edpb.europa.eu/system/files/2023-10/EDPB_guidelines_202007_controllerprocessor_final_en.pdf)
- "The exchange of the same data or set of data between two entities without jointly determined purposes or jointly determined means of processing should be considered as a transmission of data between separate controllers." — [EDPB Guidelines 07/2020, para 70](https://www.edpb.europa.eu/system/files/2023-10/EDPB_guidelines_202007_controllerprocessor_final_en.pdf)
- A host that stores data "solely on behalf of its customers" and per their instructions is a processor. "Company XYZ is a processor in relation to the processing and storage of other companies' employees." — [EDPB Guidelines 07/2020, paras 66 and 71 examples](https://www.edpb.europa.eu/system/files/2023-10/EDPB_guidelines_202007_controllerprocessor_final_en.pdf)
- The EDPB cites Fashion ID (C-40/17, paras 77 to 79): joint responsibility attaches only to the stages a party helps decide. — [EDPB Guidelines 07/2020, footnote 30](https://www.edpb.europa.eu/system/files/2023-10/EDPB_guidelines_202007_controllerprocessor_final_en.pdf)
- GDPR Article 20: the data subject can receive data they provided "in a structured, commonly used and machine-readable format" and transmit it "to another controller without hindrance," and have it sent directly "where technically feasible." It applies only when processing rests on consent or contract and is automated. — [GDPR Art. 20](https://gdpr-info.eu/art-20-gdpr/)
- CCPA "sell" means disclosing personal information "to a third party for monetary or other valuable consideration." A business "does not sell personal information when: (A) A consumer uses or directs the business to intentionally: (i) Disclose personal information. (ii) Interact with one or more third parties." — [CCPA, Cal. Civ. Code 1798.140(ad)](https://cppa.ca.gov/regulations/pdf/cppa_act.pdf)
- CCPA "share" means disclosure "to a third party for cross-context behavioral advertising, whether or not for monetary or other valuable consideration." It has the same exception for consumer-directed intentional disclosure. — [CCPA, Cal. Civ. Code 1798.140(ah)](https://cppa.ca.gov/regulations/pdf/cppa_act.pdf)
- "Intentionally interacts" means "the consumer intends to interact with a person, or disclose personal information to a person, via one or more deliberate interactions." "Hovering over, muting, pausing, or closing a given piece of content does not constitute" intent. — [CCPA, Cal. Civ. Code 1798.140(s)](https://cppa.ca.gov/regulations/pdf/cppa_act.pdf)
- A secondary source says the consumer-directed exception holds only if the third party does not also sell the information. — [TrueVault, Do You "Sell" Personal Information?](https://knowledge.truevault.com/article/222-do-you-sell-personal-information)

**Inferences:**

- Per-business records: Tetsu stays a processor for each business. This fits the shared-database examples, since RLS already keeps businesses from seeing each other's data.
- Global profile: Tetsu sets its purpose (reuse across businesses), its fields, and its retention. That makes Tetsu a controller for the profile. Tetsu needs a lawful basis (contract or consent) and its own privacy notice to the person.
- Share to a business: the person directs Tetsu to send chosen fields. The business then holds them as its own controller data. Per para 70 and the travel agency example, this is separate controllers, not joint control. Joint control becomes a risk if Tetsu and businesses jointly design shared uses, such as cross-business marketing or a shared customer directory.
- Do not use per-business data for Tetsu's own purposes, such as filling the global profile from a business's records without the person acting. Article 28(10) would make Tetsu a controller for that use and could breach the processor agreement.
- Minimization: share only the fields a business's booking form needs, chosen per business.
- Erasure: deleting the global profile is Tetsu's duty as controller. Data already sent to a business is that business's. Tetsu may need to tell recipients under GDPR Article 19 (notify recipients of erasure) and to help businesses as processor. The Article 19 text was not re-fetched in this session.
- Portability: Article 20 covers data the person gave Tetsu for the profile, if the basis is consent or contract. Per-business portability requests go to each business, with Tetsu helping as processor.
- CCPA: a share the person triggers with a deliberate click falls under the consumer-directed exception. A pre-ticked or hover-based share would not. No advertising use means no "share" for cross-context behavioral advertising.

**Gaps:**

- No ICO guidance page was fetched in this session. No EDPB or ICO text specific to booking marketplaces or multi-merchant wallets was found.
- No CPPA regulation text on consent design (dark patterns, 11 CCR 7004) was fetched.
- GDPR Articles 17(2) and 19 were not fetched.

## Simplest consent UX patterns

**Takeaway:**
No primary source on multi-merchant consent UX was gathered here. The legal sources still point to a simple pattern. On a business's sign-up form, offer "Fill from my Tetsu profile." Show the exact fields that will be shared with that named business. Require one deliberate click. Remember the choice per business, and let the person view and revoke it in one list.

**Cited Findings:**

- A CCPA exception needs a deliberate act. Hovering or closing content is not intent. — [CCPA, Cal. Civ. Code 1798.140(s)](https://cppa.ca.gov/regulations/pdf/cppa_act.pdf)
- Auth0's "suggested" linking finds matching accounts and prompts the user, rather than acting silently. — [Auth0, User Account Linking](https://auth0.com/docs/manage-users/user-accounts/user-account-linking)
- Mindbody's "Places You Go" lists the businesses an account is linked to, and the person can link a studio there. — [Bay Studios notice](https://bay-studios.com/mindbody-universal-account-update/)
- When the calling origin and RP ID differ, clients "SHOULD" show both, to reduce "user confusion regarding who is accessing" the credential. — [W3C WebAuthn Level 3, 14.5.5](https://www.w3.org/TR/webauthn-3/)

**Inferences:**

- Prompt: "Share with Riverside Yoga: name, email, phone. [Share] [Edit] [Not now]." The business name and the field list are visible. Nothing is pre-ticked beyond the fields the form needs.
- Pre-filled sign-up: the fields fill in, and the person can edit before submit. Submit is the deliberate act.
- Remember consent per business and per field set. Ask again only when a business asks for a new field.
- Revoke: a "Businesses you share with" page, like "Places You Go," with an unlink per row. Explain that already-shared data stays with the business and how to ask it to delete.
- Profile edits: ask before pushing changes to businesses, or let the person choose "keep my details up to date at this business." Mindbody's automatic sync is the opposite default.

**Gaps:**

- No primary source was gathered on Shop Pay, Stripe Link, OpenTable, or similar multi-merchant consent flows.
- No usability study on per-merchant share prompts was found.
