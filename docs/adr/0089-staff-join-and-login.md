# 0089. Staff join and log in

- Status: Proposed
- Date: 2026-11-24

## Context

[#111](https://github.com/heavymedl-labs/tetsu/issues/111)'s sub-slice 2 lets staff join by invite and log in at
`/app`. [ADR-0048](0048-sign-in-design.md) sets their roles and sessions, [ADR-0040](0040-identity-and-sign-in.md)
their invites, [ADR-0084](0084-realm-routes.md) their addresses, and [ADR-0088](0088-admins-apart-from-staff.md) that
their storage stays apart from admins', with the login flow shared in code. What is left open: how long an invite
lasts, where a person lands, how the first owner is made, and how staff tables meet tenant isolation
([ADR-0042](0042-tenant-isolation.md)).

The [research](../research/identity/staff-join-and-login.md) found that leaders put the business in the address with a
switcher, land a person in one business, send invites that last 7 to 90 days and ask an existing account to log in
and accept, and make a first owner with one command that names the role.

## Decision

- **Staff are a realm,** `Realm::Staff`, at `/app`, with the cookie `__Host-tetsu-staff`, sessions of 12 hours and 1
  hour idle (ADR-0048), passkeys labelled "Tetsu staff · Name", and pages at `/app/login`, `/app/join`, and
  `/app/settings/profile` and `/app/settings/security`. `/app` sends a logged-in person to their Profile until #158
  gives each business a home, then to their default business.
- **Storage, apart from admins' (ADR-0088):**
  - `staff`, global, as ADR-0048 has staff identities: name, email unique across staff, user handle, and when
    disabled.
  - Staff passkeys and ceremonies in tables of their own.
  - `staff_roles`, tenant-owned (ADR-0042): `(tenant_id, id)`, the staff member, and a role, one per business.
  - Staff invites, tenant-owned, naming the business, the person, and the role. A link is read before any business
    is known, so it is found by its hash through a lookup function, as sessions are (ADR-0048).
  - Sessions stay one table, as its `realm` column was built for: a nullable key per realm, `admin_id` or
    `staff_id`, with a check that exactly one is set and matches the realm.
- **Invites last 7 days for staff,** single use and cancellable, after GitHub and Shopify. Admins' stay at an hour,
  since an admin reaches every business. ADR-0040's "short-lived" holds as a week at most, bounded by single use.
- **An invite to an email already a staff member's** adds the role on login and acceptance, never a second identity
  (ADR-0040).
- **The first owner comes from a command:** `tetsu staff invite <business> <name> <email> --role owner`, which
  resolves the business's address in the composition root, prints the link only to a terminal, and emails it, as
  `tetsu admin create` does. Only `owner` is offered until sub-slice 4 adds the other roles and their checks.

## Alternatives considered

- **Invites of an hour, as admins'.** The smallest window for a stolen link, but owners' staff often miss it, and
  every leader read gives days.
- **The business in every staff address now, `/app/<business>/settings/...`.** Leaders do so for business pages, but
  a person's own Profile and Security belong to them, not to one business, as GitHub's `/settings` and Vercel's
  account pages do.
- **Memberships in Tenancy's schema.** Keeps tenants' data in one module, but a module owns one schema (invariants),
  and a role is who may sign in where, which is Identity's.

## Consequences

- Identity gains a staff store implementing `LoginStore` with its own SQL, repeating much of the admins' by design
  (ADR-0088), and its first tenant-owned tables, with row security and the catalog checks ADR-0042 requires.
- The composition root resolves a business's address before the staff command reaches Identity, since modules meet
  only through use cases.
- Sub-slice 3 adds the site's "Log in", sub-slice 4 the other roles, and #158 the business home.

## Sources

- [How staff join and log in](../research/identity/staff-join-and-login.md), with its sources: Shopify, Linear,
  Vercel, Stripe, GitHub, Square, Mastodon, and Discourse.
