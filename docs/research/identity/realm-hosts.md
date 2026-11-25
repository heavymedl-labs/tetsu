# A host for each realm

Researched 2026-11-25 for [#169](https://github.com/heavymedl-labs/tetsu/issues/169), after the maintainer chose to give
each realm a host of its own now rather than share one until a domain
([#102](https://github.com/heavymedl-labs/tetsu/issues/102)). What is settled is not repeated here: each realm's
passkeys belong to its own host ([ADR-0040](../../adr/0040-identity-and-sign-in.md)), realms' paths
([ADR-0084](../../adr/0084-realm-routes.md)), why a shared host costs passkeys and signals, and why free hours allow a
service per realm ([staff pages and their host](staff-pages.md)), and that a passkey made on an `onrender.com` host
can use only that host ([passkey domains](../passkey-domains.md)). This page covers how leaders split hosts by who
uses them, and how one image serves three hosts on the demo. Sources were read through search summaries on
2026-11-25, and say so where they are a vendor's community rather than its documentation.

## Summary

Leaders serve the back office on a host apart from the pages customers book or shop on: Shopify's admin at
`admin.shopify.com`, its storefronts and customer accounts on each store's domain, and Square's Dashboard on
`squareup.com`, with each business's booking site at an address of its own. Mindbody keeps staff and customer sign-in
apart and gives staff an app of their own. On Render, one image deploys to several services, each by the same digest,
and on `onrender.com`, a public suffix, no service's cookies or passkeys reach another's. Tetsu's server does not
check the `Host` header; Render routes each host to its own service, and a check would need to learn the host Render's
health checks send.

## Back offices live on a host apart from customers

**Takeaway:**

A platform's staff work at one address and its customers at another, and a business's own staff and customers never
share a login page.

**Cited Findings:**

- Shopify moved every store's admin to `admin.shopify.com` in January 2023
  ([Shopify changelog](https://changelog.shopify.com/posts/shopify-admin-now-lives-at-admin-shopify-com)), while
  customer account pages live on the store's own domain, such as `account.<store domain>`, with login pages there too
  since May 2025 ([Shopify Help](https://help.shopify.com/en/manual/domains/add-a-domain/connecting-domains/connect-domain-customer-account);
  [Shopify changelog](https://changelog.shopify.com/posts/branded-urls-on-customer-login-pages)).
- Square's Dashboard is its sellers' console, reached at `squareup.com`
  ([Square: Dashboard overview](https://squareup.com/help/ca/en/article/5183-square-dashboard-overview)), and a
  business's online booking site is a page with an address of its own, created from Appointments
  ([Square: booking site](https://squareup.com/help/us/en/article/7245-create-a-square-online-booking-site-from-appointments)).
- Mindbody describes its consumer app as for finding and booking, and the Business app as for owners and staff
  ([Mindbody: business app](https://www.mindbodyonline.com/en-gb/business/business-app)); a third-party guide reports a
  staff sign-in link on its consumer sign-in page, and different lockouts for each
  ([Gymdesk](https://gymdesk.com/blog/faqs-about-mindbody-accounts)).

## One image deploys to several Render services

**Takeaway:**

Render deploys a prebuilt image to a service by digest, and each service is deployed on its own, so three services
take three deploy calls with one digest. Migrations run once, before any of them.

**Cited Findings:**

- A deploy may name the image by digest, and every part of the image's address but the tag or digest must match the
  service's own ([Render: deploy hooks](https://render.com/docs/deploy-hooks);
  [Render: deploy a prebuilt image](https://render.com/docs/deploy-an-image)). Render's blueprints list deploying the
  same image to several places as a use of image services
  ([Render tutorial](https://render.com/tutorials/advanced-blueprint-patterns/docker-and-image-runtimes)).
- Rolling an image service back needs that image and digest still in the registry (same sources), which Tetsu's deploy
  already keeps for the commits CI passed.

## Hosts on a public suffix share nothing

**Takeaway:**

`onrender.com` is on the Public Suffix List, so each service's host is a site of its own: a cookie set by one is never
sent to another, and a passkey made on one answers only there. Realm separation on the demo comes from the hosts, not
only from Tetsu's code.

**Cited Findings:**

- Render's `onrender.com` is in the list's private section, so `tetsu-demo.onrender.com` is a registrable domain in its
  own right ([passkey domains](../passkey-domains.md#the-demo-host-can-never-be-tetsus-passkey-home)).
- Tetsu's session cookies are `__Host-` cookies, which a browser binds to the exact host that set them
  ([ADR-0048](../../adr/0048-sign-in-design.md)).

## A Host header check is defense in depth, not the boundary

**Takeaway:**

Frameworks offer a list of allowed hosts so a forged `Host` header cannot steer links or redirects. Tetsu builds every
link from its configured origins, never from `Host`, and Render routes each host to one service, so a check adds
little now. It would need to admit the host Render's health checks send, which was not found.

**Cited Findings:**

- Django's `ALLOWED_HOSTS` and Drupal's trusted host patterns refuse requests for hosts a site does not serve
  ([PortSwigger: Host header attacks](https://portswigger.net/web-security/host-header)), and OWASP's testing guide
  shows how an unchecked `Host` header leads to redirects to an attacker's domain
  ([OWASP WSTG](https://owasp.org/www-project-web-security-testing-guide/latest/4-Web_Application_Security_Testing/07-Input_Validation_Testing/17-Testing_for_Host_Header_Injection)).
- No ASVS 5.0 requirement naming the `Host` header was found.

## Recommendations for #169

- **One image, one setting per realm's origin, and one naming what a service serves.** Every service knows every
  realm's origin, so it can link across hosts, such as an invite link to the staff host; each serves only its own
  realm's routes, and every other path is the site's not-found page. The site keeps `TETSU_ORIGIN`.
- **Realms keep their paths on their own hosts:** admins at `/admin` on the admin host and staff at `/app` on the app
  host, as ADR-0084 has them, so no route, test, or reserved name changes. `/` on the admin and app hosts sends a
  visitor to the realm's home, as Shopify's admin host opens on the admin.
- **Each realm's relying party is its own origin,** so its passkeys answer only its host.
- **The deploy migrates once, then deploys the same digest to each service and smoke checks each host,** including
  that a realm's pages are not found on another's host. One service ID per realm is a repository variable.
- **No `Host` check now,** since links come from settings and Render routes by host. It is named on the issue as a
  hardening step once Render's health check host is known.
- **On the demo:** `tetsu-demo` stays the site, and `tetsu-demo-admin` and `tetsu-demo-app` are new free services, set
  to sleep when idle like the first. The admin's demo passkey is enrolled again on the admin host in the reset #111's
  sub-slice 2e plans.

## Gaps

- Pages were read through search summaries, not opened. Vagaro's staff and customer hosts were not found.
- The host Render's health checks send, and whether three services waking at once change Neon's free connection use,
  were not checked.
