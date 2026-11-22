# The signed-in experience

Research on 2026-11-18, after an operator asked, while testing sessions on the demo, what a logged-in operator should
see. The [lifecycle note](operator-lifecycles.md) covers a session's limits and what a page says at each one; this note
covers the rest of being logged in: how a person knows they are, and as whom, where their account lives, and what the
public site does meanwhile. Sources are industry patterns and Linear's app, read on a throwaway free-plan workspace.
[Account area and dashboards](account-area-and-dashboards.md) extends it to a shared shell, account area, and home
for every realm.

## Summary

Practice agrees on three pieces, and Tetsu's operators have one of them.

1. **An indicator on every logged-in page:** the person's initials or picture, usually top right, opening a short
   menu that names them and ends with Log out. ASVS 5.0.0 7.4.4 (L2) asks the same: "all pages that require
   authentication have easy and visible access to logout functionality." Tetsu has none; its one logged-in page, `/ops`,
   carries Log out on the page.
2. **An account page,** split into profile, security (passkeys and sessions), and preferences. Tetsu's `/ops` card is
   the start of one: the operator's name, their passkeys, and Log out. Sessions are [#145](https://github.com/heavymedl-labs/tetsu/issues/145).
3. **A public site that knows,** swapping "Log in" for "Open app". It is the one piece Tetsu's operators do not need.

## Practice

- **Where.** NN/g finds people look top right for "Log in" and "My account"
  ([utility navigation](https://nngroup.com/articles/utility-navigation/)). Apps whose first question is which
  workspace, such as Linear, put a workspace badge top left instead.
- **What the menu holds.** The person's name and email first, account settings, then Log out last, set apart by a
  divider ([SaaSFrame](https://www.saasframe.io/patterns/profile-menu), [Clerk](https://clerk.com/docs/users/user-button)).
  Small internal tools often skip the menu for a visible strip: Django's admin shows "Welcome, *name*. View site /
  Change password / Log out" to staff only, and never on its login or logout pages
  ([#22295](https://code.djangoproject.com/ticket/22295), [#26035](https://code.djangoproject.com/ticket/26035)).
  GOV.UK services put a plain "Sign out" link in the header, shown only when signed in
  ([One Login header](https://github.com/govuk-one-login/service-header), [HMRC](https://design.tax.service.gov.uk/hmrc-design-patterns/sign-out/)).
- **Sessions.** GitHub's Sessions page lists each signed-in device with "Revoke session"
  ([GitHub docs](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/viewing-and-managing-your-sessions)).
- **The public site.** Most products show the same cached page to everyone and let the browser pick "Log in" or "Open
  app", from a cookie that only says someone is logged in and proves nothing; the app checks the real session. GitHub
  documents such a `logged_in` cookie ([GitHub cookies](https://docs.github.com/en/site-policy/privacy-policies/github-cookies)).
  WordPress instead shows logged-in staff a toolbar on every public page
  ([show_admin_bar](https://developer.wordpress.org/reference/hooks/show_admin_bar/)).

## Linear's lifecycle

Read in the app on 2026-11-18. Linear's sign-up flow is a business creating a workspace, so it is the reference for
staff sign-up ([#23](https://github.com/heavymedl-labs/tetsu/issues/23)) more than for operators.

| Moment | What Linear does |
| --- | --- |
| Sign up | Google first, then "Create a workspace" (name, an address checked as it is typed, region), under a line naming the account in use with "Use a different email" |
| Welcome | Five skippable steps, each a form beside a blurred picture: profile, invite teammates, connect GitHub, connect Slack, email updates |
| Logged in, opens `/` or `/login` | Goes to the app |
| Logged in, opens a public page | The page, with "Open app" for "Log in" and "Sign up"; the cached HTML holds both, and the browser picks |
| In the app | The workspace's badge top left opens Settings, Invite and manage members, Switch workspace, and Log out |
| Switch workspace | The account's email as its heading, then its workspaces, "Create or join a workspace", and "Add an account" |
| Settings | A page of its own with "Back to app": personal pages first, then the workspace's |
| Profile | Picture, email, full name, title, username, and "Leave workspace" |
| Security and access | Sessions (device, "Current session", an approximate place, a Log out on each), passkeys, API keys, signing key, approved apps |
| Members | Name, email, role, teams, joined, last seen; filtered by member, pending invite, suspended, or left |
| Log out | No confirmation; lands on the public home, saying nothing |

## What it means for operators

Operators work for Tetsu, have no workspace, and are invited by command, so most of Linear's flow does not apply. What
does:

- **The indicator is the person.** With no workspace to name, the badge is the operator's initials, top right, and its
  menu names them as an operator.
- **Linear's member states match the operator lifecycle:** pending invite is invited, member is joined, suspended is
  disabled. A members page waits until there are enough operators to manage from a page rather than a command.
- **Logout lands on the login page,** as it does today. An operator who logs out is usually about to log in again or
  close the tab, and the login page already says why when a session timed out.
- **The public site stays the same for everyone.** Operators are few and know `/ops`, and a page that never depends on
  who asks can be cached (#100). A logged-in hint for a "Console" link remains easy to add later.
- **Sessions show a device, not a place.** ADR-0059 already sets a coarse device label; Linear's place comes from an
  IP lookup, which would be a new source of personal data.

## Recommendations

1. **A console header on every logged-in operator page** ([#148](https://github.com/heavymedl-labs/tetsu/issues/148)): the wordmark home at left; at right the operator's initials,
   opening their name, "Operator", Account, and Log out. It meets 7.4.4 for every operator page to come, and needs no
   new data, since the session already names the operator.
2. **The `/ops` card becomes the Account page,** in Linear's order: profile, then security. #145's sessions join it.
3. **Later, each on its own trigger:** profile editing when operators need it, a members page when there are enough
   operators, and the staff realm's own header and sign-up flow with #23.

## Gaps

- The patterns come from vendor and design-system documents and one app; no source tests an operator console's header
  with its users.
- Linear's docs say logging out ends every session, yet its Sessions page ends them one at a time; one throwaway
  account cannot show which a second device sees.
