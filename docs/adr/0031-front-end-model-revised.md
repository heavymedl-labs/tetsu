# 0031. Front-end model, revised

- Status: Accepted
- Date: 2026-11-09
- Supersedes [ADR-0030](0030-front-end-model.md)

## Context

[ADR-0030](0030-front-end-model.md) put Tetsu's Web Components on businesses' own pages and handed them a
sign-in token. Reviews the same day found that fails ASVS Level 2, which
[ADR-0029](0029-security-target-revised.md) requires with no accepted risks for identity and money
([research](../research/front-end-model.md#review-on-2026-11-09)). A business's page runs scripts Tetsu does not
control, which can read a token, overlay or imitate a signed-in frame, or send forms with a customer's cookies.
Browsers also block cookies in frames on other sites. Stripe's hosted Checkout and Shopify's Buy Button keep
checkout at the top level for these reasons, and [#14](https://github.com/heavymedl-labs/tetsu/issues/14) had
already set the rule that an embed is the same content without the site's outer layout.

## Decision

ADR-0030's aims stand: a business can use Tetsu's site, put Tetsu inside its own, or build on Tetsu's data, and
nothing ties Tetsu to one JavaScript framework. This is how.

- **The use cases are the product.** Every channel calls them, and none holds business rules.
- **Tetsu server-renders its pages** in Askama ([ADR-0007](0007-server-stack.md)), the one source of their
  markup. They work without JavaScript, except steps that cannot, such as a processor's card fields or a passkey.
- **Islands add behavior, not markup.** They are light-DOM Web Components on Tetsu's own pages. New content comes
  as HTML fragments Askama renders; a library for that, such as htmx, needs its own ADR.
- **An embed is a frame of Tetsu's page, for browsing only.** Embeddable pages, and every request they make,
  ignore the session and change nothing, so nothing signed in is ever framed. A small `<tetsu-embed>` element,
  usable in any framework or none, places the frame and checks every message it gets (`v5.0.0-3.5.5`).
- **Signing in and paying happen at the top level, never in a frame of a business's page.** Payment uses the
  processor's hosted page or card fields, as [ADR-0004](0004-security.md) allows.
- **Each business names the exact `https://` sites that may frame its pages.** Framing is decided in one place,
  the composition root's security-header layer, and fails closed: anything else gets `frame-ancestors 'none'`
  (`v5.0.0-3.4.6`). That layer and the list's validation are Tier 1.
- **Every form that changes state is protected against forgery** (`v5.0.0-3.5.1`), never by `SameSite` alone.
- **A business with developers builds on Tetsu's public data through the API** from its own backend. Acting for a
  signed-in customer that way waits on [#9](https://github.com/heavymedl-labs/tetsu/issues/9) and
  [#12](https://github.com/heavymedl-labs/tetsu/issues/12), so it cannot skip Tetsu's sign-in rules.
- **The design system has three layers:** design tokens in the Design Tokens Community Group's format (2025.10),
  generated into `--tetsu-*` CSS properties; base CSS on them; and Web Components, the islands and
  `<tetsu-embed>`. A business's theme is token values stored with the tenant.
- **Tenants stay on Tetsu's host, reached by path,** until [#8](https://github.com/heavymedl-labs/tetsu/issues/8)
  decides addresses, per-business subdomains, and custom domains.

Open, with owners: tools and packaging for the design system
([#86](https://github.com/heavymedl-labs/tetsu/issues/86)); the staff app, performance budgets, accessibility, and
native apps (#14); addresses and domains (#8); customer sign-in (#9); payment processors
([#10](https://github.com/heavymedl-labs/tetsu/issues/10)); and the API (#12). How each piece is built is in its
issue: embedding in [#89](https://github.com/heavymedl-labs/tetsu/issues/89), tokens and styles in
[#84](https://github.com/heavymedl-labs/tetsu/issues/84) and [#87](https://github.com/heavymedl-labs/tetsu/issues/87),
islands in [#88](https://github.com/heavymedl-labs/tetsu/issues/88).

## Alternatives considered

- **ADR-0030 as written.** Exposes the token to the host page's scripts, and has no control over who embeds.
- **Sign-in and payment inside a frame.** Keeps the flow on the business's page, but browsers block the session
  cookie, and the host page can overlay or imitate a signed-in frame.
- **Sign-in on a subdomain of the business's own domain.** First-party, so it works in Safari, but same-site with
  the host page, whose scripts can then post forms with the customer's cookies, and the business controls its DNS.
  It waits on #8.
- **Components on the host page behind a backend run by Tetsu.** Keeps tokens off the page, but the component, its
  styles, and its version still live on pages Tetsu does not control.
- **Links only.** Safe and simplest, but both real studios in the research show their schedule on their own pages.
- **Headless only, or Tetsu's own site in a JavaScript framework.** The API would come first, and live pages would
  need a second runtime.

## Consequences

- An embed shows exactly Tetsu's page, so look, behavior, and version live in one place.
- Booking from an embed opens a new tab on Tetsu's address. Both real studios keep the whole flow on their own
  pages, so this is the main cost, until #8 allows a business's own domain.
- The security-header layer stops being one constant and becomes Tier 1.
- The islands and `<tetsu-embed>` bring TypeScript or JavaScript, with its own CONSTRAINTS.md row first
  ([ADR-0014](0014-checks-and-duties.md)).
- Embedding waits on a way to update existing tenants, since seeding skips them
  ([ADR-0018](0018-demo-businesses-as-data.md)), and for real businesses on a paid plan
  ([ADR-0012](0012-first-hosting-and-deploys.md)).

**Weakest evidence.** No browser test of embedding yet, and no measure of drop-off at the new tab.

**Revisit** if framed sign-in becomes reliable and safe from the host page, if drop-off proves high, when #8
decides domains, or if the islands outgrow plain Web Components.

## Sources

- [Front-end model research](../research/front-end-model.md), 2026-11-09, with three reviews and an industry check.
- [Customer channels research](../research/market-scan/customer-channels.md), 2026-11-04.
- OWASP ASVS 5.0.0 [V3](https://github.com/OWASP/ASVS/blob/master/5.0/en/0x12-V3-Web-Frontend-Security.md):
  3.4.6, 3.5.1, 3.5.5.
