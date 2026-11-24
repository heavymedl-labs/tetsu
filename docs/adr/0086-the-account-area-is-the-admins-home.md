# 0086. The account area is the admins' home

- Status: Proposed
- Date: 2026-11-24
- Supersedes [ADR-0084](0084-realm-routes.md)'s admin home, which waited on
  [#157](https://github.com/heavymedl-labs/tetsu/issues/157)

## Context

[ADR-0084](0084-realm-routes.md) sends `/admin` to Profile "until operators have a home", and left what that home shows
to #157. The research suggested a home of "needs you" items, the previous login, and recent activity, in an app
shell whose sidebar the settings pages leave through "Back to app"
([Account area and dashboards](../research/identity/account-area-and-dashboards.md#what-each-realms-home-shows)).

Before building it, the maintainer asked whether it is needed. Logging in already lands on Profile, which the
maintainer chose over a landing page. Of what Tetsu holds today, only events that stopped retrying and floods of
refused requests need an admin, both are rare, and neither can be acted on from a page yet. A home would mostly say
that nothing needs you, one click before the page the admin came for.

Linear keeps workspace administration as a group in its settings sidebar, below the personal pages, rather than in
the app. Admins are a small team doing occasional work at desks, which that shape fits.

## Decision

- **The account area is the admins' home.** `/admin` sends a logged-in admin to Profile, as logging in and joining
  do, for good rather than until a home exists.
- **Admin work joins the Platform group** of the settings sidebar as pages of its own, as the mailbox did. Businesses,
  admins, or events that stopped retrying each become a page there when a page can act on them.
- **No "Back to app" for admins,** as there is no app to go back to.
- **An app shell with a home of its own is for realms whose work centres on one view,** such as staff's calendar and
  a customer's next booking. Their sign-in slices decide theirs ([ADR-0048](0048-sign-in-design.md)).

## Alternatives considered

- **A home with "needs you", the previous login, and recent activity (#157).** It follows the research and Zendesk's
  agent home, but today it would be nearly always empty, adds a click before Profile, and reads Audit and the event
  outboxes, which needs a port across modules for little gain.
- **A home holding only the previous login.** Cheap, and a real security cue, but one line does not need a page. The
  Security page is its place, once logins are listed there.

## Consequences

- #157 closes as not planned, and nothing is built or changed but comments.
- The Platform group grows one page at a time, and the settings shell stays the only shell for admins.
- If admins gain work that centres on one view, a new ADR brings back a home.

## Sources

- [Linear: Members and roles](https://linear.app/docs/members-roles), where admins see an Administration group in
  settings, from the research's
  [account of Linear's settings](../research/identity/account-area-and-dashboards.md#linear-runs-one-shell-in-two-modes-app-and-settings).
- [Account area and dashboards](../research/identity/account-area-and-dashboards.md#every-useful-home-leads-with-todays-work-not-charts),
  for what homes in other products lead with.
