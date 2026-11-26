# A realm's paths on a host of its own

Researched 2026-11-26 for [#111](https://github.com/heavymedl-labs/tetsu/issues/111), after the maintainer noticed that
logging in on the admin host lands on `/admin/settings/profile`, and asked whether `/admin` is redundant now that each
realm has a host of its own ([ADR-0092](../../adr/0092-a-service-per-realm-before-domains.md)). What is settled is not
repeated here: why each realm has its own host ([a host for each realm](realm-hosts.md)), the realms' paths while they
shared one ([ADR-0084](../../adr/0084-realm-routes.md)), and where "Log in" leads
([ADR-0093](../../adr/0093-log-in-leads-to-staff.md)). Sources were read through search summaries on 2026-11-26, and
say so where they are a vendor's community or a third party rather than its documentation.

## Summary

Products whose back office has a host of its own do not repeat the host's name in the path. Shopify's move shows it
plainly: `<store>/admin/settings/...` became `admin.shopify.com/store/<store>/settings/...`, so the realm moved into
the host and the path kept only the store and the page. Stripe's personal page is `dashboard.stripe.com/settings/user`
and Cloudflare's is under `dash.cloudflare.com/profile`. A first segment that remains names a scope inside the realm,
such as Shopify's store or Slack's workspace, never the realm itself. Tetsu's `/admin` and `/app` prefixes earned their
place while every realm shared one host, and now repeat the host. Today they still do one job: this machine serves
every realm on one origin, `localhost:8080`, where the prefix is what tells the realms apart. Dropping them needs each
realm on its own origin locally too, which `*.localhost` hosts can give, since browsers treat `localhost` as a top
level for passkeys.

## Leaders' back-office paths do not name the realm

**Takeaway:**

On a dedicated host, a path starts with a scope or a page. `settings` and `profile` are the usual words for a
person's own pages.

**Cited Findings:**

| Product | Back-office host | A person's own page | Scope segment |
| --- | --- | --- | --- |
| Shopify | `admin.shopify.com` | `/store/<store>/settings/...` | `/store/<store>` |
| Stripe | `dashboard.stripe.com` | `/settings/user` | none in the path |
| Cloudflare | `dash.cloudflare.com` | `/profile/api-tokens` | account id |
| Slack | `app.slack.com` | (in the client) | `/client/<workspace id>` |
| Google Workspace | `admin.google.com` | (in the console) | none in the path |

- Shopify moved every store's admin from the store's own address to `admin.shopify.com`, replacing the
  `myshopify.com` part of each store's URL ([Shopify changelog](https://changelog.shopify.com/posts/shopify-admin-now-lives-at-admin-shopify-com)).
  A community thread shows the old address `example-store.com/admin/settings/checkout/editor` redirecting to
  `admin.shopify.com/store(s)/example-store/settings/checkout/editor`
  ([Shopify community](https://community.shopify.dev/t/will-deep-links-into-sections-of-the-admin-stay-supported/28120)).
  The thread spells it `stores`, a guide `store`; either way `admin` leaves the path.
- Stripe's docs link `dashboard.stripe.com/settings/user` as a person's details, password, and sessions
  ([Stripe docs](https://docs.stripe.com/dashboard/basics)).
- Cloudflare's user API token links open at the profile level, `/profile/api-tokens`
  ([Cloudflare docs](https://developers.cloudflare.com/fundamentals/user-profiles)).
- Slack's web client lives at `app.slack.com/client/<workspace id>/<channel id>`
  ([Slack help](https://slack.com/help/articles/221769328-Locate-your-Slack-URL-or-ID)).
- The Google Admin console is at `admin.google.com`, reached by menu rather than a documented path
  ([Google support](https://support.google.com/a/answer/33324)).

## Leaders redirect once, and forgive a trailing slash

**Takeaway:**

A logged-out visit to any page takes one redirect to login, carrying the page to return to. A dashboard's root is
that same one hop. No leader read answers a trailing slash with a 404: Vercel sends it to the address without one,
and the rest serve the same page. GitHub answers other case with a 404.

**Cited Findings:**

Each address was requested logged out, with a browser's user agent, on 2026-11-26. Shopify, Cloudflare, and Linear
answer every address with the same 200 page and route in the browser, so they show nothing here.

| Address | Answer |
| --- | --- |
| `dashboard.stripe.com/` | 303 to `/login` |
| `dashboard.stripe.com/settings/user` | 303 to `/login?redirect=%2Fsettings%2Fuser` |
| `dashboard.stripe.com/settings/user/` | 303 to `/login?redirect=%2Fsettings%2Fuser%2F`, keeping the slash |
| `github.com/settings/profile` and `.../profile/` | 302 to `/login?return_to=...%2Fsettings%2Fprofile`, both without the slash |
| `github.com/Settings/profile` | 404 |
| `github.com/login/`, `app.squareup.com/login/` | 200, the login page |
| `app.squareup.com/` | 301 to the marketing site, `squareup.com/us/en` |
| `app.squareup.com/dashboard` | 302 to `/login?return_to=%2Fdashboard` |
| `vercel.com/account/`, `vercel.com/dashboard/` | 308 to the address without the slash |

Where a store or business sits in the path, Shopify puts a segment before it, `/store/<store>`, so a store's name can
never collide with the admin's own pages. When Shopify moved its admin, the old addresses kept working as redirects
([above](#leaders-back-office-paths-do-not-name-the-realm)).

## Linear puts the workspace bare at the root

**Takeaway:**

Linear, whose settings shell Tetsu follows ([ADR-0086](../../adr/0086-the-account-area-is-the-admins-home.md)), puts
the workspace's name as the first segment with no segment before it, and keeps its own words, such as `settings` and
`login`, out of workspace names. Every page's address names the workspace, even a person's own pages. An address
without it is a shortcut that Linear rewrites to the workspace the person last used. Linear forgives a trailing slash
and answers other case with "Not found".

**Cited Findings:**

Each address was opened in Chrome, logged in to a throwaway workspace, `yeetcode`, on 2026-11-26. Linear routes in the
browser, so the address shown is where its client router settled, not an HTTP redirect.

| Opened | Settled at |
| --- | --- |
| `/` | `/yeetcode/agent`, the workspace's default view |
| `/yeetcode` | `/yeetcode/agent` |
| `/login`, logged in | `/yeetcode/agent` |
| `/settings` | `/yeetcode/settings/account/preferences` |
| `/settings/account/profile` | `/yeetcode/settings/account/profile` |
| `/yeetcode/settings/account/profile/` | the same page, the slash kept in the address |
| `/YeetCode/Settings/Account/Profile` | "Not found" |
| `/yeetcode/nonsense-page` | the workspace's shell, at the address as typed |

- The settings sidebar holds a person's pages under `/<workspace>/settings/account/...` (`preferences`, `profile`,
  `notifications`, `security`, `connections`) and the workspace's under `/<workspace>/settings/...`, such as
  `issue-labels` and `import-export`.
- Tetsu differs on purpose: a staff account spans businesses ([ADR-0040](../../adr/0040-identity-and-sign-in.md)), so
  its personal pages name no business, as GitHub's `/settings/profile` names no organization
  ([ADR-0084](../../adr/0084-realm-routes.md)).

## A framework keeps its prefix only when the host is shared

**Takeaway:**

Django mounts its admin at `/admin/` on a shared host, and moving it to a host of its own takes host routing or a second
server. Advice on the path itself is split, and a changed path is not a protection.

**Cited Findings:**

- Moving Django's admin to a subdomain means routing each host to its own URL configuration, as `django-hosts` does
  ([Crowdbotics, on Medium](https://medium.com/crowdbotics/how-to-use-dynamic-subdomains-in-django-dc1cb2cac00b)), or
  running a second server ([Open edX discussion](https://groups.google.com/g/edx-code/c/Zk7K8MVTtMs/m/9xEjRywQBwAJ)).
- A Django ticket asks for `www` `/admin` to redirect to an `admin.` host
  ([Django #23116](https://code.djangoproject.com/ticket/23116)).
- OWASP treats a hidden admin address as something to test for, not a protection, as
  [Where each realm's login starts](login-entry-points.md#a-back-office-is-reached-by-its-address) records.

## Tetsu's prefixes now repeat the host

**Takeaway:**

On the demo, every admin and staff address carries a segment its host already says. Locally, the same segment is the
only thing that separates realms.

**Cited Findings:**

Tetsu's routes on 2026-11-26, from `crates/identity/src/adapters/http.rs` and `crates/tetsu/src/serve.rs`:

| Host | Root | Home | Personal pages | Other pages |
| --- | --- | --- | --- | --- |
| `tetsu-site` | the site's home | | | `/<business>`, `/<business>/theme.css` |
| `tetsu-admin` | 303 to `/admin` | `/admin`, a 303 to Profile or login | `/admin/settings/profile`, `/admin/settings/security` | `/admin/login`, `/admin/join`, `/admin/mailbox`, `/admin/passkeys/...`, `/admin/session`, `/admin/sessions/end`, `/admin/logout` |
| `tetsu-staff` | 303 to `/app` | `/app`, a 303 to Profile or login | `/app/settings/profile`, `/app/settings/security` | `/app/login`, `/app/join`, `/app/join/invitation`, `/app/join/accept/...`, and the same session paths |

- Logging in on the admin host takes two redirects before a page: `/` to `/admin`, then `/admin` to Profile.
- The realm prefix is also load-bearing in code: the return-to-page rule accepts only paths under a realm's base
  (`return_path` in `crates/identity/src/browser.rs`), and invite links are built from `/admin/join` and `/app/join`.
- `.env.example` serves every realm on `http://localhost:8080` (`TETSU_SERVES=site,admin,staff`), with both realm
  origins the same. Locally, then, realms share one passkey RP ID and differ only by path and cookie name. That is the
  shared-host cost ADR-0092 removed from the demo.
- Staff's planned per-business pages, `/app/<business>` (ADR-0084), would be `/<business>` on a staff host. The site's
  reserved names ([#90](https://github.com/heavymedl-labs/tetsu/issues/90)) already keep a business from taking
  `settings`, `login`, `join`, and Tetsu's other words.

## Local realms can each have a host

**Takeaway:**

`admin.localhost` and `app.localhost` are secure contexts and their own passkey hosts in Chromium, so this machine can
give each realm its own origin without TLS or a hosts file. No source showed a test of `admin.localhost` as an RP ID,
so it needs a check in a browser before it is relied on.

**Cited Findings:**

- Chromium allows WebAuthn on plain HTTP for `localhost` and any host ending in `.localhost`, and treats `localhost`
  as a top-level domain for RP IDs ([Chromium: origins that support WebAuthn](https://chromium.googlesource.com/chromium/src/+/main/content/browser/webauth/origins.md)).
- A passkey answers only on its RP ID, so a passkey made on `admin.localhost` would not be offered on `app.localhost`
  ([web.dev: RP ID](https://web.dev/articles/webauthn-rp-id)).
- Serving several hosts from one local process needs the server to route by `Host`, which Tetsu does not do today
  ([a host for each realm](realm-hosts.md#a-host-header-check-is-defense-in-depth-not-the-boundary)). The other way
  is a process, or a port, per realm.
