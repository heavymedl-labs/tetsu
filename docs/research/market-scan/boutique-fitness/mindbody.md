# Mindbody

Mindbody is the category's broadest platform and its largest two-sided network: studio software plus a consumer app and ClassPass. It is moving down-market, cutting its US entry price to $79 a month in July 2026, while its parent Playlist consolidates the industry.

## Takeaways for us

- Its $79 tier now bundles AI churn insights, a website and a marketplace listing, which raises the bar for v1.
- Its 2025 shared-login backlash argues for tenant-owned customer identity.
- We can't match a 3M-user marketplace; integrate ClassPass and Reserve with Google instead.

## At a glance

| Field | Value |
| --- | --- |
| Founded | 1998, San Luis Obispo, California |
| Ownership | Vista Equity (2019, $1.9B); parent brand Playlist since June 2025; Playlist merged with EGYM in March 2026 |
| Scale | 60,000+ businesses in 130+ countries (vendor claim) |
| Verticals | Fitness, wellness (chiropractic, physical therapy, med spa), beauty (salons, barbers) |
| Consumer reach | Mindbody app with 3M+ users; owns ClassPass (since 2021) |
| Ratings | Capterra 4.0 (about 2,990 reviews); G2 3.8 |

## Pricing and business model

- **Subscription:** Starter from $79 per location per month in the US, with unlimited staff users. Accelerate and Ultimate are quote-only.
- **Marketplace fee:** 20% of a new client's first purchase through the Mindbody app, capped at $30.
- **Payments:** a percentage rate plus $0.10 to $0.30 per transaction. Mindbody says its own processing isn't required.
- **Contracts:** reviewers report 12-month auto-renewing terms, early-termination penalties, and a data-export fee of about $500 at exit. Not confirmed by Mindbody.

## Strengths and complaints

### Strengths

- The largest discovery channel: a 3M-user consumer app, and ownership of ClassPass.
- Feature breadth across fitness, wellness and beauty, and global language coverage.
- A $79 entry price with unlimited staff logins and a website included.

### Complaints

- Support quality, the lowest-rated area in reviews.
- Opaque upper-tier pricing and reported mid-contract increases.
- Marketplace commission charged on clients studios feel they found themselves.
- A 2025 shared-login change that put customer identity under Mindbody's control was partly rolled back after studio backlash.
- An older stack: reviewers report booking widgets breaking under browsers' third-party cookie blocking.

## API

- REST API v6 (about 142 endpoints) using an `API-Key`, a per-studio `SiteId` and staff user tokens; sandbox site `-99`.
- HMAC-signed webhooks. No official OpenAPI spec; a competitor reports per-call fees of about $0.002.

## Feature map

Of 46 feature areas, 33 are platform or platform with config, 6 are fitness-specific, 4 are integrations, and 3 have no home in our plan yet.

| Area | Feature | Tag | Platform capability | Notes |
| --- | --- | --- | --- | --- |
| Identity | Staff logins, permission groups, manager vs. owner roles, IP restrictions | Platform | Identity, Authorization | Custom groups on Ultimate |
| Identity | Staff login locations | Platform | Authorization | Which sites a staffer can work from |
| Identity | Consumer account on the Mindbody app, shared across businesses | (new) | Identity | 2025 shared-login change partly rolled back |
| Org structure | Multi-location (datashare) with central staff and reporting | Platform | Tenancy, Analytics | Two setup options |
| Customers | Client profiles, notes, photos | Platform | Party |  |
| Customers | Liability waivers signed in the Business app | Platform | Documents |  |
| Customers | SOAP notes | Vertical | Documents | For massage, physical therapy; not fitness |
| Scheduling | Classes, appointments and events on one screen, overlaps allowed | Platform + config | Scheduling |  |
| Scheduling | Courses and workshops with enrollment | Platform | Scheduling, Catalog |  |
| Scheduling | Room and resource management, rentals | Platform + config | Scheduling | Accelerate+ |
| Scheduling | Recurring appointments | Platform + config | Scheduling | Staff or client booked |
| Scheduling | Buffer times and notice periods per service | Platform + config | Booking |  |
| Booking | Capacity, with separate online vs. total limits | Platform + config | Booking | Holds spots for walk-ins |
| Booking | Booking and cancellation rules, who can book | Platform + config | Booking |  |
| Booking | Automated waitlists with notify or auto-promote | Platform + config | Booking, Notifications |  |
| Booking | Pick-a-Spot with numbered room layouts | Vertical | Booking | Accelerate+ |
| Booking | Multichannel booking: website widgets, branded app, Mindbody app, Facebook | Platform + config | Public API | Changes sync everywhere |
| Attendance | Self check-in by tablet, barcode or client ID | Platform + config | Attendance |  |
| Attendance | Late-cancel and no-show fees | Vertical | Booking, Ledger | All plans |
| Commerce | Memberships, contracts, class packs | Platform | Subscriptions, Entitlements |  |
| Commerce | Intro offers | Vertical | Catalog, Entitlements |  |
| Commerce | Dynamic pricing for peak and off-peak | Platform + config | Pricing |  |
| Commerce | Promo codes | Platform | Pricing | Accelerate+ |
| Commerce | POS, retail, inventory, purchase orders and receiving | Platform | Orders, Inventory |  |
| Commerce | Online store | Platform | Catalog, Orders |  |
| Payments | Mindbody Payments (Stripe-powered), POS hardware | Integration | Payments | Optional |
| Messaging | Automatic confirmations and reminders; staff get copies | Platform | Notifications |  |
| Marketing | Email and SMS campaigns, automations, smart lists, intro-offer sequences | Platform | Segments, Workflows | Ultimate or add-on; built with Attentive |
| Marketing | Client referral program with offers | Vertical | Segments, Pricing |  |
| Marketing | Reviews shown in the Mindbody app | Platform + config | Party, Notifications |  |
| Sales | Lead pipeline; Google Ads lead forms | Platform | Party, Segments | Ultimate |
| Staff | Profiles, bios, schedules, self-managed availability | Platform | Staff |  |
| Staff | Substitutions requested by text, quick sub from schedule | Vertical | Staff, Notifications |  |
| Staff | Time clock, hourly pay, commissions, tips; payroll export to ADP, Paychex | Platform | Staff, Ledger |  |
| Staff | Staff contests and KPIs | Platform + config | Analytics | Insights add-on |
| Insights | Reports, dashboards, scheduled report delivery | Platform + config | Analytics |  |
| Insights | AI Insights: churn and spend predictions, benchmarks vs. about 40K businesses | Platform + config | Analytics, Domain events | Advanced on higher tiers |
| Platform | Customized website built by Playlist | Platform + config | Theming | All plans |
| Platform | Branded mobile app | Platform + config | Theming | Add-on |
| Platform | Business app for staff | Platform + config | Public API |  |
| Platform | Public API v6 and webhooks | Platform | Public API, Domain events | Developer account |
| AI | AI front desk for calls and messages | Integration | Notifications | Ultimate or add-on |
| Distribution | Consumer marketplace (3M+ users) | (new) | Tenancy (cross-tenant exception) | 20% of first purchase, capped at $30 |
| Integrations | ClassPass (owned) and Reserve with Google | Integration | Booking |  |
| Integrations | Partner marketplace: door access, payroll, reviews, streaming, waivers | Integration | Integrations |  |
| Finance | Mindbody Capital business loans | (new) | None | A financial service |

## Sources

- [Mindbody: new US pricing, $79 per month](https://www.mindbodyonline.com/business/education/blog/new-mindbody-pricing-united-states)
- [Mindbody: scheduling](https://www.mindbodyonline.com/en-gb/business/scheduling), [staff management](https://www.mindbodyonline.com/business/staff-management), [business management](https://www.mindbodyonline.com/business/business-management), [home page FAQ](https://www.mindbodyonline.com/), [marketing suite](https://mindbodyonline.com/business/education/blog/wellness-marketing-suite)
- [Pabau: Mindbody plan breakdown](https://pabau.com/blog/mindbody-pricing/) (competitor)
- [Vibefam: Mindbody review 2026](https://vibefam.com/mindbody-review-pricing-features-pros-cons-2026/) (competitor)
- [API Evangelist: Mindbody APIs](https://providers.apievangelist.com/providers/mindbody/), [Jentic: Mindbody Public API](https://jentic.com/apis/mindbodyonline)
