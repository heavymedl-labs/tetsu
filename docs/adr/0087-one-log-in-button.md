# 0087. One "Log in" button, routed by a remembered realm

- Status: Accepted
- Date: 2026-11-24
- The remembered admin hint, and `/login`'s routing by it, superseded by
  [ADR-0093](0093-log-in-leads-to-staff.md): "Log in" always leads to staff login

## Context

Tetsu's site header offers "See plans" and no way to log in. Staff will need one with
[#111](https://github.com/heavymedl-labs/tetsu/issues/111), and the maintainer, an admin, has to remember
`/admin/login`, or `/admin`, which sends a logged-out visitor there. The maintainer asked for one button that serves
both.

The [research](../research/identity/login-entry-points.md) found that every business-facing booking site puts "Log in"
in its header, leading to the business's dashboard, and that only a marketplace, Fresha, shows a chooser. Customers
start from a business's booking page. One login page for every realm would undo
[ADR-0040](0040-identity-and-sign-in.md)'s passkey domains once Tetsu has domains, and an email-first page that
routes by realm would tell anyone which realm holds an address. Identity platforms also route by a choice the browser
remembers, which reveals nothing to anyone else. A hint that outlasts logout is a persistent preference cookie, which
the UK's ICO exempts from consent only when the person is told of it plainly.

## Decision

- **The site header shows "Log in", leading to `/login`,** beside "See plans". `login` is already a reserved name, so
  no business can take it.
- **`/login` only redirects,** with a 303, to the admin login when the browser holds the admin hint, and to the staff
  login, `/app/login` ([ADR-0084](0084-realm-routes.md)), otherwise. #111 builds staff login in the same change, so
  the default always exists.
- **Admins ask for the hint.** The admin login page offers "Remember this browser for Log in". A successful login with
  it ticked sets the hint; the admin's Security page can forget it. Nothing sets it otherwise, so it is a service the
  admin requested.
- **The hint is a cookie of one fixed value, `admin`,** `HttpOnly`, `Secure`, `SameSite=Lax`, on path `/`, lasting a
  year. It holds no identifier and grants nothing: `/login` maps it to a fixed page, never a URL it carries, and the
  passkey still decides who logs in.
- **Customers have no hint and no link from Tetsu's site.** They log in on their business's page.
- **"Sign up" waits** on [#23](https://github.com/heavymedl-labs/tetsu/issues/23) and the Level 2 review, which keep
  sign-ups off.

## Alternatives considered

- **A chooser page,** "staff or admin". Fresha's pattern, but it adds a step for every staff member to serve one admin,
  and it advertises the admin login.
- **One passkey-first page for every realm,** routing by whichever account the passkey belongs to. Works while the
  realms share one passkey domain (ADR-0061), but crosses the realm boundary on purpose and stops working when they
  split.
- **An email-first page that routes by realm.** Reveals which realm holds an address.
- **Setting the hint at every admin login, unasked.** One less tick, but a persistent cookie the admin did not ask
  for, which the ICO's exemption covers only with prominent notice.
- **Bookmarking `/admin`.** Works today and needs nothing built, but the maintainer asked for the button.

## Consequences

- #111 builds `/login`, the header link, and the hint along with staff login, so `/login` never points at a page
  that does not exist.
- The admin login page gains one checkbox, and the Security page one control to forget the browser.
- When realms move to their own hosts, the hint belongs on the domain they share. If admins move to a domain of their
  own, setting it needs a redirect through the main site, decided then.

## Sources

- [Where each realm's login starts](../research/identity/login-entry-points.md), with its sources: the sites' headers
  read on 2026-11-24, Microsoft's home realm discovery and Azure AD B2C session pages, and the ICO's exceptions to
  consent.
