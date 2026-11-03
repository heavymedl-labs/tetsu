# Momence

Momence is the modern challenger that got acquired: a 2020 Y Combinator startup with a free tier, on-demand video and a self-serve API, now owned by Xplor, the same parent as Mariana Tek. Its pricing has shifted toward transaction fees, and reviews describe support slipping after the acquisitions.

## Takeaways for us

- Xplor now owns Momence and Mariana Tek; expect consolidation and studios wary of forced migrations.
- Its self-serve API is the bar. We should add machine-to-machine credentials, a sandbox tenant and reliable webhooks.
- The Host and Member split maps onto staff and customer scopes in our Authorization module.
- Its platform-wide customer login is the opposite of tenant-owned identity.

## At a glance

| Field | Value |
| --- | --- |
| Founded | 2020, San Francisco (Y Combinator Summer 2020) |
| Ownership | Clubessential Holdings (Jan 2025), which merged into Xplor Technologies (Mar 2026) |
| Sibling products at Xplor | Mariana Tek, ClubReady, Exerp, myFitApp |
| Scale | 4,500+ businesses at acquisition; about $15M revenue in 2024 (third-party estimate) |
| Verticals | Yoga, Pilates, dance, fitness, wellness, events |
| Ratings | Capterra 3.9 (76 reviews), support 3.7; consumer app 4.9 on the App Store |

## Pricing and business model

| Tier | Monthly | Momence fee on sales |
| --- | --- | --- |
| Basic | $0 | 5% (plus a client surcharge) |
| Pro | $60 | 2.5% |
| Custom | $199 | None |

- **Add-ons:** AI Agent at $399 a month. Reviewers report real spend of $200 to $2,000+ a month once add-ons and locations stack.
- **Processing:** reviewers cite a 1% surcharge above Stripe; a competitor cites 3.9% + $0.30 after a processor change.
- **Month-to-month** contracts are available.

## Strengths and complaints

### Strengths

- Free tier as a low-risk way in.
- Native on-demand video and courses, which most rivals outsource.
- Fast, modern UI; the consumer app is among the best rated.
- One of only two self-serve APIs in the group (with Arketa), and the only one with OAuth.

### Complaints

- Support declined after the acquisitions, with more canned and AI-style replies.
- Fee stacking: platform fees, processing surcharges, add-ons.
- Reports of payment-capture bugs and gift cards invalidated during an upgrade.
- Required backlinks to momence.com from studio websites.
- No spot-map booking documented.

## API

- REST v2 at `api.momence.com/api/v2`, split into a Host API (staff) and a Member API (the logged-in customer). OpenAPI 3.0 schema and an `llms.txt` index.
- Self-serve: studios create OAuth clients in the dashboard with no approval.
- Gaps: no machine-to-machine tokens, access tokens expire within hours, webhooks are experimental and enabled by support, no sandbox or SDK.

## Feature map

Of 38 feature areas, 29 are platform or platform with config, 2 are fitness-specific, 5 are integrations, and 2 have no home in our plan yet. Momence's commerce model is the most flexible of the challengers.

| Area | Feature | Tag | Platform capability | Notes |
| --- | --- | --- | --- | --- |
| Identity | Staff access levels (front desk and up) | Platform | Identity, Authorization | |
| Identity | Two-factor authentication | Platform | Identity | |
| Identity | One customer login across every Momence studio ("your hosts") | (new) | Identity | Platform-owned customer identity |
| Org structure | Franchise and corporate dashboard, multi-location | Platform | Tenancy, Analytics | |
| Customers | Profiles, notes, tags, CRM filters and segments | Platform | Party, Segments | |
| Customers | Child accounts, with subscriptions shared by the studio | Platform | Party, Entitlements | |
| Customers | Document signing: waivers, contracts, privacy policy | Platform | Documents | |
| Scheduling | Classes, events, private events | Platform + config | Scheduling | |
| Scheduling | Appointments by service, venue and staff; Zoom for appointments | Platform + config | Scheduling, Booking | |
| Scheduling | Recurring bookings | Platform + config | Booking | |
| Booking | Waitlists | Platform + config | Booking | |
| Booking | Cancellation policies per class | Platform + config | Booking | |
| Booking | Pay later; unpaid appointments paid afterwards | Platform + config | Orders | |
| Attendance | Self check-in; check-in and undo | Platform + config | Attendance | |
| Commerce | Subscriptions and packs, with minimum-renewal contracts | Platform | Subscriptions, Entitlements | |
| Commerce | Freeze; transfer to another membership at renewal | Platform | Subscriptions | Incompatible bookings auto-cancelled |
| Commerce | Money credits vs. class and appointment credits | Platform | Entitlements, Ledger | |
| Commerce | Activation on first booking | Platform + config | Entitlements | |
| Commerce | Intro offers | Vertical | Catalog, Entitlements | |
| Commerce | Discount codes with usage limits | Platform | Pricing | |
| Commerce | Sliding-scale pricing | Platform + config | Pricing | |
| Commerce | Products, retail, inventory, barcodes | Platform | Catalog, Inventory | |
| Commerce | Gift cards | Platform | Ledger | |
| Commerce | Refunds | Platform | Ledger | |
| Payments | Embedded processing with platform fees | Integration | Payments | Fees vary by plan |
| Messaging | Two-way SMS, email, unified inbox | Platform | Notifications | |
| Marketing | Automations, sequences, lead management | Platform | Segments, Workflows | |
| Marketing | Facebook Pixel, Meta and Google integrations | Integration | Analytics | |
| Engagement | Customer referrals with rewards; "SuperFans" | Vertical | Segments | |
| Staff | Payroll reporting; staff mobile apps | Platform | Staff | |
| Insights | Reports, class profitability meter, location reporting | Platform + config | Analytics | |
| AI | AI Agent and sales agent for email, SMS and inbox | Integration | Notifications | $399 a month |
| On-demand | Video library, collections, courses, rentals, scheduled drops, tiered video memberships | (new) | Media, Catalog | |
| Platform | Customizable checkout pages; multiple languages | Platform + config | Theming | |
| Platform | Consumer app (multi-studio) and staff apps | Platform + config | Public API | |
| Platform | Public API v2 (Host and Member), self-serve OAuth | Platform | Public API, Authorization | |
| Integrations | ClassPass, Gympass | Integration | Booking | |
| Integrations | Zoom, Shopify, Xero, QuickBooks, Kisi, Zapier, Calendly | Integration | Integrations | |

## Sources

- [Xplor: Momence joins Clubessential Holdings](https://xplor.com/press/momence-joins-clubessential-holdings/)
- [PR Newswire: Xplor and Clubessential unified company](https://www.prnewswire.com/news-releases/xplor-technologies-and-clubessential-holdings-launch-unified-global-company-under-xplor-brand-302728484.html)
- [Momence developer hub](https://api.docs.momence.com/), [getting started](https://api.docs.momence.com/docs/getting-started)
- [Momence help center](https://intercom.help/momence/en/articles/11943074-customer-onboarding-faqs); changelog [2020](https://momence.substack.com/sitemap/2020), [2021](https://momence.substack.com/sitemap/2021), [2023](https://momence.substack.com/sitemap/2023)
- [Frontmat: Momence after two acquisitions](https://frontmat.studio/blog/momence-fees-after-the-acquisitions.html)
- [Carly: Momence API guide](https://www.usecarly.com/blog/momence-api/)
- [Supergood: Momence API report card](https://supergood.ai/api-report-card/momence) (sells an unofficial API)
- [Vibefam: Momence review 2026](https://vibefam.com/momence-review-pricing-features-pros-cons-2026/) (competitor)
