# Identity realms, signup, and login in collaboration and developer SaaS (Linear, Slack, Notion, GitHub, Atlassian, Figma, Calendly)

Researched 2026-11-11. Vendor help pages were fetched on that date, except where noted. Where a claim comes from a third-party page or from search summaries, it says so. Claims recalled from background knowledge but not re-verified are listed only under Gaps.

## 1. Global identity vs per-workspace accounts, and multi-workspace login and switching

### Takeaway

Most of these products use one global user with memberships in workspaces or organizations: Linear, Notion, Figma, GitHub, Atlassian, and probably Calendly. Slack is the exception. One email gives a separate account in each workspace, but a single email login finds all of them. Several products also let one browser hold more than one global account (several emails) and switch between them.

### Cited Findings

- **Slack: per-workspace accounts.** "You can use the same email address to join as many workspaces as you'd like, but you'll have separate Slack accounts for each one." — [Slack: Join a Slack workspace](https://slack.com/help/articles/212675257-Join-a-Slack-workspace) (wording quoted in a search summary)
- **Slack: one email confirmation finds every workspace.** One email address logs in to all of a person's workspaces. After confirming with a code, passkey, or SSO, the user picks a workspace from a list. Desktop adds more through "Add a workspace" > "Sign in to another workspace". Password-free login sends "a code that allows you to sign in to any workspaces associated with your email address." — [Slack: Log in to Slack](https://slack.com/help/articles/212681477-Sign-in-to-Slack)
- **Slack: user IDs differ per workspace.** A third-party test found that the same email had different user IDs in two workspaces, and that presence and status are set per workspace. This is a single unofficial test. — [slack.green](https://slack.green/en/blog/slack-multiple-workspaces)
- **Slack: outside workspaces stay separate.** A university IT page says that logging in to a workspace outside its Enterprise Grid with the same email gives a separate account that is "not associated with" the university Slack account. — [U-M TeamDynamix](https://teamdynamix.umich.edu/TDClient/30/Portal/KB/ArticleDet?ID=10178)
- **Linear: global user, several accounts per browser.** You can be logged in to several user accounts at once (several emails) and switch between their workspaces without logging in again. Each added email's workspaces appear under "Switch workspace". Logging out "will sign you out of all other sessions." — [Linear: Login methods](https://linear.app/docs/login-methods)
- **GitHub: global personal account.** Each GitHub account can be linked to exactly one external (SAML) identity per organization. Each external identity can be linked to exactly one GitHub account in an organization. In this model, one personal account belongs to many orgs. — [GitHub: About authentication with SAML SSO](https://docs.github.com/en/authentication/authenticating-with-saml-single-sign-on/about-authentication-with-saml-single-sign-on)
- **GitHub EMU: identity owned by the enterprise.** Under Enterprise Managed Users, the IdP owns the lifecycle, username, profile, and org membership. "Managed user accounts cannot create public content or collaborate outside your enterprise." People therefore keep a separate personal account for open source. — [GitHub: About Enterprise Managed Users](https://docs.github.com/en/enterprise-cloud@latest/admin/managing-iam/understanding-iam-for-enterprises/about-enterprise-managed-users)
- **Atlassian: global account, claimable by domain owners.** An Atlassian account becomes "managed" when an org verifies the email domain and then claims the account. Unclaimed accounts on a verified domain, and accounts on personal email, are "external users". The org can invite or remove external users but cannot edit or delete their accounts. — [Atlassian: What are managed accounts](https://support.atlassian.com/user-management/docs/what-are-managed-accounts/)
- **Figma: org membership follows the email domain.** Members are users whose email matches one of the org's verified domains. Everyone else is a guest. — [Figma: Log in / SSO](https://help.figma.com/hc/en-us/articles/360040532333-Log-in-to-Figma); [Figma: Add members or guests](https://help.figma.com/hc/en-us/articles/360040453113-Add-members-or-guests-to-an-organization)
- **Notion: one account, workspace-level login rules.** Notion has help pages on creating, joining, and switching workspaces, and its login page says a user "may be asked to sign in with a specific provider when you open a workspace" if that workspace requires it. Login rules are therefore checked per workspace on top of one account. — [Notion: Log in and out](https://www.notion.com/help/log-in-and-out); [Notion: Create, join & leave workspaces](https://www.notion.com/help/create-delete-and-switch-workspaces)

### Inferences

- The global-user model (Linear, Notion, Figma, GitHub, Atlassian) is the common pattern. It fits Tetsu staff who belong to several businesses: one person, many memberships, and a business switcher.
- Slack's model gives each workspace its own user record, with email-based discovery as glue. That isolates tenants well, but profiles, MFA, and passkeys must be set up per workspace. Slack has moved toward a global feel with "sign in to any workspaces associated with your email."
- GitHub EMU and Atlassian claiming show the tension Tetsu will meet if a business ever wants to own its staff identities. A global account can only be partly controlled by one tenant.
- Linear's "sign out of one session signs out of all other sessions" shows that sessions belong to the global user, not to a workspace.

### Gaps

- Whether workspaces inside one Slack Enterprise Grid org share one user ID was not confirmed from an official source. From background knowledge, Grid gives one org-level account across its workspaces, but this is unverified.
- Notion's switcher mechanics and support for several logged-in emails were not fetched in detail.
- Calendly's account model was not confirmed: whether one user can belong to more than one organization. The sources found tie an account to one login method at a time and do not settle membership in several organizations.
- Figma's support for several logged-in accounts was not covered by the fetched page.

## 2. Login methods by plan, and signup flow steps

**Takeaway:**

The common baseline is Google OIDC plus passwordless email (a code, or a link and code), with passkeys increasingly common. Microsoft and Apple are offered by Notion, Slack (Apple), and Calendly (Microsoft). Passwords survive at Notion, Figma, and Calendly, and at Slack only for Frontline Members. SAML SSO and SCIM sit on top tiers. Workspaces join by invitation, or automatically through approved or verified email domains.

**Cited Findings:**

- **Slack.** Methods: an email code (not a link), a passkey, Apple, Google, or SSO where required. Username and password are only for "Frontline Members", who then get an email verification code. 2FA asks for a 6-digit code where the workspace requires it. — [Slack: Log in](https://slack.com/help/articles/212681477-Sign-in-to-Slack)
- **Linear: methods.** Google, email (a "login link or code" that works either way), passkeys (several devices, under Preferences > Account > Security & Access), and SAML. — [Linear: Login methods](https://linear.app/docs/login-methods)
- **Linear: plans and admin rules.** Login restrictions need Business or Enterprise. "SAML authentication and IP restrictions are only available to workspaces on our Enterprise plan." Admins can require specific login methods. Owners and admins can always log in by any method, to prevent lockout. — [Linear: Login methods](https://linear.app/docs/login-methods)
- **Linear: approved domains.** Approved email domains let anyone with a matching address join without an invite. Linear warns that a domain you cancel or transfer must be removed from the approved domains. — [Linear: Invite members](https://linear.app/docs/invite-members)
- **Notion: methods.** An email verification code, email and password, Google, Apple, Microsoft, passkeys, and SAML SSO on Business and Enterprise. Google and Microsoft work only when the provider's email equals the Notion email. — [Notion: Log in and out](https://www.notion.com/help/log-in-and-out)
- **Notion: joining.** "Allowed email domains" auto-join people with matching addresses. Verified domains are a separate setting, and SAML SSO needs at least one. JIT provisioning lets a SAML user join automatically as a member. — [Notion: Workspace settings](https://www.notion.com/help/workspace-settings); [Notion: SAML SSO configuration](https://www.notion.com/help/saml-sso-configuration)
- **Figma: methods.** Email and password, Google, and SAML 2.0 SSO on the Organization, Education, Government, and Enterprise plans. Google SSO for an org is all-or-nothing: "There is no way to make this optional or enable this for only some users." Government requires SAML and does not support email and password. Governance+ on Enterprise supports several IdPs. — [Figma: Log in / SSO](https://help.figma.com/hc/en-us/articles/360040532333-Log-in-to-Figma)
- **Figma: domain capture.** People who sign up from verified domains (checked with a DNS TXT record) are added automatically as org members. — [Figma: Manage domain capture](https://help.figma.com/hc/en-us/articles/360045953273-Manage-domain-capture-for-an-organization)
- **Figma: device verification.** On unverified domains, members may have to verify by email on each new device or browser. On verified domains they skip this. — [Figma: Log in / SSO](https://help.figma.com/hc/en-us/articles/360040532333-Log-in-to-Figma)
- **Calendly: methods.** Google, Microsoft (Microsoft 365 or Outlook.com), a Calendly password, and SAML SSO. — [Calendly help 360038853374](https://help.calendly.com/hc/en-us/articles/360038853374)
- **Calendly: changing method.** Users switch between Google, Microsoft, and password under Login preferences, confirmed by a 6-digit email code. — [Calendly: Update login preferences](https://calendly.com/help/how-to-update-your-login-preferences)
- **Calendly: SSO.** SAML SSO is Enterprise. Once SSO is enforced, users must use it, and only the org owner can still use "Log in using another method". — [Calendly: SAML SSO overview](https://help.calendly.com/hc/en-us/articles/360056867653)
- **GitHub EMU.** EMU supports SAML or OIDC, with Entra Conditional Access Policy checks under OIDC. Accounts are provisioned by SCIM 2.0. Okta and Entra ID cannot be combined for SSO and SCIM. — [GitHub: About EMU](https://docs.github.com/en/enterprise-cloud@latest/admin/managing-iam/understanding-iam-for-enterprises/about-enterprise-managed-users)

**Inferences:**

- The common signup flow is: choose Google, Microsoft, or Apple, or enter an email. Verify the email by a code (Slack, Notion), a link or code (Linear), or OAuth. Then either join a workspace (by invite or matching domain) or create one.
- Codes that work across devices are preferred over links alone. Slack and Notion use codes, and Linear offers both. Codes avoid the problem of a link opening in the wrong browser.
- An owner exemption from enforced login methods (Linear, Calendly) is a standard lockout guard.
- For Tetsu, domain auto-join probably matters less, because many small businesses use gmail.com. Invitation will be the main path. Note that Linear's domain-transfer warning echoes the risk in section 6.

**Gaps:**

- Plan tiers for Linear SCIM, Slack SAML and SCIM, and Notion SCIM were not confirmed in this pass.
- Atlassian's login methods for end users (Google, Microsoft, Apple, Slack login, passkeys) were not fetched.
- Whether GitHub supports passkeys, and on which plans, was not fetched. From background knowledge, GitHub made passkeys generally available in 2023, but this is unverified here.

## 3. MFA enforcement and account linking

**Takeaway:**

Org-level enforcement is common: Slack, GitHub orgs, and Atlassian through authentication policies. When SSO is on, MFA is handed to the IdP. GitHub made 2FA mandatory for every code contributor in 2023. Account linking is mostly implicit by matching email (Notion, Calendly). Apple "Hide My Email" breaks that match.

**Cited Findings:**

- **Slack.** Org owners and admins can make 2FA mandatory across workspaces, and new members must enroll before they can log in. With SSO, the IdP should handle 2FA, but members not required to use SSO, and guests, still set up Slack 2FA. Admins can temporarily disable 2FA for a member who is locked out. — [Slack: Mandatory workspace 2FA](https://slack.com/help/articles/212221668-Mandatory-workspace-two-factor-authentication-)
- **Slack Grid sessions.** Org-level session duration and 2FA settings apply to all member and guest accounts. Salesforce training material says so, and sources conflict a little on which roles use Slack's native MFA. — [Salesforce Trailhead](https://trailhead.salesforce.com/content/learn/modules/identity-management-in-slack/explore-access-and-security-controls)
- **GitHub mandate.** GitHub required 2FA for all users who contribute code on GitHub.com, rolling it out by group from March 13, 2023, with about 45 days' notice for each group. — [GitHub blog](https://github.blog/news-insights/product-news/raising-the-bar-for-software-security-github-2fa-begins-march-13/); [TechCrunch](https://techcrunch.com/2023/03/09/github-to-require-2fa-for-all-contributors-starting-from-march-13-to-secure-the-software-supply-chain/)
- **GitHub orgs.** Org owners can require 2FA for members, outside collaborators, and billing managers. — [GitHub: Requiring 2FA in your organization](https://docs.github.com/en/organizations/keeping-your-organization-secure/managing-two-factor-authentication-for-your-organization/requiring-two-factor-authentication-in-your-organization)
- **Atlassian: tiers.** Atlassian Guard Standard is needed for more than one authentication policy and for enforcing SSO, 2SV, and mobile session expiry. — [Atlassian: Authentication policy settings](https://support.atlassian.com/security-and-access-policies/docs/authentication-policy-settings-for-your-organizations/); [Atlassian Guard licensing](https://www.atlassian.com/licensing/atlassian-guard)
- **Atlassian: 2SV rules.** With SSO, 2SV can only be set at the IdP. 2SV can be made optional for some managed accounts, but not for some external users only. — [Atlassian: Enforce two-step verification](https://support.atlassian.com/security-and-access-policies/docs/enforce-two-step-verification/)
- **Atlassian: audits.** SSO-policy accounts can legitimately show no Atlassian 2SV, and audits should account for this. This comes from a third-party hardening guide. — [howtoharden.com](https://howtoharden.com/guides/atlassian/)
- **Notion linking.** Google and Microsoft login work only if the provider's email matches the Notion account email, so the match is made by email. Apple "Hide My Email" creates a new account that cannot be linked to an existing one. — [Notion: Log in and out](https://www.notion.com/help/log-in-and-out)
- **Calendly linking.** One login method is active at a time. Switching it needs a 6-digit email code. Users removed from an SSO org go back to their original method, such as Google. — [Calendly: Update login preferences](https://calendly.com/help/how-to-update-your-login-preferences); [Calendly: SAML SSO overview](https://help.calendly.com/hc/en-us/articles/360056867653)
- **GitHub linked identities.** GitHub records an explicit link from a personal account to an IdP identity for each org. Logging in with an identity already linked to another account fails. Relinking warns that access to teams and orgs may be lost. PATs and SSH keys must be authorized separately for SSO orgs. — [GitHub: About authentication with SAML SSO](https://docs.github.com/en/authentication/authenticating-with-saml-single-sign-on/about-authentication-with-saml-single-sign-on)

**Inferences:**

- Tetsu can follow the pattern: a per-business "require MFA or passkey" setting, which is skipped when the business uses SSO that hands MFA to the IdP. It needs an admin path to recover locked-out staff, and owners exempt from lockout.
- Linking by verified email (the Notion model) is the easiest for users. The section 6 incident shows its risk when the IdP's email is not stable. Linking by stored `sub` plus a verified email, with explicit confirmation when a new method is attached (as Calendly does with an email code), is safer.

**Gaps:**

- Linear's MFA enforcement for workspaces was not found. It seems to rely on passkeys and SAML rather than TOTP, but this is unconfirmed.
- Figma's 2FA enforcement was not fetched.
- Calendly's 2FA options were not fetched.

## 4. Enterprise SSO and personal accounts

**Takeaway:**

There are three patterns. (a) The tenant owns its identities outright, as in GitHub EMU, which gives isolated accounts. (b) The tenant claims global accounts on its verified domains, as in Atlassian managed accounts, Figma domain capture, and Notion verified domains. (c) SSO is an extra check layered on a personal account, as in GitHub SAML linked identities and Slack or Linear per-workspace enforcement.

**Cited Findings:**

- **GitHub EMU.** The IdP controls usernames, generated by normalizing an IdP identifier, and profile data, which users cannot change. It also controls org membership. Users cannot collaborate outside the enterprise. Unauthenticated access shows a 404 unless admins turn on a redirect to SSO. — [GitHub: About EMU](https://docs.github.com/en/enterprise-cloud@latest/admin/managing-iam/understanding-iam-for-enterprises/about-enterprise-managed-users)
- **GitHub SAML for an org.** Personal accounts need an active SAML session to reach org resources and to authorize OAuth apps. Enterprise owners can view and revoke linked identities, sessions, and credentials. — [GitHub: About authentication with SAML SSO](https://docs.github.com/en/authentication/authenticating-with-saml-single-sign-on/about-authentication-with-saml-single-sign-on)
- **Atlassian.** Verifying a domain and then claiming accounts gives full control: edit, deactivate, or delete. Unclaimed accounts stay external. When SSO is enforced, users missing from the IdP cannot log in, so Atlassian advises a separate policy without SSO for them. Guard allows several policies for external users. — [Atlassian: Managed accounts](https://support.atlassian.com/user-management/docs/what-are-managed-accounts/); [Atlassian: Authentication policy settings](https://support.atlassian.com/security-and-access-policies/docs/authentication-policy-settings-for-your-organizations/)
- **Figma.** SAML applies only to members on the verified domains. Guests use Google or a password "regardless of an organization's SAML SSO settings." — [Figma: Log in / SSO](https://help.figma.com/hc/en-us/articles/360040532333-Log-in-to-Figma)
- **Slack Grid.** Org owners choose whether SSO is required for all, required excluding guests, or optional. This comes from a third-party guide. — [nhimg.org](https://nhimg.org/slack-enterprise-security-guide)
- **Calendly.** Enforced SSO forces every user except the owner onto SSO. Email changes go through the IdP. — [Calendly: SAML SSO overview](https://help.calendly.com/hc/en-us/articles/360056867653); [Calendly: Update login preferences](https://calendly.com/help/how-to-update-your-login-preferences)

**Inferences:**

- For Tetsu, pattern (c) fits best, perhaps with (b) later: SSO as a per-business requirement on top of a global staff account. Requiring SSO for one business should not lock a person out of their other businesses. That means enforcement is checked when the person enters a business, as Notion does ("when you open a workspace"), not when they log in globally.
- If a business ever needs to own accounts outright, the EMU-style choice gives isolated accounts tied to the tenant. That runs against the global-staff model, so it is a separate decision.

**Gaps:**

- Slack Enterprise Grid domain claiming, and whether Grid gives one account across its workspaces, was not confirmed officially.
- Notion's enterprise claiming of accounts on verified domains was not fetched in detail.

## 5. Session scope across workspaces and subdomains

**Takeaway:**

Little is documented officially. What is documented shows sessions owned by the global user (Linear), with per-org policy checks layered on top: GitHub's SAML session for each org, Atlassian's session length by policy, and Slack Grid's org-level session duration.

**Cited Findings:**

- **Linear.** Logging out logs you out of all other sessions. Several accounts can be logged in at once, and switching between them needs no new login. — [Linear: Login methods](https://linear.app/docs/login-methods)
- **GitHub.** Access to an SSO org needs an active SAML session for that org, on top of the github.com session. — [GitHub: About authentication with SAML SSO](https://docs.github.com/en/authentication/authenticating-with-saml-single-sign-on/about-authentication-with-saml-single-sign-on)
- **Atlassian.** Authentication policies, with Guard, control session and mobile-session expiry for each user group. — [Atlassian: Authentication policy settings](https://support.atlassian.com/security-and-access-policies/docs/authentication-policy-settings-for-your-organizations/)
- **Slack Grid.** Session duration is set at the org level. — [Salesforce Trailhead](https://trailhead.salesforce.com/content/learn/modules/identity-management-in-slack/explore-access-and-security-controls)

**Inferences:**

- The two-layer model maps well to Tetsu. A global staff session covers the person. A per-business "step-up" or policy check covers entry to each business, such as SSO freshness or MFA age. Per-business session limits then apply without logging the person out of other businesses.

**Gaps:**

- Cookie and domain scoping for these products was not found in official docs. That includes Slack's per-workspace `*.slack.com` subdomains and Atlassian's `*.atlassian.net` sites versus `id.atlassian.com`. Background knowledge suggests a central identity domain with per-tenant subdomains, but this is unverified.

## 6. Security incidents and design lessons

**Takeaway:**

Two lessons matter directly for Tetsu. First, identifying users by email from Google OAuth lets a buyer of a lapsed domain take over its former accounts. That hits services that auto-join by domain. Second, incidental data leaks can come from broadcasting identity data over realtime channels.

**Cited Findings:**

- **Google OAuth and lapsed domains (published January 2025).** Truffle Security showed that buying a failed startup's domain let the buyer recreate former employees' Google accounts. With those, it logged into the ex-employees' accounts on Slack, Notion, Zoom, ChatGPT, and HR systems. The `sub` claim is the stated unique identifier, but one engineer reported it "changes in about 0.04% of logins". Google first marked the report "Won't fix", then reopened it and paid a bounty, with no fix at the time of writing. — [Truffle Security](https://trufflesecurity.com/blog/millions-at-risk-due-to-google-s-oauth-flaw)
- **The post's recommendations.** Google should add immutable user and workspace IDs. Providers should require extra checks before password resets. Providers that let anyone with a matching domain join a workspace should reconsider, because it exposes full user lists. — [Truffle Security](https://trufflesecurity.com/blog/millions-at-risk-due-to-google-s-oauth-flaw)
- **Linear's matching warning.** Linear tells admins to remove approved domains they cancel or transfer, which is the same risk. — [Linear: Invite members](https://linear.app/docs/invite-members)
- **Slack invite-link hash leak (2017 to 2022).** Creating or revoking a shared invite link broadcast the creator's salted password hash over a websocket to connected workspace members. The bug ran from April 17, 2017 to July 17, 2022, and Slack reset passwords for about 0.5% of users. — [Slack: Notice about password resets](https://slack.com/blog/news/notice-about-slack-password-resets); [BleepingComputer](https://www.bleepingcomputer.com/news/security/slack-resets-passwords-after-exposing-hashes-in-invitation-links/); [The Register](https://www.theregister.com/security/2022/08/08/slack-exposed-hashed-passwords-for-years/273695)

**Inferences:**

- Key OAuth identities on `(issuer, sub)`, not on email. Treat email as a contact attribute that must be verified again when linking. Do not auto-link by email from an IdP without the provider's verified-email claim, and preferably without a confirmation step.
- Avoid domain auto-join for Tetsu businesses, or require DNS verification and periodic re-verification (Figma uses a DNS TXT record).
- Serialize account data for realtime and API output from an allowlist, so secrets such as password hashes and token hashes can never ride along in events.
- Passwordless email codes plus passkeys, as Slack, Linear, and Notion offer, remove the password-hash class of risk entirely.

**Gaps:**

- I did not research or verify other relevant incidents: the 2022 theft of Slack's GitHub repositories and tokens, the 2022 theft of OAuth tokens issued to Heroku and Travis CI that were used against GitHub, the Okta 2023 support-system breach that touched SaaS customers, and Microsoft "nOAuth" (Descope 2023, mutable email claim in Entra ID). Each would reinforce the "key on immutable ID" lesson and is worth a follow-up.
- Whether Google has since shipped the immutable workspace or user ID claims requested in the Truffle post was not checked, so its status as of late 2026 is unknown.
