# How staff join and log in

Researched 2026-11-24 for [#111](https://github.com/heavymedl-labs/tetsu/issues/111)'s sub-slice 2, staff join and
log in, after the maintainer asked that each slice be checked against industry leaders before it is built. A second
pass the same day asked how invites and memberships, owned by each business, are read before a business is known. The
identity model is settled elsewhere and not repeated here: one global staff identity with a membership and role in
each business ([commerce, payments, and booking realms](commerce-booking-realms.md);
[multi-tenant identity patterns](multitenant-identity-patterns.md)), sessions owned by the person with checks per
business ([SaaS realm models](saas-realm-models.md#5-session-scope-across-workspaces-and-subdomains)), and staff
stored apart from admins ([ADR-0088](../../adr/0088-admins-apart-from-staff.md)). This page covers what those left
open: where the business sits in an address, where staff land after logging in, how an invite works and how long it
lasts, and how an operator command creates a business's first owner. Sources were read through search summaries on
2026-11-24, and say so where they are a vendor's community rather than its documentation.

## Summary

Leaders put the business in the address's path and give each person a switcher at the top left: Shopify's
`admin.shopify.com/store/<handle>`, Linear's `linear.app/<workspace>`, and Vercel's `vercel.com/<team>`, which is
[ADR-0084](../../adr/0084-realm-routes.md)'s `/app/<business>`. After login a person lands in one business: Vercel's
default team, which they can change, and Linear signs into one workspace. Invites name an email, are revocable, and
last days, not hours: GitHub and Shopify 7, Stripe 10, Square 30, and Linear's emailed ones 90; an expired one is
sent again. Inviting someone who already has an account asks them to log in and accept, never to make a second
account, and Square's broken cases come from people who made one. Operator commands that create a privileged
account name the role as a flag: Mastodon's `tootctl accounts create <name> --email … --role Owner`, and
Discourse's `rake admin:create`. Tetsu's one conflict is the invite's life: ADR-0040 asks for short-lived links, and
admins' last an hour, while a business owner's staff need days.

## The business is in the address, with a switcher

**Takeaway:**

Every leader read names the business in the path, so a link to a page names its business, and a person in two
businesses moves between them through a switcher, not by logging in again.

**Cited Findings:**

- Shopify's admin moved to one domain, `admin.shopify.com`, replacing each store's `myshopify.com` address
  ([Shopify changelog](https://changelog.shopify.com/posts/shopify-admin-now-lives-at-admin-shopify-com)), with the
  store's handle in the path, `admin.shopify.com/store/<handle>`, according to a third-party guide; a person switches
  stores from the store's name in the admin ([Shopify Help Center: switching between stores](https://help.shopify.com/en/manual/your-account/shopify-admin)).
- Each Linear workspace has its own address, `linear.app/example`, and a person switches from the workspace's name at
  the top left ([Linear: conceptual model](https://linear.app/docs/conceptual-model);
  [Linear: workspaces](https://linear.app/docs/workspaces)).
- A Vercel team's address is `vercel.com/<slug>`, chosen from a switcher at the top left of the navigation
  ([Vercel: account management](https://vercel.com/docs/accounts)).
- Stripe switches accounts from the account's name at the top left; its documentation gives no account in the
  Dashboard's address ([Stripe: multiple accounts](https://docs.stripe.com/get-started/account/multiple-accounts)).
  Square has no switcher; a person in two businesses signs out of one, by its community's account
  ([Square Community](https://community.squareup.com/t5/Archived-Discussions-Read-Only/I-own-2-companies-I-need-to-switch-accounts-in-Dashboard-without/m-p/397498)).

## After logging in, a person lands in one business

**Takeaway:**

Logging in opens one business, not a list: the one the person chose as default, or the one they logged in to. Only
someone in several businesses ever sees a choice, and then through the switcher.

**Cited Findings:**

- Vercel's default team "controls which team you land on at login or when you open the dashboard"; the first team a
  person creates becomes it, they change it in their settings, and leaving it picks another
  ([Vercel: account management](https://vercel.com/docs/accounts)).
- Logging in to Linear means signing in to one workspace ([Linear: conceptual model](https://linear.app/docs/conceptual-model)).

## An invite names an email, lasts days, and asks an existing account to accept

**Takeaway:**

Invites are sent to an email by someone allowed to invite, can be cancelled before they are used, and last 7 to 90
days; an expired one is sent again rather than extended. A person who already has an account accepts by logging in,
and the platforms that let a second account be made for the same person are the ones whose users report broken
invites.

**Cited Findings:**

| Platform | An invite lasts | Source |
| --- | --- | --- |
| GitHub | 7 days; owners cancel or edit it before it is accepted; 50 a day for a new organization | [GitHub Docs](https://docs.github.com/en/organizations/managing-membership-in-your-organization/inviting-users-to-join-your-organization) |
| Shopify | 7 days; an expired one means removing and adding the person again | [Shopify Help Center](https://help.shopify.com/en/manual/your-account/staff-accounts/create-staff-accounts) |
| Stripe | 10 days; someone with a login logs in with the invited email and switches to the account | [Stripe: start a team](https://docs.stripe.com/get-started/account/teams); [Stripe Support](https://support.stripe.com/questions/invite-team-members-or-developers-to-access-your-stripe-account) |
| Square | 30 days; resent from the team member's profile | [Square: add and manage team members](https://squareup.com/help/us/en/article/8356-add-and-manage-team-members) |
| Linear | Emailed invites 90 days, by its changelog; a reusable link until reset | [Linear: invite members](https://linear.app/docs/invite-members); [Linear changelog](https://linear.app/changelog/2025-02-13-initiative-updates) |

- Square's moderators answer that a team member in two businesses accepts the second invite with their existing
  login, and that the broken cases come from people who made a new account instead
  ([Square Community](https://community.squareup.com/t5/Staff-Payroll/Team-Members-who-use-Square-for-multiple-businesses/m-p/720413)).

## An operator command names the role it creates

**Takeaway:**

Self-hosted platforms create their first privileged account from a command run on the server, with the role as a
flag on one account-creating command, and show its secret once, in the terminal.

**Cited Findings:**

- Mastodon's setup creates its owner with `tootctl accounts create <username> --email <email> --confirmed --role
  Owner`, printing a random password in the terminal; roles are named, such as Owner, Admin, and Moderator
  ([Mastodon: setup](https://docs.joinmastodon.org/admin/setup); [tootctl](https://docs.joinmastodon.org/admin/tootctl/)).
- Discourse creates an admin with `rake admin:create`, which asks for the email, and offers to grant admin to an
  account that already exists ([Discourse Meta](https://meta.discourse.org/t/create-an-admin-account-from-the-console/17274)).

## Invites and memberships belong to the business, with two narrow ways across

**Takeaway:**

Products built on row security keep invites and memberships as the business's own rows, and open exactly two paths
across businesses: a function that finds one invite by its token, and a person's own memberships, keyed on who is
logged in. Tetsu's [ADR-0042](../../adr/0042-tenant-isolation.md) already allows both in words: lookups before a
business is known go through "one narrow `SECURITY DEFINER` function each", and "which businesses can I work at" is
named as one. Its catalog check does not yet allow them: a tenant-owned table may hold only the two tenant policies
for its module's role, so a lookup role, whose functions run with row security forced, finds nothing there.

**Cited Findings:**

- Basejump, a multi-tenant starter for Supabase, keeps `basejump.invitations` under its account (`account_id`), with
  row security on, owners alone able to read, make, and delete them, and invites lasting 24 hours from creation
  ([Basejump invitations migration](https://raw.githubusercontent.com/usebasejump/basejump/main/supabase/migrations/20240414162100_basejump-invitations.sql)).
- Its `lookup_invitation` and `accept_invitation` are `SECURITY DEFINER` with a fixed `search_path`, each taking one
  token; accepting inserts the membership, deletes a one-time invite, and answers "already a member" from the
  membership's key (same source).
- Its memberships, `basejump.account_user`, have row security, with a policy that lets a person read their own rows,
  `user_id = auth.uid()`, across every account, and no policy that lets anyone insert them; only definer functions do
  ([Basejump accounts migration](https://raw.githubusercontent.com/usebasejump/basejump/main/supabase/migrations/20240414161947_basejump-accounts.sql)).
- AWS's walkthrough puts row security on its tenant-to-user table too, rather than leaving the mapping open
  ([AWS Database Blog](https://aws.amazon.com/blogs/database/multi-tenant-data-isolation-with-postgresql-row-level-security)).
- Nile's `users.tenant_users` keeps memberships tenant-aware beside a global `users.users`, and Tetsu's earlier
  research recommended a definer function keyed on the logged-in account for a person's list of businesses
  ([global tables](../tenant-isolation/roles-global-tables-neon.md#accounts-that-span-tenants-global-users-with-per-tenant-memberships)).

## Recommendations for sub-slice 2

- **Routes, after ADR-0084:** `/app/login`, `/app/join`, `/app/settings/profile`, and `/app/settings/security` for
  the person, with the business only in `/app/<business>/...` once there are business pages (#158). `/app` sends a
  logged-in person to their Profile until then, as `/admin` does (ADR-0086), and later to their default business.
- **Data, after ADR-0048 and ADR-0088:** a global `staff` table with its own passkeys and sessions, and a
  membership per business, `(staff, tenant, role)`, owned by the tenant. The first slice stores only owners; the
  other roles and their checks are sub-slice 4.
- **The command:** `tetsu staff invite <business> <name> <email> --role owner`, after Mastodon's one command with a
  role flag, printing the link to a terminal and emailing it, as `tetsu admin create` does. Inviting an email that
  is already a staff member's adds the membership and asks them to log in and accept, never to make a second
  identity (ADR-0040).
- **Invite life: a decision for the maintainer.** Leaders give 7 days or more, and a business owner's staff may not
  open their email within the hour admins' links last. ADR-0040 asks for short-lived links (`v5.0.0-6.4.1`) without
  a number. Seven days, single use, and cancellable matches GitHub and Shopify, and keeps a stolen link's window
  bounded; an hour, as for admins, keeps the window smallest but makes owners resend often.
- **Invites and roles stay tenant-owned, as ADR-0089 has them.** The catalog check gains what ADR-0042's words
  already allow: a tenant-owned table may also hold read-only policies for its own module's lookup role, whose
  functions each take one input, so the invite lookup finds one invite by its link's hash, and a later function finds
  one person's businesses. Writes stay with the module role in the business's own transaction: joining reads the
  invite, then adds the role and spends the invite where `app.tenant_id` names its business.
- **Sessions:** a staff session belongs to the person, as ADR-0048 has it, and switching business issues a new token
  (ADR-0040); with one business per person in this slice, there is nothing to switch yet.

## Redeeming an invite binds it to one account

A second pass, on 2026-11-25, asked how redeeming works once the invite is found: who may accept it, what accepting
proves, and how a one-time link survives mail scanners.

**Takeaway:**

An emailed invite is accepted only by the account it names: a new person proves the email by redeeming the link, and
someone who already has an account logs in as that account first. A mismatch should refuse without spending the
invite, which Auth0 does not do. Scanners follow links in mail, so opening a link must spend nothing; only a person's
explicit action may.

**Cited Findings:**

- Auth0 requires the accepting account's email to match the invited one, and marks it verified on acceptance; an
  invite opened while logged in as another account is spent without adding anyone, and the fix is a new invite
  ([Auth0: invite members](https://auth0.com/docs/manage-users/organizations/configure-organizations/invite-members);
  [Auth0 Support](https://support.auth0.com/center/s/article/auth0-organization-invitation-disappears-without-being-accepted)).
- GitHub lets an email invite be accepted only by an account with that verified email
  ([GitHub Docs](https://docs.github.com/en/organizations/managing-membership-in-your-organization/inviting-users-to-join-your-organization)).
- Clerk accepts an invite by a new sign-up, whose email the invite verifies, or by an existing user's sign-in with the
  same ticket ([Clerk: accept organization invitations](https://clerk.com/docs/guides/development/custom-flows/organizations/accept-organization-invitations)).
- Outlook Safe Links and Mimecast open links in mail before the person does, spending one-time tokens a GET redeems;
  the fixes are a page whose button redeems it, or a code ([Descope: email scanners](https://docs.descope.com/flows/use-cases/email-scanner);
  [Supabase discussion](https://github.com/orgs/supabase/discussions/41618)). Tetsu's join link already keeps its
  token in the address's fragment, which no browser sends, and redeems it only from the page's own POST.

## Recommendations for redeeming

- **One account per invite, chosen when it is issued.** `tetsu staff invite` names a staff member by email: an
  existing one, or a new one it creates without a passkey. The invite holds that `staff_id`, so nothing at redemption
  chooses an account.
- **A new person joins as an admin does:** the link's page registers their first passkey and starts their session,
  which proves the email (as Auth0 and Clerk do).
- **Someone with a passkey logs in, then accepts:** the page asks them to log in, and accepting checks that the
  session's staff member is the invite's. A session of anyone else is refused, and the invite stays unspent.
- **All in one transaction, in the business's own scope:** spending the invite comes first, by deleting it where
  `app.tenant_id` names its business, so a second tab finds nothing; the passkey and session go through the staff
  functions; then the role is added. Any failure undoes all of it.
- **The same routes as admins:** `/app/join`, its token in the fragment, after ADR-0084.

## Issuing an invite: replacing one, and what its email says

A third pass, on 2026-11-25, asked what `tetsu staff invite` must do beyond creating the invite: what a second
invite to the same person does, and what the email may carry.

**Takeaway:**

Leaders have no resend that revives an old link: a new invite replaces the pending one, and the old link stops
working. The email names the business, which the business chose, so that name is the one piece of text an attacker
can steer, and leaders' worst invite incidents came from it.

**Cited Findings:**

- GitHub lets owners edit or cancel a pending invite until it is accepted, and lists no resend; its community's
  answer is to cancel and invite again
  ([GitHub Docs](https://docs.github.com/en/organizations/managing-membership-in-your-organization/canceling-or-editing-an-invitation-to-join-your-organization);
  [GitHub Community](https://github.community/t/how-do-i-resend-a-pending-invitation/2203)).
- Shopify has no resend for staff invites; removing the pending invite and adding the email again sends a new link
  and ends the old one, by its community's account
  ([Shopify Community](https://community.shopify.com/t/how-can-i-resend-staff-account-invites-on-my-platform/28530)).
- Clerk's invites are pending, accepted, or revoked, each made with an email and a role, and revocable
  ([Clerk: invitations](https://clerk.com/docs/guides/organizations/add-members/invitations.md)).
- Attackers named OpenAI organizations with links and phone numbers, so its own invite emails carried them, and
  passed the receiving servers' checks ([BleepingComputer](https://bleepingcomputer.com/news/security/cybersecurity-firms-targeted-by-fraudulent-openai-organization-invites)).
  OWASP calls user text placed in an application's mail content spoofing, a risk even when it is escaped
  ([OWASP: content spoofing](https://owasp.org/www-community/attacks/Content_Spoofing)), and Tolgee shipped an
  organization name unescaped into its invite email
  ([GHSA-gx3w-rwh5-w5cg](https://github.com/tolgee/tolgee-platform/security/advisories/GHSA-gx3w-rwh5-w5cg)).

## Recommendations for issuing

- **A new invite replaces the old,** as `staff_invites`' one row per person and business already makes it, and ends
  any join begun from the old link. There is no resend.
- **The email names the business and the role as escaped text, never inside a link.** Escaping stops markup, not a
  URL a mail reader turns into a link, so the name is trusted only while the operator names businesses; before owners
  name their own, business names must refuse URLs.
- **An existing email keeps its staff member's name,** since only the person changes it (ADR-0040), and the command
  says so when the name given differs.
- **Someone who already holds a role at the business is issued nothing,** since changing a role is sub-slice 4's.
- **A disabled staff member is issued nothing,** as for admins.

## Gaps

- The pages were read through search summaries, not opened; Shopify's `/store/<handle>` path comes from a third-party
  guide, and Square's multi-business answers from its community.
- No booking competitor's staff invite or landing was found: Mindbody, Vagaro, Fresha, and Glofox document neither.
- No source compared invite lifetimes against ASVS's "short period"; the numbers above are product choices.
