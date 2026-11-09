# Front-end model research

Research for [#14](https://github.com/heavymedl-labs/tetsu/issues/14) and
[ADR-0030](../adr/0030-front-end-model.md), read 2026-11-09. This is research, not a decision. Where it disagrees
with an ADR, the ADR wins. The [customer channels research](market-scan/customer-channels.md) covers what
competitors offer; this page covers how such a system is built.

## Criteria

Written before looking at sources.

| # | Criterion | Why |
| --- | --- | --- |
| M1 | A business can use Tetsu's site, put Tetsu inside its own, or build its own on Tetsu's data | Competitors offer all three ([customer channels](market-scan/customer-channels.md)) |
| M2 | Components work in any framework, or none | The maintainer's goal: not tied to React, Astro, or Svelte |
| M3 | Public pages work before JavaScript loads, and load fast on a phone | Anvil Row's customers book from a phone |
| M4 | Card data never reaches Tetsu's pages or components | [ADR-0004](../adr/0004-security.md) |
| W1 | One maintainer can keep it consistent | The project's size |

## Findings

**A server-rendered default, with Web Components for interactive parts and an API for the rest, is an
established model.** The main risks are sign-in inside an embed and keeping two kinds of front end alike.

### Prior art

- **Shopify** serves storefronts as server-rendered Liquid themes. Dawn, its reference theme, builds its
  interactive parts as custom elements, such as the cart drawer, variant pickers, and predictive search
  ([Maestrooo: theme architecture](https://support.maestrooo.com/article/762-theme-architecture),
  [Shopify front-end architecture](https://jatinthummar.github.io/blog/shopify-frontend-architecture)). For full
  control, Shopify offers the Storefront API and Hydrogen, a headless React framework
  ([Shopify: Hydrogen](https://shopify.dev/docs/storefronts/headless/hydrogen)). Strength: the Dawn details come
  from third-party write-ups, not Shopify's documentation.
- **Design systems built on Web Components:** Adobe's Spectrum Web Components, on Lit, "designed to work with any
  web framework — or even without one" ([Spectrum Web Components](https://opensource.adobe.com/spectrum-web-components/));
  Shoelace, on Lit ([shoelace.style](https://shoelace.style/)); and Salesforce's Lightning Web Components and IBM
  Carbon in community lists ([web component libraries](https://dev.to/43081j/list-of-web-component-libraries-and-systems-33bl)).

### Web Components across frameworks

[Custom Elements Everywhere](https://custom-elements-everywhere.com/) tests how frameworks use custom elements. It
lists Vue and Svelte at 100%. React scored 71% in talks from 2017 to 2019; a 2026 article reports React 19 at
100% ([buildmvpfast](https://www.buildmvpfast.com/blog/web-components-declarative-shadow-dom-react-19-2026)), not
confirmed on the site itself.

### Design tokens

The W3C Design Tokens Community Group published its first stable format, 2025.10, on 2025-10-28, with a resolver
module for themes such as light and dark
([announcement](https://w3.org/community/design-tokens/2025/10/28/design-tokens-specification-reaches-first-stable-version/),
[format](https://www.w3.org/community/reports/design-tokens/CG-FINAL-format-20251028/)). It is a Community Group
report, not a W3C Standard, and its FAQ calls the core format stable for production.

### Server HTML with interactive islands

Islands architecture renders the page on the server and upgrades small regions into self-contained widgets
([Jason Miller](https://jasonformat.com/islands-architecture), [patterns.dev](https://www.patterns.dev/vanilla/islands-architecture)).
Web Components suit it, since each element upgrades on its own
([Begin](https://dev.to/begin/island-architecture-with-web-components-3hnp)).

### Against

Rich Harris argues websites should work without JavaScript, and Web Components alone do not, including on the
server ([Why I don't use web components](https://dev.to/richharris/why-i-don-t-use-web-components-2cia)). Ionic's
reply says libraries such as Lit and Stencil manage most of the friction
([Why we use web components](https://dev.to/ionic/why-we-use-web-components-2c1i)). Server-rendered pages that
use components only as islands meet M3 either way.

### Payments in embedded components

Stripe Elements serves card fields from an iframe on Stripe's domain that the host page cannot read, which keeps
card data off the merchant's page ([Stripe Web Elements](https://docs.stripe.com/stripe.js/switching)). A
component can hold such a frame, which meets M4.

### Sign-in inside an embed

When third-party cookies are blocked, Google lists partitioned cookies (CHIPS), the Storage Access API, and Related
Website Sets as the only ways for a cross-site frame to keep cookies. It lists CHIPS in Chrome 114, Edge 114,
Firefox 141, and Safari 26.2 ([Google: CHIPS](https://privacysandbox.google.com/cookies/chips)). A trade site
claims Chrome retired it in 2025 ([ppc.land](https://ppc.land/ships/)), and a WebKit bug filed 2026-01-24 reports
Safari dropping partitioned cookies after cross-site redirects. Strength: contested. Signing in on a Tetsu page at
the top level, or in a popup, and passing a token back avoids depending on any of them.

### One API or several

Sam Newman's Backends for Frontends pattern gives each experience its own thin API over shared domain services,
so one team does not gatekeep every front end ([Sam Newman](https://samnewman.io/patterns/architectural/bff/),
[Microsoft](https://learn.microsoft.com/en-us/azure/architecture/patterns/backends-for-frontends)). The cost is
duplicated code across them. With one maintainer and one API consumer per kind of client, a single API fits now.

## Review on 2026-11-09

A fresh-context adversarial review of ADR-0030 the day it was accepted. Each point was checked against its
source.

- **A token on the business's page fails ASVS Level 2.** `v5.0.0-10.1.1` (L2): "tokens are only sent to
  components that strictly need them", with the backend-for-frontend pattern as its example, where tokens stay
  with the backend ([V10](https://github.com/OWASP/ASVS/blob/master/5.0/en/0x19-V10-OAuth-and-OIDC.md)). A
  business's page runs scripts Tetsu does not control.
- **postMessage must check its sender.** `v5.0.0-3.5.5` (L2) discards messages from untrusted origins or in an
  invalid shape ([V3](https://github.com/OWASP/ASVS/blob/master/5.0/en/0x12-V3-Web-Frontend-Security.md)).
- **`frame-ancestors` controls frames, not scripts.** A component loaded by `<script>` on any site is untouched by
  it, so it cannot limit which sites embed a component
  ([MDN](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/frame-ancestors)).
- **Islands that render their own markup duplicate the server's.** Upgrading server HTML without JavaScript means
  either the component adds only behavior, or its markup exists twice.
- **Custom properties inherit into shadow roots,** so generic names such as `--color-primary` collide with a host
  page's, for example Tailwind's.
- **#14 had already set the rule** that an embed is the same content without the site's outer layout, which a
  frame of Tetsu's own page meets.

A review of the first ADR-0031 draft added that sign-in or payment inside a frame is still exposed to the host
page: it can overlay a signed-in frame (UI redress) or swap it for a look-alike, and `v5.0.0-3.4.6` (L2) allows
framing only where necessary. It also found that one host for every tenant rules out per-tenant session cookies
under `v5.0.0-3.3.3` (L2, `__Host-` cookies) and `v5.0.0-3.5.4` (L2, separate hostnames).

### Industry check

Read 2026-11-09. Mostly vendor documentation and support pages; strength noted per point.

- **Checkout runs at the top level.** Stripe's hosted Checkout refuses to run in a frame and asks for a top-level
  redirect ([OPSWAT](https://www.opswat.com/docs/mdcloud/knowledge-base/why-am-i-seeing-the-error----stripe-checkout-is-not-able-to-run-),
  secondhand). Visa's Secure Acceptance requires blocking third-party framing to prevent clickjacking, citing PCI
  DSS v4 ([Visa](https://docs.cas.visaacceptance.com/docs/cybs/en-us/sa/developer/all/sa-hosted/secure-acceptance/sa-wm-iframe-implementation.md)).
  Shopify's Buy Button opened checkout in a new window, and now offers the same tab
  ([Shopify changelog](https://changelog.shopify.com/posts/buy-button-now-supports-checkout-in-same-tab)); its
  Checkout Kit opens a new tab or popup ([Checkout Kit](https://shopify-dev.shopifycloud.com/docs/storefronts/mobile/checkout-kit/web)).
- **Sign-in in a frame fails where third-party cookies are blocked.** Auth0 documents Safari blocking its cookies
  in frames ([Auth0](https://support.auth0.com/center/s/article/Safari-blocks-Auth0-auth-cookies-in-iframe)).
  Booking vendors advise linking rather than embedding ([WriteUpp](https://help.writeupp.com/en/article/can-i-embed-online-booking-in-my-website-using-an-iframe-tsm32j/),
  [Reservio](https://help.reservio.com/en/articles/10656312-how-to-fix-reservation-button-issues-in-iframe-on-safari-iphone-ipad)).
  A top-level tab cannot read a session cookie set in a frame
  ([Google](https://developers.google.com/classroom/add-ons/developer-guides/third-party-cookies)). Mindbody's
  widgets let customers sign in inside the embed ([Mindbody](https://www.mindbodyonline.com/business/education/blog/how-to-turn-website-visits-into-bookings)),
  the counterexample.
- **A subdomain of the business's own domain is first-party.** Serving the app from a subdomain the business
  points at it lets Safari treat it as first-party
  ([write-up](https://medium.com/@pulamigaurav8/how-we-solved-authentication-issues-caused-by-intelligent-tracking-prevention-itp-in-04988d05cdc6),
  one team's report).
- **Per-tenant subdomains isolate sessions.** Path-based tenancy complicates cookie isolation
  ([pattern note](https://commons-os.github.io/patterns/subdomain-based-tenant-routing/)); scoping each session
  cookie to its tenant's subdomain fixed one framework's cross-tenant sign-in
  ([ABP](https://abp.io/support/questions/9488/Blazor-WebApp-login-issues-authenticate-in-one-tenant-is-logged-in-all)).
  A shared parent domain belongs on the Public Suffix List, as `myshopify.com` and `github.io` are, so no cookie
  can span tenants ([PSL note](https://kestra.io/blueprints/mozilla-public-suffix-cookie-isolation)).
- **Server-rendered HTML fragments are a mainstream pattern** ("HTML over the wire"): Hotwire and htmx
  ([Thoughtworks Radar](https://thoughtworks.com/radar/techniques/hotwire),
  [Turbo](https://turbo.hotwired.dev/handbook/introduction), [Hypermedia Systems](https://hypermedia.systems/extending-html-as-hypermedia)).

A second review of the draft, the same day, found that booking on a subdomain of the business's own domain, added
after the industry check, reopens the main risk: that host is same-site with the business's page, so its scripts
can send forms with the customer's cookies, which only anti-forgery checks stop (`v5.0.0-3.5.1`), and the
business controls its DNS. Tenancy's router also resolves the tenant from the path alone, so any host pointed at
Tetsu would serve every tenant. It also corrected the industry check: Stripe's card fields and embedded Checkout
do run framed on merchants' pages, and Stripe requires registering every domain that shows a wallet when using
them, framed or not ([Stripe](https://docs.stripe.com/payments/payment-methods/pmd-registration)). The booking
vendors advise linking for Safari's cookie rules, not specifically for sign-in.

A third review found the Mindbody counterexample unsupported: its blog does not say sign-in happens inside the
embed. It could not load the Visa page, whose address suggests it documents running Secure Acceptance in a frame,
so the claim that Visa's hosted page refuses framing is unconfirmed. It also found that #8, not #9, already owns
tenant addresses and how a business proves it owns a domain.

These led to [ADR-0031](../adr/0031-front-end-model-revised.md).

## What this cannot show

No data on how often small businesses embed rather than link to a booking page, or on how much keeping Web
Components and server-rendered pages consistent costs one maintainer. No sources were tried in code.
