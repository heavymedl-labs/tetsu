# 0094. Realm paths at the root of their hosts

- Status: Accepted
- Date: 2026-11-26
- Supersedes [ADR-0084](0084-realm-routes.md)'s `/admin` and `/app` prefixes, [ADR-0092](0092-a-service-per-realm-before-domains.md)'s
  "realms keep their paths on their own hosts", and [ADR-0093](0093-log-in-leads-to-staff.md)'s header link to `/login`

## Context

ADR-0084 put each realm under a prefix, `/admin` or `/app`, because every realm shared one host. ADR-0092 gave each
realm a host of its own and kept the prefixes, so every admin and staff address now repeats what its host already
says: `tetsu-admin.onrender.com/admin/settings/profile`.

Products whose back office has a host of its own do not name the realm in the path. Shopify moved
`<store>/admin/settings/...` to `admin.shopify.com/store/<store>/settings/...`, and Stripe's personal page is
`dashboard.stripe.com/settings/user`. A first segment that remains names a scope inside the realm, such as a store
([realm paths on hosts](../research/identity/realm-paths-on-hosts.md)).

The prefixes also cost redirects. Opening the admin host, or its settings shell's brand link to `/`, takes two
redirects before a page: `/` to `/admin`, then `/admin` to Profile. The site header's "Log in" adds a third host hop
through `/login`.

Locally, the prefix is what still separates realms: `.env.example` serves every realm on `http://localhost:8080`,
where they also share one passkey host. Chromium treats `*.localhost` as a secure context and as a top level for
passkeys, so each realm can have its own local origin.

## Decision

- **Each realm's pages sit at the root of its host.** No path names the realm:

  | Host | Pages |
  | --- | --- |
  | Admins | `/`, `/login`, `/join`, `/settings/profile`, `/settings/security`, `/mailbox` |
  | Staff | `/`, `/login`, `/join`, `/join/invitation`, `/settings/profile`, `/settings/security`, and later `/<business>/...` |
  | Site | `/`, `/login`, `/<business>`, and later `/<business>/account/...` |

  A business's staff pages begin with its name and no segment before it, as Linear's begin with the workspace's.
  Requests keep their names under the page they serve, such as `/login/begin`, `/passkeys/finish`, `/session`,
  `/sessions/end`, and `/logout`. The site's reserved names ([#90](https://github.com/heavymedl-labs/tetsu/issues/90))
  keep a business from taking any of them on the staff host.
- **Each page has one address, without a trailing slash.** A `GET` or `HEAD` with one gets a 308 to the address
  without it, as Vercel does, so a mistyped address still lands. A path in other case, or with an empty segment, gets
  the realm's not-found answer, as GitHub's does.
- **A redirect only where the request needs one:** a logged-out visit to a page goes to `/login?next=<page>`, a
  logged-in visit to `/login` goes on to that page, a form post ends with a 303 to the page that follows (logout,
  ending sessions), and a realm host's `/` goes to its home. Each takes one hop.
- **Links point at the canonical address, never at a redirect.** The settings shell's brand links to Profile, the site
  header's "Log in" links straight to staff login at `TETSU_STAFF_ORIGIN`, and invite links name `/join` on their
  realm's host. The site's `/login` stays, as a 303 for anyone who types it.
- **A realm host's `/` is its home,** from the realm's own router rather than the site's: Profile for admins
  ([ADR-0086](0086-the-account-area-is-the-admins-home.md)), and Profile for staff until #158 gives them a home page of
  their own, which `/` then shows without a redirect. Logged out, `/` goes to `/login` in one hop, as
  `dashboard.stripe.com/` does.
- **Locally, each realm has its own origin,** `http://localhost:8080` for the site, `http://admin.localhost:8081`, and
  `http://app.localhost:8082`, from one process. A service serving several realms listens for each on its origin's
  port, so no router reads the `Host` header, as on the demo.
- **Old prefixed paths are not redirected.** Only the maintainer holds links to them, as when ADR-0084 moved `/ops`.

## Alternatives considered

- **Keep the prefixes until domains ([#102](https://github.com/heavymedl-labs/tetsu/issues/102)).** No work now, but
  every page built meanwhile, such as staff's business pages, adds an address to move later, and the redirects stay.
- **Strip the prefix on realm hosts only, keep it locally.** One router per environment shape, so tests pass locally
  on addresses the demo never serves, and local realms keep sharing a passkey host.
- **Route by `Host` in one local listener.** One port, but a `Host` check must then learn what Render's health checks
  send ([a host for each realm](../research/identity/realm-hosts.md#a-host-header-check-is-defense-in-depth-not-the-boundary)),
  and the demo would not run it.
- **A 404 for a trailing slash.** No redirect rule at all, but no leader read does it: Vercel redirects, and Stripe,
  GitHub, and Square serve the page.
- **Serve the page at a trailing slash too,** as Linear and Stripe do. No hop, but each page then has two addresses.
- **Name the business in every staff address, personal pages too,** as Linear names the workspace, with
  `/settings/...` as a shortcut to the last business used. It loses because a staff account spans businesses, and the
  shortcut is one more redirect.
- **Staff's business pages under a segment, such as `/b/<business>`,** as Shopify's `/store/<store>`. No business
  name could ever collide with a staff page, but every staff address grows, and the reserved names already prevent
  the collision, as Linear's do.
- **Redirect the old prefixed paths, as Shopify did when its admin moved.** Right for a product with users, but only
  the maintainer holds such links.
- **Serve Profile at `/` for admins.** No hop at all, but Profile would have two addresses.

## Consequences

- The return-to-page rule accepts any plain path on the realm's own host, since every path there is the realm's. It
  stays Tier 1, with its property tests rewritten for a root base.
- The realm's unknown-path answer becomes its host's fallback, in place of the site's not-found page.
- Invite links, the deploy's smoke check, `.env.example`, and the docs that name realm paths change with the code.
- Admin and staff passkeys enrolled locally are made again on the new local hosts. The demo keeps its passkeys, since
  its hosts do not change.
- #111's sub-slice 3 builds the header link and `/login` as this ADR has them, whichever lands first.

## Sources

- [A realm's paths on a host of its own](../research/identity/realm-paths-on-hosts.md), 2026-11-26.
- [Shopify changelog: Shopify admin now lives at admin.shopify.com](https://changelog.shopify.com/posts/shopify-admin-now-lives-at-admin-shopify-com).
- [Stripe docs: Web Dashboard](https://docs.stripe.com/dashboard/basics), linking `/settings/user`.
- Stripe, GitHub, Square, and Vercel's answers to logged-out requests, and Linear's routes logged in, 2026-11-26,
  recorded in the research.
- [Chromium: origins that support WebAuthn](https://chromium.googlesource.com/chromium/src/+/main/content/browser/webauth/origins.md),
  on `.localhost` hosts.
