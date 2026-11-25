# How staff join and log in

Researched 2026-11-24 for [#111](https://github.com/heavymedl-labs/tetsu/issues/111)'s sub-slice 2, staff join and
log in, after the maintainer asked that each slice be checked against industry leaders before it is built. The
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
- **Sessions:** a staff session belongs to the person, as ADR-0048 has it, and switching business issues a new token
  (ADR-0040); with one business per person in this slice, there is nothing to switch yet.

## Gaps

- The pages were read through search summaries, not opened; Shopify's `/store/<handle>` path comes from a third-party
  guide, and Square's multi-business answers from its community.
- No booking competitor's staff invite or landing was found: Mindbody, Vagaro, Fresha, and Glofox document neither.
- No source compared invite lifetimes against ASVS's "short period"; the numbers above are product choices.
