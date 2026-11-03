# Arketa

Arketa is the venture-backed independent expanding from booking into a full "operating system": payroll with tax filing, accounting, on-demand video and events, all in one bundle. It wins switchers from Mindbody and Mariana Tek mostly on support and UX, and its API is open but minimal.

## Takeaways for us

- All-in-one now reaches payroll tax filing and bookkeeping. Our Ledger covers most accounting reports; payroll tax filing would be a regulated integration. Decide deliberately whether to follow.
- Its service-type model, with pricing options explicitly linked to what they can book, is a template for Catalog and Entitlements.
- Events and trainings belong in the platform's Scheduling and Catalog modules, not as vertical bolt-ons.
- A sandbox tenant would put us ahead of every challenger.

## At a glance

| Field | Value |
| --- | --- |
| Founded | 2020 (as Sutra) by Rachel Lea Fishman and Josh Archer; New York |
| Funding | About $23M total: $7.6M seed (2022, First Round) and $15M Series A (Jun 2025, Inspired Capital); Y Combinator backed |
| Scale | 3,000+ studios; nearly $500M in transactions processed; about 55 to 110 staff |
| Customers | Yoga, Pilates, gyms, recovery; 92% women-led businesses (vendor claim) |
| Ratings | Capterra 4.3 (111 reviews), with ease, value and support each 4.3 |

## Pricing and business model

- **Quote-based,** with no published tiers. The vendor promises no mid-contract increases and no features moved between tiers.
- **Bundled:** texting, subbing, reviews and video are included rather than metered.
- **Processing:** flat 2.9% + $0.30 on Stripe.
- **No long-term contract;** migration of clients, packages and stored cards is included.

## Strengths and complaints

### Strengths

- Support is the most-cited reason for switching to it, including from Mariana Tek.
- The widest scope in the group: payroll, tax filing and accounting are built in.
- Fast feature cadence and a clean UI.

### Complaints

- Bugs in sign-ups, payments, time zones and membership changes during busy release periods.
- A steep setup curve; onboarding doesn't cover everything.
- Payroll reporting is thinner than dedicated tools.
- Offshore support time zones frustrate some US studios.

## API

- Public API with self-serve keys (Settings, Integrations); docs on GitHub Pages; 25 requests per second.
- No sandbox (every call hits live data) and no dedicated API support.

## Feature map

Of 44 feature areas, 31 are platform or platform with config, 6 are fitness-specific, 4 are integrations, and 3 have no home in our plan yet. Arketa's pricing model is the cleanest in the group: every offering is a service type, and pricing options are explicitly linked to them.

| Area | Feature | Tag | Platform capability | Notes |
| --- | --- | --- | --- | --- |
| Identity | Team permissions | Platform | Authorization |  |
| Org structure | Multi-location brands | Platform | Tenancy |  |
| Customers | Client profiles and notes | Platform | Party |  |
| Customers | Liability waivers | Platform | Documents |  |
| Customers | Loyalty milestones (e.g. an email on the 100th class) | Vertical | Domain events |  |
| Scheduling | Classes built from service-type templates; recurring series | Platform + config | Scheduling, Catalog | Template sets duration, capacity, location |
| Scheduling | Events: workshops, retreats, pop-ups; drop-in by default | Platform | Scheduling, Catalog |  |
| Scheduling | Appointments against staff availability | Platform + config | Scheduling |  |
| Scheduling | Teacher trainings and courses | Platform | Scheduling, Catalog |  |
| Scheduling | Livestream classes via Zoom | Integration | Integrations |  |
| Booking | Waitlists | Platform + config | Booking |  |
| Booking | Mat or spot selection | Vertical | Booking |  |
| Booking | Cancellation policy | Platform + config | Booking |  |
| Booking | Booking embedded on the studio's own site | Platform + config | Public API |  |
| Attendance | Check-in screen | Platform + config | Attendance |  |
| Attendance | No-show fees | Vertical | Booking, Ledger |  |
| Commerce | Drop-in price per service type | Platform | Pricing, Catalog |  |
| Commerce | Class packages (credits) | Platform | Entitlements |  |
| Commerce | Memberships, unlimited or credit-based | Platform | Subscriptions, Entitlements |  |
| Commerce | Intro offers (a flag on any pricing option) | Vertical | Catalog |  |
| Commerce | Bundles across service types | Platform | Pricing |  |
| Commerce | Invoices for private or non-standard services | Platform | Orders, Ledger |  |
| Commerce | Pricing options linked to the service types they can book | Platform | Entitlements | Explicit eligibility |
| Commerce | Direct purchase links | Platform + config | Orders |  |
| Commerce | POS, retail, inventory | Platform | Orders, Inventory |  |
| Commerce | Discounts and promo codes | Platform | Pricing |  |
| Commerce | Rewards program | Vertical | Ledger |  |
| Payments | Stripe processing | Integration | Payments | 2.9% + $0.30 |
| On-demand | Video library with categories, collections, rentals | (new) | Media, Catalog |  |
| Messaging | In-app texting and phone calls with clients | Platform | Notifications |  |
| Marketing | Email and SMS broadcasts, automations, workflows | Platform | Segments, Workflows |  |
| Marketing | Lead management, conversion tracking, campaign revenue attribution | Platform | Party, Analytics |  |
| Marketing | Review collection | Platform + config | Notifications |  |
| Staff | Instructors, permissions, time tracking | Platform | Staff |  |
| Staff | Substitute management | Vertical | Staff, Scheduling |  |
| Staff | Pay rules: per class, per head, revenue share, hourly, salary, commission, tips | Platform + config | Staff, Ledger |  |
| Staff | Direct deposit and payroll tax filing (W-2, 1099) | Integration | Staff |  |
| Finance | Arketa Accounting: bank feeds, categorization, P&L, balance sheet, cash flow | (new) | Ledger | Optional bookkeeper |
| Insights | Reporting and peer benchmarking | Platform + config | Analytics |  |
| AI | Assistant for plain-English data questions; churn prediction | Platform + config | Analytics |  |
| Platform | Branded client app; business app | Platform + config | Theming, Public API |  |
| Platform | Public API with self-serve keys | Platform | Public API | No sandbox |
| Platform | MCP server for the help center | (new) | None | AI assistants can query their docs |
| Integrations | ClassPass, Mailchimp, Klaviyo, HubSpot, Flodesk, Google Calendar, Squarespace, Zapier | Integration | Integrations |  |

## Sources

- [Arketa help center](https://help.arketa.com/getting-started/welcome): [schedule](https://help.arketa.com/schedule/overview), [pricing](https://help.arketa.com/pricing/overview), [public API](https://help.arketa.com/settings/integrations/api)
- [Arketa: Series A announcement](https://arketa.com/blog/series-a-fundraise)
- [Arketa vs. Momence](https://www.arketa.com/compare/arketa-vs-momence) (vendor comparison)
- [Capterra: Arketa](https://www.capterra.com/p/212718/Arketa/)
- [Yespress: Arketa profile](https://yespress.io/arketa)
