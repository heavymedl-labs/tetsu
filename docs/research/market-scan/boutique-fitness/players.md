# Players

The seven boutique fitness platforms, read 2026-11-01. The [Compare](compare.md) matrix replaces each one's feature map. The full maps are in git history, such as `git show f46cbef:docs/research/market-scan/boutique-fitness/mindbody.md`.

## Mariana Tek

Mariana Tek is the premium specialist: built only for class-based boutique studios, strongest on the branded client experience and multi-location brands, and sold entirely by quote. Its main weakness is visible in its own help center, where an acquired marketing product still runs as a separate system.

Takeaways for us:

- The closest match to our fitness vertical; its brand-then-studio settings split validates our Tenancy design.
- Xplor Growth runs as a second customer database. One Party module for all customer data avoids that seam.
- Its gated API is an opening for self-serve keys.

At a glance:

| Field | Value |
| --- | --- |
| Founded | About 2014, Washington, DC |
| Ownership | Xplor Technologies (merged with Clubessential in March 2026); zingfit was folded into it |
| Focus | Pilates, Lagree, yoga, cycling, HIIT, barre, group fitness; single studios to franchises |
| Customers | "Thousands" of studios (vendor claim); over 70% migrated from other software |
| Ratings | Capterra 4.5 (62 reviews); Software Advice 4.3 (97 reviews) |

Pricing and business model:

- **Three bundles, all quote-only:** Core, Core + Growth, and Full Growth Bundle. All include unlimited instructors and classes.
- **Payments:** Stripe only, with Stripe's processing fees on top.
- **Reported spend:** around $300 to $500 a month for growing studios, per one third-party tracker. Unconfirmed.
- **Services:** paid marketing services, plus Xplor Capital financing and Xplor Insurance.

Strengths:

- Deepest boutique-specific workflows: spot booking, standby, penalties, add-ons.
- Branded app included rather than sold as an add-on.
- Real franchise tooling (royalty collection, HQ content syncing).

Complaints:

- Quote-only pricing.
- Some reviewers report slow help with complex settings.
- Xplor Growth has its own contacts, users, settings and billing, separate from studio customer management. Two customer databases is a seam studios have to manage.

API:

- Seven APIs (Admin, Customer, Studio, Stripe, Documents, Pricing, Advertising) plus webhooks; API-key or OAuth 2.0 auth; a sandbox.
- Production access needs a partner survey, signed terms, a 1 to 3 week review and activation per studio.

Sources:

- [Mariana Tek Knowledge Base](https://support.marianatek.com/en/)
- [Mariana Tek: brand vs. studio settings](https://support.marianatek.com/en/articles/5640165-which-key-settings-in-mariana-tek-are-configured-at-the-brand-vs-studio-level)
- [Mariana Tek: group fitness software](https://www.marianatek.com/group-fitness-studio-software/)
- [Pulse Signal: Mariana Tek plans](https://getpulsesignal.com/compare/classpass-vs-marianatek)
- [Supergood: Mariana Tek API report card](https://supergood.ai/api-report-card/mariana-tek) (sells an unofficial API)
- [Capterra: Mariana Tek](https://capterra.com/p/175281/Mariana-Tek/)

## Momence

Momence is the modern challenger that got acquired: a 2020 Y Combinator startup with a free tier, on-demand video and a self-serve API, now owned by Xplor, the same parent as Mariana Tek. Its pricing has shifted toward transaction fees, and reviews describe support slipping after the acquisitions.

Takeaways for us:

- Xplor now owns Momence and Mariana Tek; expect consolidation and studios wary of forced migrations.
- Its self-serve API is the bar. We should add machine-to-machine credentials, a sandbox tenant and reliable webhooks.
- The Host and Member split maps onto staff and customer scopes in our Authorization module.
- Its platform-wide customer login is the opposite of tenant-owned identity.

At a glance:

| Field | Value |
| --- | --- |
| Founded | 2020, San Francisco (Y Combinator Summer 2020) |
| Ownership | Clubessential Holdings (Jan 2025), which merged into Xplor Technologies (Mar 2026) |
| Sibling products at Xplor | Mariana Tek, ClubReady, Exerp, myFitApp |
| Scale | 4,500+ businesses at acquisition; about $15M revenue in 2024 (third-party estimate) |
| Verticals | Yoga, Pilates, dance, fitness, wellness, events |
| Ratings | Capterra 3.9 (76 reviews), support 3.7; consumer app 4.9 on the App Store |

Pricing and business model:

| Tier | Monthly | Momence fee on sales |
| --- | --- | --- |
| Basic | $0 | 5% (plus a client surcharge) |
| Pro | $60 | 2.5% |
| Custom | $199 | None |

- **Add-ons:** AI Agent at $399 a month. Reviewers report real spend of $200 to $2,000+ a month once add-ons and locations stack.
- **Processing:** reviewers cite a 1% surcharge above Stripe; a competitor cites 3.9% + $0.30 after a processor change.
- **Month-to-month** contracts are available.

Strengths:

- Free tier as a low-risk way in.
- Native on-demand video and courses, which most rivals outsource.
- Fast, modern UI; the consumer app is among the best rated.
- One of only two self-serve APIs in the group (with Arketa), and the only one with OAuth.

Complaints:

- Support declined after the acquisitions, with more canned and AI-style replies.
- Fee stacking: platform fees, processing surcharges, add-ons.
- Reports of payment-capture bugs and gift cards invalidated during an upgrade.
- Required backlinks to momence.com from studio websites.
- No spot-map booking documented.

API:

- REST v2 at `api.momence.com/api/v2`, split into a Host API (staff) and a Member API (the logged-in customer). OpenAPI 3.0 schema and an `llms.txt` index.
- Self-serve: studios create OAuth clients in the dashboard with no approval.
- Gaps: no machine-to-machine tokens, access tokens expire within hours, webhooks are experimental and enabled by support, no sandbox or SDK.

Sources:

- [Xplor: Momence joins Clubessential Holdings](https://xplor.com/press/momence-joins-clubessential-holdings/)
- [PR Newswire: Xplor and Clubessential unified company](https://www.prnewswire.com/news-releases/xplor-technologies-and-clubessential-holdings-launch-unified-global-company-under-xplor-brand-302728484.html)
- [Momence developer hub](https://api.docs.momence.com/), [getting started](https://api.docs.momence.com/docs/getting-started)
- [Momence help center](https://intercom.help/momence/en/articles/11943074-customer-onboarding-faqs); changelog [2020](https://momence.substack.com/sitemap/2020), [2021](https://momence.substack.com/sitemap/2021), [2023](https://momence.substack.com/sitemap/2023)
- [Frontmat: Momence after two acquisitions](https://frontmat.studio/blog/momence-fees-after-the-acquisitions.html)
- [Carly: Momence API guide](https://www.usecarly.com/blog/momence-api/)
- [Supergood: Momence API report card](https://supergood.ai/api-report-card/momence) (sells an unofficial API)
- [Vibefam: Momence review 2026](https://vibefam.com/momence-review-pricing-features-pros-cons-2026/) (competitor)

## Arketa

Arketa is the venture-backed independent expanding from booking into a full "operating system": payroll with tax filing, accounting, on-demand video and events, all in one bundle. It wins switchers from Mindbody and Mariana Tek mostly on support and UX, and its API is open but minimal.

Takeaways for us:

- All-in-one now reaches payroll tax filing and bookkeeping. Our Ledger covers most accounting reports; payroll tax filing would be a regulated integration. Decide deliberately whether to follow.
- Its service-type model, with pricing options explicitly linked to what they can book, is a template for Catalog and Entitlements.
- Events and trainings belong in the platform's Scheduling and Catalog modules, not as vertical bolt-ons.
- A sandbox tenant would put us ahead of every challenger.

At a glance:

| Field | Value |
| --- | --- |
| Founded | 2020 (as Sutra) by Rachel Lea Fishman and Josh Archer; New York |
| Funding | About $23M total: $7.6M seed (2022, First Round) and $15M Series A (Jun 2025, Inspired Capital); Y Combinator backed |
| Scale | 3,000+ studios; nearly $500M in transactions processed; about 55 to 110 staff |
| Customers | Yoga, Pilates, gyms, recovery; 92% women-led businesses (vendor claim) |
| Ratings | Capterra 4.3 (111 reviews), with ease, value and support each 4.3 |

Pricing and business model:

- **Quote-based,** with no published tiers. The vendor promises no mid-contract increases and no features moved between tiers.
- **Bundled:** texting, subbing, reviews and video are included rather than metered.
- **Processing:** flat 2.9% + $0.30 on Stripe.
- **No long-term contract;** migration of clients, packages and stored cards is included.

Strengths:

- Support is the most-cited reason for switching to it, including from Mariana Tek.
- The widest scope in the group: payroll, tax filing and accounting are built in.
- Fast feature cadence and a clean UI.

Complaints:

- Bugs in sign-ups, payments, time zones and membership changes during busy release periods.
- A steep setup curve; onboarding doesn't cover everything.
- Payroll reporting is thinner than dedicated tools.
- Offshore support time zones frustrate some US studios.

API:

- Public API with self-serve keys (Settings, Integrations); docs on GitHub Pages; 25 requests per second.
- No sandbox (every call hits live data) and no dedicated API support.

Sources:

- [Arketa help center](https://help.arketa.com/getting-started/welcome): [schedule](https://help.arketa.com/schedule/overview), [pricing](https://help.arketa.com/pricing/overview), [public API](https://help.arketa.com/settings/integrations/api)
- [Arketa: Series A announcement](https://arketa.com/blog/series-a-fundraise)
- [Arketa vs. Momence](https://www.arketa.com/compare/arketa-vs-momence) (vendor comparison)
- [Capterra: Arketa](https://www.capterra.com/p/212718/Arketa/)
- [Yespress: Arketa profile](https://yespress.io/arketa)

## Walla

Walla is the smallest, most focused player: a premium, published-price platform for boutique studios that sells retention intelligence ("WallaPredict") and polish rather than breadth. It has a public API and webhooks, but a small review base that is mostly vendor-referred.

Takeaways for us:

- A premium price works when reliability and support are visibly better, which is what our engineering standards aim at.
- Its "no unpaid reservations" rule is worth enforcing as an invariant in our Booking types.
- Retention prediction fits our Analytics read models over domain events; no separate system needed.
- A single responsive web app is a credible v1, and a data point for PWA vs. native.

At a glance:

| Field | Value |
| --- | --- |
| Founded | 2020, San Diego, by Laura Munkholm (CEO) and Doug Hecht |
| Funding | About $18M: seed, $8M Series A (2022), and $5M (Oct 2025, Social Leverage and Ankona Capital) |
| Markets | United States and Canada |
| Focus | Pilates, Lagree, yoga, boutique fitness; single to multi-location |
| Ratings | Capterra 4.9 (25 reviews), but most were invited by the vendor with an incentive |

Pricing and business model:

| Plan | Per location, monthly | Notes |
| --- | --- | --- |
| Core | From $320 | Unlimited users |
| Pro | From $599 | Adds the full marketing suite |

- **The highest published entry price in the group,** framed as replacing several separate tools.
- **No free trial or free version** listed.

Strengths:

- Reviews consistently praise ease of use for staff and clients, and reliability compared with Mindbody and WellnessLiving.
- Responsive, founder-visible support (monthly webinars, frequent releases).
- Key integrations such as ClassPass are built in, with Zapier for the rest.

Complaints:

- Price is high for small studios.
- Reporting gaps (flexible date ranges, intro-offer models).
- Inventory and detailed accounting were thin or missing per older reviews.
- The review base is small and mostly vendor-referred, so ratings say less than they appear to.

API:

- Public REST API for members, classes, bookings and payments, plus webhooks for booking, payment, cancellation and membership events (e.g. `booking.confirmed`).
- Access, auth and rate limits aren't publicly documented; confirm in a demo.
- One responsive web app across devices rather than separate native apps, per a reviewer.

Sources:

- [Walla: integrations, API and webhooks](https://www.hellowalla.com/features/integrations-2), [Pilates studio software](https://hellowalla.com/made-for/pilates), [retail report](https://www.hellowalla.com/blog/retail-report-gives-you-insight-into-inventory)
- Walla support: [payroll](https://support.hellowalla.com/en/articles/9080392-payroll), [payroll settings](https://support.hellowalla.com/payroll-settings-walla-support-center), [important things to know](https://support.hellowalla.com/en/articles/9203767-important-things-to-know-about-using-walla)
- [Capterra: Walla](https://capterra.com/p/265361/Walla/)
- [Techjockey: Walla plans](https://www.techjockey.com/detail/walla)
- [Techleap: Walla raises $5M](https://finder.techleap.nl/news/feed/walla-secures-5m-for-ai-fitness-software), [Social Leverage: Walla Series A](https://socialleverage.beehiiv.com/p/social-leverage-letter-issue-46)

## Mindbody

Mindbody is the category's broadest platform and its largest two-sided network: studio software plus a consumer app and ClassPass. It is moving down-market, cutting its US entry price to $79 a month in July 2026, while its parent Playlist consolidates the industry.

Takeaways for us:

- Its $79 tier now bundles AI churn insights, a website and a marketplace listing, which raises the bar for v1.
- Its 2025 shared-login backlash argues for tenant-owned customer identity.
- We can't match a 3M-user marketplace; integrate ClassPass and Reserve with Google instead.

At a glance:

| Field | Value |
| --- | --- |
| Founded | 1998, San Luis Obispo, California |
| Ownership | Vista Equity (2019, $1.9B); parent brand Playlist since June 2025; Playlist merged with EGYM in March 2026 |
| Scale | 60,000+ businesses in 130+ countries (vendor claim) |
| Verticals | Fitness, wellness (chiropractic, physical therapy, med spa), beauty (salons, barbers) |
| Consumer reach | Mindbody app with 3M+ users; owns ClassPass (since 2021) |
| Ratings | Capterra 4.0 (about 2,990 reviews); G2 3.8 |

Pricing and business model:

- **Subscription:** Starter from $79 per location per month in the US, with unlimited staff users. Accelerate and Ultimate are quote-only.
- **Marketplace fee:** 20% of a new client's first purchase through the Mindbody app, capped at $30.
- **Payments:** a percentage rate plus $0.10 to $0.30 per transaction. Mindbody says its own processing isn't required.
- **Contracts:** reviewers report 12-month auto-renewing terms, early-termination penalties, and a data-export fee of about $500 at exit. Not confirmed by Mindbody.

Strengths:

- The largest discovery channel: a 3M-user consumer app, and ownership of ClassPass.
- Feature breadth across fitness, wellness and beauty, and global language coverage.
- A $79 entry price with unlimited staff logins and a website included.

Complaints:

- Support quality, the lowest-rated area in reviews.
- Opaque upper-tier pricing and reported mid-contract increases.
- Marketplace commission charged on clients studios feel they found themselves.
- A 2025 shared-login change that put customer identity under Mindbody's control was partly rolled back after studio backlash.
- An older stack: reviewers report booking widgets breaking under browsers' third-party cookie blocking.

API:

- REST API v6 (about 142 endpoints) using an `API-Key`, a per-studio `SiteId` and staff user tokens; sandbox site `-99`.
- HMAC-signed webhooks. No official OpenAPI spec; a competitor reports per-call fees of about $0.002.

Sources:

- [Mindbody: new US pricing, $79 per month](https://www.mindbodyonline.com/business/education/blog/new-mindbody-pricing-united-states)
- [Mindbody: scheduling](https://www.mindbodyonline.com/en-gb/business/scheduling), [staff management](https://www.mindbodyonline.com/business/staff-management), [business management](https://www.mindbodyonline.com/business/business-management), [home page FAQ](https://www.mindbodyonline.com/), [marketing suite](https://mindbodyonline.com/business/education/blog/wellness-marketing-suite)
- [Pabau: Mindbody plan breakdown](https://pabau.com/blog/mindbody-pricing/) (competitor)
- [Vibefam: Mindbody review 2026](https://vibefam.com/mindbody-review-pricing-features-pros-cons-2026/) (competitor)
- [API Evangelist: Mindbody APIs](https://providers.apievangelist.com/providers/mindbody/), [Jentic: Mindbody Public API](https://jentic.com/apis/mindbodyonline)

## ABC Glofox

ABC Glofox is the franchise and multi-country specialist, now one brand inside ABC Fitness's gym-software portfolio. It has the most engineering-mature public API of the incumbents, but post-acquisition reviews describe weaker support and price hikes.

Takeaways for us:

- Its webhook contract is a good template for our outbox, and its price-breakdown endpoint matches our Pricing module.
- International franchises need localization, VAT and local payment methods, so the Ledger needs currency and tax from day one.
- Its weakness is post-acquisition service, not product.

At a glance:

| Field | Value |
| --- | --- |
| Founded | 2014, Dublin, Ireland |
| Ownership | ABC Fitness Solutions (Thoma Bravo), acquired August 2022 for a reported $200M plus earn-out |
| Sibling products | ABC Ignite (gyms), ABC Trainerize (coaching), ABC XLerate (marketing CRM), ABC GymSales |
| Footprint | 80+ countries, 17 languages (vendor claim) |
| Notable customers | Club Pilates, Pure Barre, Rumble, CycleBar (Xponential brands), BHOUT, SETS |
| Ratings | Capterra 4.4 (354 reviews); G2 4.5; Trustpilot 3.6 |

Pricing and business model:

- **Four tiers, priced by quote** except a "from $99 a month" headline. Operators report $110 to $500+ a month.
- **Branded app and XLerate** are add-ons on Essential and Boost, included on Elite.
- **Processing** runs through Stripe (about 2.9% + $0.30) or GoCardless (about 1%).
- **No per-seat staff fees** (vendor claim). Go-live typically takes 4 to 6 weeks.

Strengths:

- Franchise and multi-country depth: localization, VAT, local payment methods.
- Branded app design is well reviewed (App Store 4.5).
- A cross-sell ecosystem for gyms, coaching and sales.

Complaints:

- Reviews repeatedly say support worsened "since the company was sold".
- A widely cited 70% mid-contract price increase.
- Headcount reportedly fell about 29% from 2024 to 2026, consistent with the support complaints.
- The member app rates lower on Google Play (3.9).

API:

- REST with a published OpenAPI spec; every call is branch-scoped (`x-glofox-branch-id`) and uses an API key and token issued on email request.
- CDC webhooks are HMAC-SHA256 signed and at-least-once (5-second timeout, three retries, soft deletes). A price-breakdown endpoint owns discount and tax math.

Sources:

- [ABC Glofox API developer portal](https://gf-dev-docs.glofox.com/), [CDC webhooks](https://gf-dev-docs.glofox.com/cdc-webhooks/), [request access](https://gf-dev-docs.glofox.com/request-access/), [booking flow](https://gf-dev-docs.glofox.com/flows/book/)
- [Glofox help center](https://support.glofox.com/), [getting started](https://support.glofox.com/hc/en-us/categories/46200714763284-Getting-Started), [memberships and credits](https://support.glofox.com/hc/en-us/sections/46375645361812-Step-6-Memberships-and-Credits), [staff permissions](https://support.glofox.com/hc/en-us/articles/52585050344852), [scheduling](https://www.glofox.com/features/scheduling/)
- [Glofox: Is Glofox part of a bigger company?](https://www.glofox.com/blog/is-glofox-part-of-a-bigger-company/)
- [FitGap: ABC Glofox plans](https://us.fitgap.com/products/025414/abc-glofox)
- [Vibefam: Glofox review 2026](https://vibefam.com/glofox-review-pricing-features-pros-cons-2026/) (competitor)

## WellnessLiving

WellnessLiving is the founder-led generalist: the widest vertical range (fitness through salon and spa) at published prices, with a large OpenAPI-documented API. Its weak spots are a locked-in payment processor, a poorly rated client app, and billing practices that draw formal complaints.

Takeaways for us:

- One spec for internal and partner clients is the right model; we can generate SDKs in several languages from ours.
- Processor lock-in is a top complaint, so a Payments port with more than one adapter is a selling point.
- Regional hosts point to a later need: a tenant registry that routes tenants to regional databases.

At a glance:

| Field | Value |
| --- | --- |
| Founded | 2009, Richmond Hill (Toronto area), Ontario, Canada |
| Ownership | Founder-led; US$66M minority growth round in Aug 2022 (McCarthy Capital equity, CIBC debt) |
| Staff | About 385 (LinkedIn, Apr 2026) |
| Verticals | Yoga, Pilates, fitness, martial arts, dance, salon, spa, wellness |
| Data centers | United States and Australia |
| Ratings | Capterra 4.4 (611 reviews); G2 about 4.5; BBB 1.1 (26 reviews, 24 complaints in 3 years) |

Pricing and business model:

| Tier | Monthly | Annual, per month | Staff |
| --- | --- | --- | --- |
| Starter | $69 | $62 | 1 |
| Business | $199 | $179 | Unlimited |
| BusinessPro | $349 | $314 | Unlimited |
| Enterprise | Quote | Quote | Unlimited |

- **Processing is locked to Paragon or Nuvei;** Stripe isn't offered. Reviewers report PCI and annual processor fees on top.
- **Contracts** auto-renew annually with a 30-day notice window.
- **Free data migration** on all plans.

Strengths:

- Broadest vertical coverage, which suits multi-format wellness businesses.
- Published prices and a low entry tier.
- Onboarding and marketing automation are consistently praised.
- Founder-led, without a private-equity parent.

Complaints:

- Processor lock-in, with fees reviewers say double or triple real cost.
- The client app rates about 2 on Google Play and 3 on the App Store.
- Reports of add-ons enrolled without consent and hard cancellations.
- In-product AI upsell pop-ups.
- Weaker fit for one-to-one appointment businesses.

API:

- An official OpenAPI spec (about 324 paths) that its own Achieve and Elevate apps also use; a generated PHP SDK plus JavaScript; US and Australia hosts.
- Access is gated by a questionnaire, NDA, API agreement and account-executive approval.

Sources:

- [WellnessLiving feature menu and June 2018 release notes](https://www.wellnessliving.com/blog/june-2018-release-notes/), [August 2018 release notes](https://www.wellnessliving.com/blog/august-2018-release-notes)
- [WellnessLiving PHP SDK (OpenAPI)](https://github.com/wellnessliving/wl-openapi-php)
- [StackScored: WellnessLiving pricing](https://www.stackscored.com/pricing/fitness-studio-management/wellnessliving/)
- [API Evangelist: WellnessLiving OpenAPI subset](https://raw.githubusercontent.com/api-evangelist/wellnessliving/refs/heads/main/openapi/wellnessliving-payments-api-openapi.yml)
- [Vibefam: WellnessLiving review 2026](https://vibefam.com/wellnessliving-review-pricing-features-pros-cons-2026/) (competitor)
- [Supergood: WellnessLiving API report card](https://supergood.ai/api-report-card/wellnessliving) (sells an unofficial API)
