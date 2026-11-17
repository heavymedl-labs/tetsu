# 0030. Front-end model

- Status: Superseded by 0031
- Date: 2026-11-09

## Context

[#14](https://github.com/heavymedl-labs/tetsu/issues/14) asks how businesses and their customers reach Tetsu.
The [customer channels research](../research/market-scan/customer-channels.md) found that all 14 competitors
offer a hosted booking page, a widget for the business's own site, or both, and that nobody offers just one way
in. Both real studios it looked at keep booking on their own websites, with booking embedded.

Businesses want different amounts of control: a Tetsu page and nothing else, Tetsu's booking inside a site they
already have, or a site or app of their own built on Tetsu's data. The maintainer wants the same freedom for
Tetsu's own sites, and front-end code that is not tied to one framework.

[ADR-0007](0007-server-stack.md) already chose Askama for server-rendered HTML, and
[ARCHITECTURE.md](../../ARCHITECTURE.md) has the server render each business's booking pages. Modules reach
the outside only through adapters ([ADR-0003](0003-architecture.md)), so every channel is one more adapter over
the same use cases.

## Decision

Tetsu uses a hybrid model: a server-rendered default site, with a public API and framework-neutral components
for everything else.

- **The use cases are the product.** Every channel calls the same use cases, and none holds business rules:
  the default site, the API, the components, and any site built on them.
- **The default site is server-rendered by Tetsu** in Askama, as ADR-0007 decided. A business with no website
  of its own gets one, fast, working without JavaScript, and readable by search engines. Its templates use the
  shared styles below, not styles of their own. Its interactive parts are the same Web Components a business
  embeds, used as islands that upgrade server-rendered HTML, so each widget has one implementation.
- **The public API is the contract** for everything Tetsu does not render: the staff app, native apps,
  partners, components, and sites businesses build. [#12](https://github.com/heavymedl-labs/tetsu/issues/12)
  decides its design.
- **A design system has four layers,** each built on the one before:
  1. **Design tokens:** colors, type, spacing, and the like, kept in one file in the Design Tokens Community
     Group's format (2025.10), and generated into CSS custom properties and the other forms each consumer needs.
  2. **Base CSS** on those tokens, shared by the default site and every component, so they look the same.
  3. **Web Components,** such as a schedule or a booking flow, built on a client generated from the API's spec.
     They work in any framework, or none.
  4. **Framework wrappers,** such as for React, Svelte, or Astro: thin, generated where tooling allows, and
     added only when someone needs one.
- **A business's theme is data:** token values stored with the tenant, applied by the default site and by
  components alike.
- **Embedding is opt-in per business.** Pages refuse framing by default (`frame-ancestors 'none'`). A business
  names the sites that may embed its pages.
- **Login never relies on cookies across sites.** A component on a business's own site signs a customer in on a
  Tetsu page opened at the top level or in a popup, which passes a token back.
  [#9](https://github.com/heavymedl-labs/tetsu/issues/9) decides the details.
- **Card fields stay in the payment processor's own frame,** inside a component as on Tetsu's pages
  ([ADR-0004](0004-security.md)).
- **Each layer's tools are chosen by an ADR when that layer is due,** such as the token format, the component
  library, and how the design system is packaged. This ADR chooses none of them. #14 still decides the staff
  app's framework.

## Alternatives considered

- **Server-rendered pages only.** Simplest, one binary and one language. But a business could change only colors,
  never structure, and nothing could live inside its own site until an API existed.
- **Headless only, where every page, Tetsu's included, is built on the API.** The most flexible, but the API must
  exist before the first page, and live pages would render in a second runtime, such as Node, beside the Rust
  server.
- **Tetsu's own site in a JavaScript framework such as Astro.** Astro's static output and islands are fast, but
  booking pages hold live data, so it would still need server rendering in Node. Businesses would gain no more
  freedom than the hybrid gives them through the API.
- **Components written separately for each framework.** Native in each, but every component kept twice or more
  by one maintainer drifts apart.

## Consequences

- Nothing already built changes. `tetsu serve`, `resolve`, and the security headers serve the default site
  as they stand.
- The design system brings a second language, TypeScript, which needs its own row in
  [CONSTRAINTS.md](../../CONSTRAINTS.md) and its own checks before any code
  ([ADR-0014](0014-checks-and-duties.md)), and likely a repository of its own.
- Components that book or show live data wait on the API (#12), and login in an embed waits on #9.
- The default site and embeds share one implementation of each interactive widget, and shared tokens and base
  CSS carry the look. What must stay alike is then only the static HTML around them.
- The order keeps the milestone first: shared styles and the default site come with the walking skeleton, and
  the API, components, and wrappers follow as booking arrives.

**Weakest evidence.** No measurement yet of how often businesses embed rather than link, or of how much work
keeping Web Components and server-rendered pages consistent takes for one maintainer. Support for partitioned
cookies across browsers is contested, which is why login avoids them.

**Revisit** if most businesses embed rather than use the default site, if Web Components prove unworkable
inside a framework a business relies on, if keeping the default site and components consistent costs more than
one renderer would, or if the staff app's and the public components' needs from the API pull apart, which is when
a backend per front end ([Sam Newman: BFF](https://samnewman.io/patterns/architectural/bff/)) earns its cost.

## Sources

- [Front-end model research](../research/front-end-model.md), 2026-11-09: prior art such as Shopify's themes and
  Web Component design systems, framework support, design tokens, islands, payments, and login in embeds.
- [Customer channels research](../research/market-scan/customer-channels.md), 2026-11-04: how 14 competitors let
  customers book, and the two studios' embedded booking.
- [MDN: Web Components](https://developer.mozilla.org/en-US/docs/Web/API/Web_components): custom elements and
  shadow DOM, standard in every current browser.
- [WebKit: Storage Access API](https://webkit.org/blog/8124/introducing-storage-access-api/): why login in an
  embed cannot rely on third-party cookies.
