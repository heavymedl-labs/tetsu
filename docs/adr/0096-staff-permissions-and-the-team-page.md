# 0096. Staff permissions and the Team page

- Status: Accepted
- Date: 2026-11-27

## Context

[#111](https://github.com/heavymedl-labs/tetsu/issues/111)'s sub-slice 4 adds the four staff roles of
[ADR-0048](0048-sign-in-design.md), with every check naming a permission, and lets a business invite its own staff
from the web rather than only through `tetsu staff invite` ([ADR-0089](0089-staff-join-and-login.md)). Today only
owners exist, and a staff member has only their own Profile, Security, and Preferences pages, which name no business.

The [research](../research/identity/staff-roles-and-team.md) found that studio software uses the same four roles under
other names; that a person grants only roles below their own and only owners make owners; that a business always keeps
an owner, and no one changes their own role; and that products naming the business in the address read the person's
role there on each request.

## Decision

- **A business's pages sit at `/<business>/settings/...` on the staff host,** as ADR-0094 planned, starting with
  `/<business>/settings/team`. Each request finds the business by its slug through Tenancy and reads the person's role
  there in that business's own transaction. A slug the person holds no role at answers as a page that does not exist,
  so it says nothing about the business.
- **Permissions are named in code,** one per check, with a fixed table from role to permissions:

  | Permission | Owner | Manager | Front desk | Instructor |
  | --- | --- | --- | --- | --- |
  | See the team | Yes | Yes | No | No |
  | Invite staff | Yes | Yes | No | No |
  | Cancel an invite | Yes | Yes | No | No |
  | Change a role | Yes | Yes | No | No |
  | Remove staff | Yes | Yes | No | No |

  Later checks, such as running bookings, add their own permissions as they are built. Each permission has a test
  that every role without it is refused.
- **A rank rule bounds what a permission reaches:** owner, then manager, then front desk and instructor alike. An owner
  acts on and grants any role. Anyone else acts only on people and invites whose role is below their own, and grants
  only such roles, so a manager invites, changes, and removes front desk and instructors only.
- **A business always keeps an owner.** The last owner can be neither demoted nor removed, checked in the database
  while the business's owners are locked, so two owners demoting each other at once cannot leave none. No one changes
  their own role or removes themselves; leaving a business waits for a later slice.
- **The Team page** lists the business's staff (name, email, role, joined) and its open invites (email, role, when it
  ends), with forms to invite, change a role, remove someone, and cancel or resend an invite. Removing someone also
  cancels their invite there; it needs a confirmation page, and works without script. "Last seen" waits until staff
  sessions record a business.
- **An invite from the web answers "Invite sent"** for every invite it does not refuse as a member here already,
  so the page reveals nothing an email has at another business, such as being disabled. It records an event in the
  business's own record, naming the actor, as ADR-0089's clarification foresaw, and sends the same email the command
  does. A new rate limit bounds the invites each staff member sends, since each sends an email.
- **Removing a role ends its access at once,** since every request reads the role (`v5.0.0-7.4.2`): the person's
  sessions remain for their own pages and their other businesses.
- **Roles show by name:** Owner, Manager, Front desk, Instructor. `tetsu staff invite --role` takes all four.
- **The sidebar gains a business group,** named for the business, holding Team, shown only to a role that may see the
  team. On a business's pages it names that business; on personal pages, the business the person joined first, until
  sub-slice 6's switcher.

## Alternatives considered

- **The business in the session, as Stripe keeps it.** Addresses stay short, but a link cannot name a business, and a
  removed role would need its sessions found and ended rather than refused on the next request.
- **A permission per role granted, such as "invite instructors".** Explicit, but four times the permissions for one
  rule, and a fifth role would multiply them again.
- **Front desk sees the team, read-only.** GitHub members see each other, but studio software keeps staff lists with
  managers, and front desk needs only the schedule (#158).
- **Answer "already staff elsewhere" or "disabled" to the inviter.** Helpful, but it tells one business about another.

## Consequences

- Identity gains a permission table and rank rule in its domain, staff store functions to list, change, and remove
  roles and cancel invites, and a migration granting what they need, under row security.
- Tenancy answers a business by its slug for Identity's pages, through the composition root, as it names one today.
- Sub-slice 6 adds the switcher and a new token on switching business, and decides how a session records its business.
- #158's Team page acceptance is met here, except "last seen".

## Sources

- [Staff roles and the Team page](../research/identity/staff-roles-and-team.md), 2026-11-27, with its sources:
  Glofox, Mindbody, Momence, Booksy, Fresha, Shopify, Mastodon, GitLab, Square, GitHub, Vercel, and Linear.
