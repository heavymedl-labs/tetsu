# How commerce, payments, and booking platforms separate platform, merchant-staff, and end-customer identities

Researched 2026-11-11 for Tetsu. Platforms covered: Shopify, Stripe, Square, Toast, Mindbody, Vagaro, Glofox, Booksy. I made about 20 tool calls, so coverage is uneven. Shopify, Stripe, Mindbody, Glofox, and Booksy are documented best. Square, Toast, and Vagaro are thinner. The Gaps sections say what is missing.

## Merchant and staff side: owner signup, staff invites, roles, staff at several merchants, MFA

### Takeaway

Every platform gives staff their own login and grants roles per merchant. Most let one person hold memberships in several merchants. Stripe sets a role per account. Shopify keeps outside agencies on a separate "collaborator" track that cannot hold the admin role. Square links one Team login to several employers, though users report this breaks often. MFA is usually optional, and the merchant can make it mandatory for its team. Shopify is the exception: collaborators must always have it.

### Cited Findings

- **Shopify roles:** "A role represents a staff member's job, and contains all the granular permissions that a user requires to do that job." Roles are managed in Settings > Users > Roles. A user can hold several roles and gets the combined permissions of all of them. — [Shopify Help: Roles](https://help.shopify.com/en/manual/your-account/users/roles/collaborator-accounts)
- **Shopify collaborators:** "Collaborators are Shopify Partners who you've allowed to access your store or organization." They must turn on two-step authentication or they cannot log in to the store. — [Shopify Help: Collaborator accounts](https://help.shopify.com/manual/your-account/staff-accounts/collaborator-accounts)
- **Shopify collaborator limits:** Collaborators cannot get the Administrator store role or organization roles. They cannot use Shopify POS. Ownership cannot be transferred to them. They do not count toward the store's user limit. Access expires after 90 days without a login, and the owner can reactivate it. When removed, their name stays in activity logs and order timelines. — [Shopify Help: Collaborator accounts](https://help.shopify.com/manual/your-account/staff-accounts/collaborator-accounts)
- **Shopify agencies:** Only someone with a Shopify Partner account can send a collaborator request. A Partner organization whose app is installed on the store can request access without a code. An agency that already has a staff account in the store may need it removed first. — [shopify.dev: Collaborator accounts](https://shopify.dev/docs/storefronts/themes/tools/collaborator-accounts); [Shopify Partners blog](https://www.shopify.com/partners/blog/shopify-collaborator-account)
- **Shopify Plus organizations:** Organization owners and Organization administrators manage users across all stores in the organization. — [Shopify Help: Adding users in Organization Settings](https://help.shopify.com/manual/shopify-plus/users/user-access.md)
- **Stripe roles per account:** "Stripe supports distinct Dashboard roles for each Stripe account. This allows team members to have the appropriate permission for each account they have access to." — [Stripe Docs: User roles](https://docs.stripe.com/dashboard/teams/roles); [Stripe Docs: Organizations team](https://docs.stripe.com/get-started/account/orgs/team)
- **Stripe MFA:** Only an organization Administrator or Super Administrator can require two-step authentication for every team member. Members who lack it must set it up at their next login. New invitees must set it up before they can join. — [Stripe Support: Require two-step authentication for your team](https://support.stripe.com/questions/require-two-step-authentication-for-your-team)
- **Stripe high-risk actions:** "Your account must require two-step authentication in order to allow non-Administrators with this role to transfer funds." — [Stripe Docs: User roles](https://docs.stripe.com/dashboard/teams/roles)
- **Stripe shared logins:** Stripe advises against sharing one login. Each user should have their own credentials and their own two-step device. An account owner can reset a member's two-step authentication from Team settings. A locked-out user can also submit a recovery form. — [Stripe Support: Two-step authentication requirement](https://support.stripe.com/questions/two-step-authentication-requirement)
- **Stripe SSO:** When SSO is configured, team roles are updated through the identity provider. — [Stripe Docs: SSO](https://stripe.com/docs/dashboard/sso)
- **Stripe support messages:** By default, only Administrators can see support messages, and access can be extended to the team. — [Stripe Support: Grant access to support messages](https://support.stripe.com/questions/grant-access-to-support-messages-to-your-team)
- **Square staff accounts and MFA:** Square recommends a separate employee account for each staff member, then turning on business two-step verification. Owners and Full Access team members control the setting. When it is on, it applies to every team member. Asked whether it can be required for only some members, Square answers "Not at this time." Methods are SMS or an authenticator app, and a device can be remembered for 90 days. Sensitive changes and high-risk actions also ask for two-step verification. — [Square Help: 2-Step Verification](https://squareup.com/help/us/en/article/5593)
- **Square escape hatch (an oddity):** Even after the team requirement is turned on, members "will now see an option to Disable two-step verification for themselves." — [Square Help: 2-Step Verification](https://squareup.com/help/us/en/article/5593). This is my reading of the snippet and is not verified in full context.
- **Square, one person at several businesses:** A Square Champion on the community forum says one email works across employers. The employee signs into their existing Team account and is offered each company they work for. — [Square Community: Team Members who use Square for multiple businesses](https://community.squareup.com/t5/Staff-Payroll/Team-Members-who-use-Square-for-multiple-businesses/m-p/720413)
- **Square, where it fails:** Users report "email already has an account" errors when an invite leads them to create an account instead of logging in. Support has told them to use a different email or contact the previous employer. — [Square Community: Cannot onboard employee](https://community.squareup.com/t5/Staff-Payroll/Cannot-onboard-employee-bec-email-is-associated-to-other-Square/m-p/745805); [Square Community: New Team Member with another employer](https://community.squareup.com/t5/Staff-Payroll/New-Team-Member-who-uses-Square-Payroll-with-another-Employer/m-p/775130)
- **Square email changes:** Owners cannot edit a team member's login email, so changing it needs Square Support. — [Square Community: Why can't my client remove me?](https://community.squareup.com/t5/Staff-Payroll/Why-can-t-my-client-remove-me/m-p/793202/highlight/true)
- **Toast MFA:** Toast documents MFA setup for merchant admin users, including authenticator apps. — [Toast Support: Set Up Multi-Factor Authentication](https://support.toasttab.com/article/Setting-Up-Multi-Factor-Authentication); [Toast Platform guide: authenticator app](https://doc.toasttab.com/doc/platformguide/intAdminMfaConfiguringQrCode.html)

### Inferences

- Every platform uses the same pattern: one global staff identity holds a membership in each merchant, and each membership carries its own role. Stripe and Square document this directly. Shopify does too through organizations and collaborators. The bugs Square users report are a lesson for Tetsu. Inviting an existing user should mean "sign in and accept." It should never mean "create an account" when that email is already taken.
- Shopify keeps third parties (collaborators) separate from staff. Collaborators have mandatory 2FA, lose access after 90 days idle, cannot be admins, and cannot take ownership. This is a good model for "outside helper" access in Tetsu.
- Stripe and Square both let the merchant require MFA for its whole team, and Stripe also gates money movement behind MFA. Requiring step-up MFA for payouts and other sensitive actions is common practice.

### Gaps

- Owner signup flows, meaning how a business account is created, were not researched for any platform.
- Square's official doc on one login across several businesses was not found, only forum posts. Square's permission sets and its per-location access are also undocumented here.
- I found nothing on staff roles or multi-business staff for Mindbody, Vagaro, Glofox, or Booksy. The same goes for Toast's employee login (which uses a PIN on the POS).
- Whether Shopify requires 2FA for store owners or staff, rather than only for collaborators, was not confirmed.

## Customer side: per-merchant or platform-wide accounts, login methods, guardians and minors, booking without an account

**Takeaway:**

There is a spectrum. At one end, Shopify customer accounts are scoped to each store. In the middle, Toast uses one guest account across its online-ordering sites. At the far end are marketplace accounts: Booksy, Vagaro, and since mid-2025 Mindbody's "Universal Account." Mindbody's move from per-studio profiles to a universal account caused sync and login trouble. Operators objected that the platform treats studio clients as its own. Glofox and Booksy both support guardians booking for children or other people. In Glofox, child accounts cannot log in at all.

**Cited Findings:**

- **Shopify login:** New customer accounts log in by email: customers "enter their email address and receive a one-time 6-digit verification code." "A password isn't required to sign in." Google and Facebook login are available. If Shop Pay is on, customers can use "their Shop credentials and saved passkeys." Shopify Plus stores can plug in an external identity provider, which "replaces the default sign-in experience." Sessions last up to 365 days. — [Shopify Help: New customer accounts](https://help.shopify.com/en/manual/customers/customer-accounts/new-customer-accounts)
- **Shopify Customer Account API:** The API uses OAuth 2.0 authorization code flow, and public clients must use PKCE. It is configured per storefront, and mobile callback URLs carry a shop identifier (`shop.{shop_id}`). The API requires the new customer accounts. — [shopify.dev: Customer Account API getting started](https://shopify.dev/docs/storefronts/headless/building-with-the-customer-account-api/getting-started)
- **Shopify customer profiles** are created and merged inside each store's admin. — [Shopify Help: New customer accounts](https://help.shopify.com/en/manual/customers/customer-accounts/new-customer-accounts)
- **Toast Mobile Order & Pay:** A guest enters a one-time passcode sent to their phone, which creates a Toast Account with saved payment details. — [Toast Support: Guest Experience for Toast Mobile Order & Pay](https://support.toasttab.com/en/article/Guest-Experience-for-Toast-Mobile-Order-Pay)
- **Toast Online Ordering:** Guests set an email and password, then get a verification email. One account works across all Toast Online Ordering sites. — [Toast Support: Create an Online Ordering Account](https://support.toasttab.com/en/article/Creating-an-Online-Ordering-Account); [Toast Guest Help: What is a Toast account](https://guesthelp.toasttab.com/en/articles/11583489-what-is-a-toast-account)
- **Toast Click to Pay:** It sends a one-time passcode by email or SMS. Toast articles disagree on which ordering channels support it. — [Toast Support: Click to Pay](https://support.toasttab.com/en/article/Using-Click-to-Pay-with-Toast-Mobile-Order-Pay)
- **Toast loyalty:** Toast's own loyalty program asks a guest who is not logged in to sign up for an online-ordering account and a loyalty account using their checkout email. — [Toast Support: Third-Party Loyalty](https://support.toasttab.com/article/Toast-Online-Ordering-Pro-Third-Party-Loyalty); [Toast: Enroll guests in Toast Loyalty](https://intercom.help/toast-central/en/articles/9326533-enroll-guests-in-toast-loyalty)
- **Mindbody Universal Account, launch:** A studio's notice says "as of July 31, 2025, Mindbody, our booking system provider, introduced a Universal Account system." It adds that "many clients have encountered temporary issues signing in if their studio accounts and universal accounts aren't synced." — [Bay Studios notice](https://bay-studios.com/mindbody-universal-account-update/)
- **Mindbody Universal Account, linking:** Clients sync at account.mindbodyonline.com, or add the studio under "Places You Go" using the same email and password as their studio account. Creating a studio profile at clients.mindbodyonline.com "automatically links to your Mindbody account," and the studio sends a verification email. Passes, credits, and history carry over. — [Bay Studios notice](https://bay-studios.com/mindbody-universal-account-update/)
- **Mindbody before 2025:** Clients had separate logins per studio and had to "logout and login to a different business." — [Harvard Digital Initiative student post](https://d3.harvard.edu/platform-digit/?p=165). Reddit members complained that the app "forces a separate login per studio." — [Vibefam summary](https://vibefam.com/mindbody-reviews-reddit-2026/), citing [r/yoga thread](https://www.reddit.com/r/yoga/comments/1mgo5sh/why_does_mindbody_create_separate_accounts_for/)
- **Mindbody account FAQ (third party):** A Mindbody account gives a single login for all the businesses a client uses. — [Gymdesk: FAQs About Mindbody Accounts](https://gymdesk.com/blog/faqs-about-mindbody-accounts); [Mindbody Support: Mindbody Account on the Client Info screen](https://support.mindbody.io/s/article/MINDBODY-Account-on-the-Client-Info-screen)
- **Mindbody backlash (weak source):** A 2026 review roundup says the 2025 Universal Login rollout "tried to convert studio customers into Mindbody customers" and was "partially rolled back after public backlash." It quotes operators saying Mindbody believes studio customers are its customers. It also cites objections to marketplace commission on bookings the studio generated itself. — [Vibefam: Mindbody review 2026](https://vibefam.com/mindbody-review-pricing-features-pros-cons-2026/). This is a low-authority competitor blog, and its Reddit page did not repeat the rollback claim when I fetched it.
- **Mindbody family accounts:** Clients can add Family accounts to "book, buy, and manage their entire family's information from a single account." — cited via search summary of Mindbody material; primary page not fetched (see Gaps).
- **Glofox family accounts:** "the 'child account' will not be able to log in themselves, but the guardian can purchase memberships and book on behalf of the child." Glofox onboarding must enable the feature, and the studio sets a minimum age. Glofox gives no waiver template, so the studio writes its own guardian terms. Family accounts are not in every package. — [Glofox Support: How to Set up Family Accounts](https://support.glofox.com/hc/en-us/articles/46478887830164-How-to-Set-up-Family-Accounts); [Glofox blog, June 2019](https://www.glofox.com/blog/product-updates-at-glofox-june-2019)
- **Glofox member apps:** Members use a shared Glofox Member App or the studio's own Standalone (branded) App, plus a web portal. — [Glofox Support: Member App and Standalone App overview](https://support.glofox.com/hc/en-us/articles/46466949130900); [Glofox: Custom Branded App](https://www.glofox.com/features/custom-branded-app/); [Glofox Support: Member information in the Member App and Web Portal](https://support.glofox.com/hc/en-us/articles/46433363215636-Member-Information-in-the-Member-App-and-Web-Portal)
- **Booksy Family & Friends, setup:** The booker needs a Booksy account and a member profile. Bookings carry a relationship tag: friend, child, spouse, partner, or other relative. — [Booksy Biz blog: Family & Friends](https://biz.booksy.com/en-us/blog/family-friends-let-clients-schedule-appointments-for-loved-ones)
- **Booksy Family & Friends, use:** Clients can book for family, friends, or pets and pay for them. Both the booker and the person getting the service receive reminders. There is no limit on how many members a client can add. The business can turn off booking for non-members. — [Booksy Biz: Customer app](https://biz.booksy.com/en-ie/features/customer-app); [Booksy Biz blog](https://biz.booksy.com/en-us/blog/family-friends-let-clients-schedule-appointments-for-loved-ones)
- **Vagaro:** Customers create one account on the Vagaro app or website and book with "your go-to businesses," so it is marketplace-style. — [Comparably: Vagaro FAQs](https://www.comparably.com/companies/vagaro/faqs). Vagaro memberships can be shared with family or friends and used at several locations. — [Vagaro: Memberships](https://as-marketing-wus2.azurewebsites.net/pro/memberships) (an Azure mirror of Vagaro marketing, so lower confidence)

**Inferences:**

- Shopify customer accounts appear to be per store: profiles live in each store's admin, the API is configured per storefront, and callbacks are keyed by shop. The Shop app is a separate, platform-wide layer that customers opt into on top of the store account. It offers passkeys and a faster checkout. This "per-merchant account plus an opt-in platform wallet" split is the closest match to what Tetsu wants.
- Mindbody shows the risk of the opposite path. Merging per-studio profiles into a universal account caused sync failures. Linking also depended on matching email and password across both records. It reinforces Tetsu's rule that the customer belongs to the business.
- Glofox's design fits Tetsu's guardian case well: a dependent profile with no credentials, a studio-set minimum age, and the studio's own guardian waiver. Booksy's relationship tags and "book for others" are a lighter version of the same idea.
- Booking without an account: Toast's online-ordering flow lets a guest check out first and only offers an account afterwards. Booksy lets a business turn off booking for non-members. Neither is a clear "walk-in, no account" model. That model usually lives in the merchant-side POS or front desk, where staff create the client record.

**Gaps:**

- Mindbody's official announcement of the Universal Account and any primary source for its rollback were not found. The rollback claim rests on one competitor blog. Mindbody's family-account help page and its consumer login methods (passwords, Apple or Google login, codes) were not fetched.
- Whether Shopify customer accounts are strictly per store is inferred, not stated in a doc I read. The legacy-account deprecation timeline was not checked.
- I found no source on login methods (passwords, social, passkeys) for Vagaro, Booksy, or Glofox customers. The same is true for guardian and minor handling at Vagaro, Square, and Toast.
- Guest or no-account booking is undocumented for Square Appointments, Vagaro, Mindbody, and Booksy.

## Branding: customer login on merchant pages and custom domains

**Takeaway:**

Evidence is thin. Shopify shows branded customer-account pages. Glofox sells a fully branded standalone member app. Marketplace platforms (Booksy, Vagaro, Mindbody's consumer app) put their own brand on customer login.

**Cited Findings:**

- Shopify shows a "branded customer account orders page," and Plus stores can replace login with their own identity provider. — [Shopify Help: New customer accounts](https://help.shopify.com/en/manual/customers/customer-accounts/new-customer-accounts)
- Shopify documents market-aware authentication URLs for headless customer accounts. — [shopify.dev: Customer Account API](https://shopify.dev/docs/storefronts/headless/building-with-the-customer-account-api/getting-started)
- Glofox sells a custom-branded member app as an alternative to its shared Member App. — [Glofox: Custom Branded App](https://www.glofox.com/features/custom-branded-app/); [Glofox Support: overview](https://support.glofox.com/hc/en-us/articles/46466949130900)
- Mindbody's universal login runs on Mindbody's own domains (account.mindbodyonline.com, clients.mindbodyonline.com), not the studio's. — [Bay Studios notice](https://bay-studios.com/mindbody-universal-account-update/)

**Inferences:**

- Branded login and per-merchant accounts tend to go together, as with Shopify and Glofox standalone apps. Platform-branded login goes with marketplace accounts (Mindbody, Booksy, Vagaro). This matches Tetsu's goal: a per-business account on the business's own pages.

**Gaps:**

- I did not confirm whether Shopify customer-account pages can be served on the merchant's own custom domain (for example, account.store.com). I believe they can but did not verify it.
- Branding and custom domains for Square, Toast, Vagaro, and Booksy login were not checked.

## Platform support access to merchant accounts: consent, audit, scope

**Takeaway:**

Stripe has the clearest model. A Stripe employee sends an access request, and the merchant approves it. Access is temporary with an expiry the merchant can edit, it can be revoked, it shows in Security history, and every change is logged and auditable. Shopify gates third parties (agencies) behind a request code, owner approval, scoped roles, and a 90-day idle expiry. I found no primary doc on how Shopify's own support staff get access.

**Cited Findings:**

- **Stripe, consent:** Stripe employees ask for temporary access by sending an access request link, which the user must approve. Stripe "will never ask for your login credentials or two-step authentication device codes." — [Stripe Support: Grant Stripe employees access to your Dashboard](https://support.stripe.com/questions/grant-stripe-employees-access-to-your-dashboard)
- **Stripe, duration and revocation:** The expiry can be changed under Manage access, and access ends with Remove user. — [Stripe Support: Grant Stripe employees access](https://support.stripe.com/questions/grant-stripe-employees-access-to-your-dashboard)
- **Stripe, audit:** "all access to your Dashboard by Stripe employees is logged and any changes made are auditable by Stripe." Approvals appear in Security history. Scope is described only as "view your Dashboard," with no finer limits documented. — [Stripe Support: Grant Stripe employees access](https://support.stripe.com/questions/grant-stripe-employees-access-to-your-dashboard)
- **Shopify request code:** A merchant can require a 4-digit collaborator request code, found in the admin's Security section. "Only the Partners who you share the code with can request access to your store." Making a new code invalidates the old one. — [Shopify Help: Collaborator accounts](https://help.shopify.com/manual/your-account/staff-accounts/collaborator-accounts)
- **Shopify approval:** Only the store owner or a user with the Administrator role can view and grant requests. The owner gets an email and an admin notification, then accepts or rejects. Permissions can be limited to specific apps and channels. — [Shopify Help: Collaborator accounts](https://help.shopify.com/manual/your-account/staff-accounts/collaborator-accounts); [shopify.dev: Collaborator accounts](https://shopify.dev/docs/storefronts/themes/tools/collaborator-accounts)
- **Shopify, staff permission to approve (secondary):** Staff holding the "Manage collaborator requests" permission can also approve. — [Loop Returns help](https://help.loopreturns.com/en/articles/1918081). This is secondary and may be outdated.

**Inferences:**

- Tetsu could copy Stripe's design for platform-staff access to a business: a request the owner must approve, a time limit, revocation by the owner, a record the business can see, and a platform-side audit log. Shopify adds the idea of a merchant-held code that controls who may even ask.

**Gaps:**

- No primary source on how Shopify Support staff (not Partners) get into a store, or on Square's support access to seller accounts. Square support has historically worked by phone verification, which I did not verify.
- Whether Stripe support access is read-only or can make changes is not stated beyond "any changes made are auditable."

## Documented incidents and complaints

**Takeaway:**

I found no documented credential-stuffing or account-takeover incident for these platforms. The largest Mindbody security event is the 2018 FitMetrix exposure, caused by unsecured servers. The complaints about shared login concern ownership and confusion, not breaches.

**Cited Findings:**

- **2018 FitMetrix exposure:** FitMetrix, a Mindbody-owned subsidiary, left Elasticsearch servers with no password, exposing customer records. — [Gizmodo](https://gizmodo.com/mindbody-exposes-data-belonging-to-millions-of-fitmetri-1829698749); [SC Media](https://scworld.com/news/fitmetrix-data-exposed-on-unprotected-elasticsearch-servers); [DataBreaches.net](https://databreaches.net/2018/10/12/mindbody-owned-fitmetrix-exposed-millions-of-user-records-thanks-to-servers-without-passwords/); [Athletic Business](https://www.athleticbusiness.com/operations/media-technology/article/15155717/customer-records-exposed-by-gym-service-data-breach)
- **Size of the exposure:** One outlet reported it "could impact 113.5M users." The number of individuals affected is unclear. — [PYMNTS](https://www.pymnts.com/cybersecurity/2018/mindbody-fitness-customer-data-breach/)
- **Login failures after the Universal Account:** Studio clients had login failures after the July 2025 Universal Account change when their profiles were not synced. — [Bay Studios notice](https://bay-studios.com/mindbody-universal-account-update/)
- **Operator complaints:** Studio owners object that Mindbody treats studio clients as Mindbody customers. They also object to commission on bookings made through the Mindbody app by clients the studio found itself. — [Vibefam: Mindbody review 2026](https://vibefam.com/mindbody-review-pricing-features-pros-cons-2026/) (low-authority source)
- **Spam results:** Many search results titled "Stolen login details are dominating… recover your mindbody portal" are SEO spam and were excluded. — e.g. [docs.thebrandindustry.com](https://docs.thebrandindustry.com/news/stolen-login-details-are-dominatingclick-fast-to-recover-your-mindbody-portal-7041231)

**Inferences:**

- For Tetsu, the Mindbody lesson is mainly about ownership and user experience: who owns the client, and keeping one account linked to several studios in sync. It is not about security. Even so, a universal login means one leaked password opens every studio a client uses.

**Gaps:**

- No search was run for incidents at Shopify, Stripe, Square, Toast, Vagaro, Glofox, or Booksy. This includes, for example, Shopify customer-account phishing and Square seller account takeovers. This is a major gap.
- No primary press coverage of the 2025 Mindbody Universal Account backlash was found.
