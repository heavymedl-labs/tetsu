# Customer channels

How 14 competitors let a business's customers book and pay. Read 2026-11-04, adding to the
[boutique fitness scan](boutique-fitness/compare.md) of 2026-11-01. This is research, not a
decision. Where it disagrees with an ADR, the ADR wins.

## Findings

1. **Nobody offers just one way in.** Competitors layer channels on one shared system, so
   the choice is which to build first and which to charge for.
2. **The business's own front door is the baseline.** All 14 offer a hosted booking page, a
   widget for the business's own site, or both. At least five also offer a website builder.
3. **Branded native apps are common, but usually cost extra.** They are an add-on or top-tier
   feature at most competitors, and Mariana Tek stands out for including one.
4. **Sending every customer to the vendor's own app is the minority model.** Six of the 14
   have a marketplace or a directory. These vendors use it for discovery, and most pair it
   with the business's own booking page or widget.
5. **Marketplaces create the conflict over who owns the customer.** Fees on new clients are
   the main source of it, and a vendor's own app makes the customer feel they belong to the
   vendor. The scan already records Mindbody's backlash over shared logins.

## The four models

| Model | Example | Customer experience | Cost to the business | Fits Tetsu's trust position |
| --- | --- | --- | --- | --- |
| Booking page or widget for the business | Walla, GlossGenius | Stays with the business's brand, no app to install | Usually included | Yes |
| Website built by the platform | GlossGenius, Mindbody | A whole site, with booking built in | Included or an extra fee | Yes, but it is a large product to build |
| Branded native app | Mariana Tek, Arketa | The business's own app on the phone | Often an add-on, and may need the business's own developer account | Yes |
| Central marketplace or app | Mindbody, Fresha, Booksy | One app for every business | Free listing, often with a fee on new clients | Weakly |

## By competitor

| Product | Own site | Website builder | Branded app | Marketplace or central app |
| --- | --- | --- | --- | --- |
| Mindbody | Widgets, on every plan | Included | Add-on | Mindbody app, listed free, 20% of a new client's first purchase, capped at $30 |
| Mariana Tek | Web integrations, Customer API | Not found | Included | No |
| ABC Glofox | Portal embeds | Embeds only | Add-on or top tier | No |
| WellnessLiving | Widgets | Extra fee | Top tier | Directory |
| Momence | Checkout pages | Checkout pages only | Sources conflict | Multi-studio app |
| Arketa | Embedded booking | Embeds only | Yes, customizable | No |
| Walla | Widgets and direct links | Separate offering | Yes | No |
| Vagaro | Widget | Yes | Extra, plus developer account fees | Marketplace |
| Fresha | Booking links, but requires a marketplace listing | Not checked | Not checked | Marketplace, one-time fee on new clients |
| Booksy | Widget | Not checked | Not checked | Listed free and can be turned off, with paid promotion |
| GlossGenius | Booking site with no app or login needed | Included, from $24 a month | Not checked | None described |
| Boulevard | Self-booking on the salon's site | Not checked | Not checked | Not found |
| Square Appointments | Free booking site and widgets | Not checked | Not checked | Not checked |
| Acuity | Booking page, embed, and link | Not checked | Not checked | Not checked |

The first seven rows draw on the earlier scan, and we confirmed Mindbody, Mariana Tek, and
Walla on their own pages. "Not checked" means we did not look, not that the feature is absent.

## Two real studios

[Intent Hot Yoga and Ride Culture](studio-evidence/README.md), both in Bellingham, Washington, run on
Mariana Tek. Each has its own website with booking embedded in its pages, and its own branded app
published under the studio's name. That matches the Apple rule below, and neither sends customers
to a vendor's app. Their screens are nearly identical apart from colors and logos.

## Constraints that shape the choice

- **Apple limits templated apps.** [Guideline 4.2.6](https://developer.apple.com/app-store/review/guidelines/)
  rejects apps from a template or generation service unless the content's provider submits
  them. The other option is one app with a picker. So a branded app per business likely means
  each business needs its own developer account, as Vagaro's fee note suggests, and a single
  app is the central model.
- **Embedded logins are fragile.** Safari treats a widget on another domain as third-party and
  [rejects its session cookies](https://www.webkit.org/blog/8124/introducing-storage-access-api),
  and reviewers report Mindbody's widgets breaking this way. Sources are old, so test this. A
  business's own subdomain pointing at Tetsu, or linking out to a hosted page, avoids it.
- **Discovery can come from elsewhere.** ClassPass integration is universal in the scan, and
  Reserve with Google works through software partners. Whether Tetsu could become a partner
  was not confirmed.

## What this means for Tetsu

Both reference businesses point the same way. Anvil Row needs booking from a phone browser, and
Juniper Lane needs online booking. Both list native apps under what can wait, and both owners
would leave over losing their data or being unable to take it out.

A leading option for [#14](https://github.com/heavymedl-labs/tetsu/issues/14), in build order:

1. **A hosted booking site per business,** on a Tetsu subdomain or the business's own domain,
   working well in a phone browser.
2. **Direct links and an embed for a business's own site.** Both real studios keep booking on their
   own pages, so this should follow soon after the hosted site, if embedded login passes testing.
3. **Integrations for discovery:** Reserve with Google and ClassPass.
4. **Branded native apps later,** as a paid option. Even the two single-location studios have one,
   though Mariana Tek includes it, so this may arrive sooner than the reference businesses suggest.
5. **No Tetsu marketplace.** It works against winning on trust, needs consumer demand we cannot
   match, and conflicts with [customer accounts owned by the business](https://github.com/heavymedl-labs/tetsu/issues/9).

A full website builder stays out of scope for now, though a simple site may become necessary
because Mindbody and GlossGenius include one.

The architecture already fits this. The server renders each business's booking pages, and it
would resolve the tenant from the request's host, which also supports moving tenants to cells.
On hosting, Render includes 15 custom domains on Pro at $0.25 for each extra, and supports
wildcards, so subdomains are cheap.

## Weakest evidence

- **Usage.** We found no independent data on how many customers install branded apps, or book
  through marketplaces versus a business's own page. Nearly all claims come from vendors.
- **Coverage.** Salon competitors got a lighter check than fitness, and several come from
  vendor pages or review sites. Boulevard's and Momence's rows are the weakest.
- **Owner preferences.** We have not asked real owners. The reference businesses are fictional.

## Sources

- Vendor pages: [Mindbody pricing](https://www.mindbodyonline.com/en-au/pricing),
  [Mariana Tek's ways to build](https://guides.marianatek.com/ways-to-build),
  [Walla widgets](https://support.hellowalla.com/prepare-your-website-and-widgets-for-client-interaction-walla-support-center),
  [Arketa's branded app](https://arketa.com/blog/new-branded-mobile-app),
  [Fresha's new client fees](https://www.fresha.com/help-center/knowledge-base/billing-and-fees/101357-marketplace-new-client-fees),
  [Booksy's marketplace](https://biz.booksy.com/en-us/features/marketplace),
  [Booksy's widget](https://biz.booksy.com/en-us/blog/a-new-booksy-widget-will-help-you-attract-clients),
  [GlossGenius's website builder](https://glossgenius.com/custom-website-builder),
  and [Boulevard's integrations](https://joinboulevard.com/integrations).
- Review sites, weaker: [Square and Acuity compared](https://koalendar.com/scheduling-software-comparison/acuity-scheduling-vs-square-appointments),
  [Vagaro on Capterra](https://www.capterra.com/p/153752/Vagaro/reviews/Capterra___6773670/),
  and [Momence's listing](https://www.softwares.com/software/momence).
- Reserve with Google: [Mindbody's announcement](https://www.mindbodyonline.com/company/press/mindbody-partners-with-google-to-pilot-new-fitness-booking-integration)
  and [Arketa's help page](https://help.arketa.com/marketing/reserve-with-google).
- [Studio evidence](studio-evidence/README.md): two real studios, 2026-11-04.
- [Hosting research](../hosting.md): Render's domain limits.
