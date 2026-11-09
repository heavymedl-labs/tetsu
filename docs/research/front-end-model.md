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

## What this cannot show

No data on how often small businesses embed rather than link to a booking page, or on how much keeping Web
Components and server-rendered pages consistent costs one maintainer. No sources were tried in code.
