# 0093. "Log in" leads to staff login

- Status: Accepted
- Date: 2026-11-26
- Supersedes [ADR-0087](0087-one-log-in-button.md)'s remembered admin hint, and its routing of `/login` by that hint
- The header's link through `/login` superseded by [ADR-0094](0094-realm-paths-at-the-root-of-their-hosts.md): it
  links straight to staff login

## Context

ADR-0087 puts one "Log in" in the site's header, leading to `/login`, which sends a browser holding an admin hint to
admin login and every other to staff login. An admin asks for the hint at login, and it lives in a cookie of the
site's.

[ADR-0092](0092-a-service-per-realm-before-domains.md) gives each realm its own host: the site, admins, and staff
each on their own. On `onrender.com`, a public suffix, each host is its own site, so a cookie the admin host sets
never reaches the site's host, and `/login` cannot see the hint. ADR-0040 gives admins a domain of their own later,
so no shared domain would carry it then either. ADR-0087 left this case to be decided when admins moved hosts.

The [research](../research/identity/login-entry-points.md#a-back-office-is-reached-by-its-address) found that
platforms do not link their back office from the public site: people reach it by its address.

## Decision

- **The site header's "Log in" leads to `/login`, which redirects with a 303 to staff login,** on the staff host,
  always. Staff are the people the site serves who log in there.
- **Admins reach their login by their own host's address,** which `/admin` on that host leads to, and bookmark it.
- **No hint, no "Remember this browser for Log in", and no control to forget it.** Nothing about an admin is stored in
  a visitor's browser by the site.
- **Customers, and "Sign up", stay as ADR-0087 has them:** customers log in on their business's page, and sign-ups
  wait on #23 and the Level 2 review.

## Alternatives considered

- **Hand the hint across hosts:** the admin host sends a successful login with the box ticked through the site's
  host, with a signed single-use token, and the site sets its own hint. It works, but adds a token flow, a redirect,
  and a forgery surface, to spare one admin a bookmark.
- **A chooser page,** "staff or admin". ADR-0087 rejected it: a step for every staff member, and it advertises the
  admin login.

## Consequences

- #111's sub-slice 3 shrinks to the header's "Log in" and `/login`'s redirect, which reads the staff origin
  `TETSU_STAFF_ORIGIN` on the site's service.
- #111 drops its criteria for the remembered browser and for the Security page forgetting it.
- The admin login stays unlinked from the site; an address no one links protects nothing by itself, so every admin
  page still requires a session.

## Sources

- [Where each realm's login starts](../research/identity/login-entry-points.md), 2026-11-24, on back offices reached
  by their address.
- [Public Suffix List](https://publicsuffix.org/list/), which makes each `onrender.com` service its own site.
