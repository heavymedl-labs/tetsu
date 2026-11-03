# ABC Glofox

ABC Glofox is the franchise and multi-country specialist, now one brand inside ABC Fitness's gym-software portfolio. It has the most engineering-mature public API of the incumbents, but post-acquisition reviews describe weaker support and price hikes.

## Takeaways for us

- Its webhook contract is a good template for our outbox, and its price-breakdown endpoint matches our Pricing module.
- International franchises need localization, VAT and local payment methods, so the Ledger needs currency and tax from day one.
- Its weakness is post-acquisition service, not product.

## At a glance

| Field | Value |
| --- | --- |
| Founded | 2014, Dublin, Ireland |
| Ownership | ABC Fitness Solutions (Thoma Bravo), acquired August 2022 for a reported $200M plus earn-out |
| Sibling products | ABC Ignite (gyms), ABC Trainerize (coaching), ABC XLerate (marketing CRM), ABC GymSales |
| Footprint | 80+ countries, 17 languages (vendor claim) |
| Notable customers | Club Pilates, Pure Barre, Rumble, CycleBar (Xponential brands), BHOUT, SETS |
| Ratings | Capterra 4.4 (354 reviews); G2 4.5; Trustpilot 3.6 |

## Pricing and business model

- **Four tiers, priced by quote** except a "from $99 a month" headline. Operators report $110 to $500+ a month.
- **Branded app and XLerate** are add-ons on Essential and Boost, included on Elite.
- **Processing** runs through Stripe (about 2.9% + $0.30) or GoCardless (about 1%).
- **No per-seat staff fees** (vendor claim). Go-live typically takes 4 to 6 weeks.

## Strengths and complaints

### Strengths

- Franchise and multi-country depth: localization, VAT, local payment methods.
- Branded app design is well reviewed (App Store 4.5).
- A cross-sell ecosystem for gyms, coaching and sales.

### Complaints

- Reviews repeatedly say support worsened "since the company was sold".
- A widely cited 70% mid-contract price increase.
- Headcount reportedly fell about 29% from 2024 to 2026, consistent with the support complaints.
- The member app rates lower on Google Play (3.9).

## API

- REST with a published OpenAPI spec; every call is branch-scoped (`x-glofox-branch-id`) and uses an API key and token issued on email request.
- CDC webhooks are HMAC-SHA256 signed and at-least-once (5-second timeout, three retries, soft deletes). A price-breakdown endpoint owns discount and tax math.

## Feature map

Of 46 feature areas, 36 are platform or platform with config, 4 are fitness-specific, and 6 are integrations. Glofox is the most gym-like of the group: facilities and courses sit beside classes.

| Area | Feature | Tag | Platform capability | Notes |
| --- | --- | --- | --- | --- |
| Identity | Staff roles (Super Admin, Admin, Receptionist, Trainer) with a granular permission matrix | Platform | Identity, Authorization | Trainers can be limited to their own classes |
| Identity | Hierarchical access across linked branches | Platform | Authorization, Tenancy | |
| Org structure | Branches, linked branches, multi-site member access | Platform | Tenancy | Every API call is branch-scoped |
| Org structure | Bulk multi-location configuration of classes, memberships, settings | Platform | Tenancy | Elite |
| Customers | Client profiles, global search, post-migration login | Platform | Party | |
| Customers | Family accounts | Platform | Party | |
| Customers | Lead capture forms and lead management | Platform | Party, Segments | Boost |
| Customers | E-agreements and contracts | Platform | Documents | Boost |
| Scheduling | Classes | Platform + config | Scheduling | |
| Scheduling | Multi-week courses | Platform | Scheduling, Catalog | Priced and booked as a unit |
| Scheduling | Appointments with trainer-managed availability and conflict checks | Platform + config | Scheduling, Staff | |
| Scheduling | Facilities (rooms, courts, saunas) with their own schedules and prices | Platform + config | Scheduling | |
| Scheduling | Categories for classes, facilities and trainers | Platform | Catalog | |
| Booking | Booking windows, cancellation rules, price and eligibility rules | Platform + config | Booking, Pricing | |
| Booking | Waitlists that auto-book and auto-charge when a spot opens | Platform + config | Booking, Payments | Staff step in if no credit or card |
| Booking | Spot management | Vertical | Booking | |
| Booking | Guest bookings | Vertical | Booking | |
| Booking | Free class bookings | Platform + config | Pricing | |
| Attendance | Check-in from staff app; access control | Platform + config | Attendance | |
| Attendance | Late-cancel and no-show reporting | Vertical | Analytics | |
| Commerce | Single and recurring memberships | Platform | Subscriptions | |
| Commerce | Trial, private, restricted-services and consecutive memberships | Platform + config | Subscriptions, Entitlements | |
| Commerce | Service credit packs | Platform | Entitlements | |
| Commerce | Store and retail products | Platform | Catalog, Orders | Boost |
| Commerce | Price breakdown endpoint with discounts and taxes | Platform | Pricing | |
| Commerce | VAT-aware billing | Platform | Ledger | |
| Payments | Glofox Payments on Stripe; Apple Pay, Google Pay | Integration | Payments | |
| Payments | Direct debit (GoCardless); BLIK in Poland | Integration | Payments | |
| Payments | Non-sufficient funds fee | Platform + config | Ledger | |
| Payments | Change payment method on a recurring membership | Platform | Subscriptions, Payments | |
| Messaging | Automated email preferences | Platform | Notifications | |
| Marketing | XLerate: email, SMS, push, dynamic tags, segmentation | Platform | Segments, Workflows | Add-on; included on Elite |
| Marketing | Targeted communications and staff tasks | Platform | Segments | Boost |
| Engagement | Community content in the member app | Vertical | Notifications | Boost |
| Staff | Staff setup, permissions and payroll | Platform | Staff, Ledger | Essential |
| Insights | Reporting; advanced reporting | Platform + config | Analytics | Advanced on Boost |
| Insights | AI churn predictor | Platform + config | Analytics, Domain events | |
| AI | ABC AI Agents for calls and chat | Integration | Notifications | |
| Platform | Glofox member app; standalone custom-branded app | Platform + config | Theming | Branded is an add-on or Elite |
| Platform | App localization (17 languages) | Platform | Theming | Elite |
| Platform | Pro staff app | Platform + config | Public API | |
| Platform | Web portal embeds for Wix, Duda, Webflow, Framer, GoDaddy, Shopify | Platform + config | Public API | |
| Platform | Public API with OpenAPI spec and signed CDC webhooks | Platform | Public API, Domain events | Access by email request |
| Live and on-demand | Livestream and on-demand classes | Integration | Integrations | External stream URLs |
| Integrations | ClassPass, Gympass (Wellhub) | Integration | Booking | |
| Integrations | ABC Trainerize coaching, Zapier | Integration | Integrations | |

## Sources

- [ABC Glofox API developer portal](https://gf-dev-docs.glofox.com/), [CDC webhooks](https://gf-dev-docs.glofox.com/cdc-webhooks/), [request access](https://gf-dev-docs.glofox.com/request-access/), [booking flow](https://gf-dev-docs.glofox.com/flows/book/)
- [Glofox help center](https://support.glofox.com/), [getting started](https://support.glofox.com/hc/en-us/categories/46200714763284-Getting-Started), [memberships and credits](https://support.glofox.com/hc/en-us/sections/46375645361812-Step-6-Memberships-and-Credits), [staff permissions](https://support.glofox.com/hc/en-us/articles/52585050344852), [scheduling](https://www.glofox.com/features/scheduling/)
- [Glofox: Is Glofox part of a bigger company?](https://www.glofox.com/blog/is-glofox-part-of-a-bigger-company/)
- [FitGap: ABC Glofox plans](https://us.fitgap.com/products/025414/abc-glofox)
- [Vibefam: Glofox review 2026](https://vibefam.com/glofox-review-pricing-features-pros-cons-2026/) (competitor)
