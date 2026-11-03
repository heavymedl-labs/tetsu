# Mariana Tek

Mariana Tek is the premium specialist: built only for class-based boutique studios, strongest on the branded client experience and multi-location brands, and sold entirely by quote. Its main weakness is visible in its own help center, where an acquired marketing product still runs as a separate system.

## Takeaways for us

- The closest match to our fitness vertical; its brand-then-studio settings split validates our Tenancy design.
- Xplor Growth runs as a second customer database. One Party module for all customer data avoids that seam.
- Its gated API is an opening for self-serve keys.

## At a glance

| Field | Value |
| --- | --- |
| Founded | About 2014, Washington, DC |
| Ownership | Xplor Technologies (merged with Clubessential in March 2026); zingfit was folded into it |
| Focus | Pilates, Lagree, yoga, cycling, HIIT, barre, group fitness; single studios to franchises |
| Customers | "Thousands" of studios (vendor claim); over 70% migrated from other software |
| Ratings | Capterra 4.5 (62 reviews); Software Advice 4.3 (97 reviews) |

## Pricing and business model

- **Three bundles, all quote-only:** Core, Core + Growth, and Full Growth Bundle. All include unlimited instructors and classes.
- **Payments:** Stripe only, with Stripe's processing fees on top.
- **Reported spend:** around $300 to $500 a month for growing studios, per one third-party tracker. Unconfirmed.
- **Services:** paid marketing services, plus Xplor Capital financing and Xplor Insurance.

## Strengths and complaints

### Strengths

- Deepest boutique-specific workflows: spot booking, standby, penalties, add-ons.
- Branded app included rather than sold as an add-on.
- Real franchise tooling (royalty collection, HQ content syncing).

### Complaints

- Quote-only pricing.
- Some reviewers report slow help with complex settings.
- Xplor Growth has its own contacts, users, settings and billing, separate from studio customer management. Two customer databases is a seam studios have to manage.

## API

- Seven APIs (Admin, Customer, Studio, Stripe, Documents, Pricing, Advertising) plus webhooks; API-key or OAuth 2.0 auth; a sandbox.
- Production access needs a partner survey, signed terms, a 1 to 3 week review and activation per studio.

## Feature map

Of 37 feature areas from the help center, 25 are platform or platform with config, 6 are fitness-specific, 5 are integrations, and 1 has no home in our plan yet.

| Area | Feature | Tag | Platform capability | Notes |
| --- | --- | --- | --- | --- |
| Identity | Staff logins, SSO, role-based permissions per location | Platform | Identity, Authorization | Roles scoped to tenant and location |
| Identity | Customer accounts shared across web and apps | Platform | Identity | One account, every device |
| Org structure | Brand vs. studio settings; HQ and child accounts | Platform | Tenancy | Two levels: brand, then location |
| Org structure | Franchise fee portal (royalties) | Platform + config | Ledger, Tenancy | Revenue share between parent and child tenants |
| Customers | Profiles, tags, notes, visit and purchase history | Platform | Party | Vertical adds fields such as injuries |
| Customers | Waivers, forms, purchase agreements | Platform | Documents | Versioned text plus signature record |
| Scheduling | Recurring classes, rooms, instructor assignment | Platform + config | Scheduling | Recurrence and resources are generic |
| Scheduling | Room layouts and pick-a-spot | Vertical | Scheduling, Booking | Spot maps are a studio concept |
| Booking | Reservations, capacity, booking cutoffs | Platform + config | Booking | Capacity plus policy windows |
| Booking | Waitlist and standby with auto-promotion | Platform + config | Booking, Notifications | Generic queue; promotion rules are policy |
| Booking | Appointments and private sessions | Platform + config | Scheduling, Booking | Shared availability engine |
| Booking | Bring-a-guest | Vertical | Booking, Party | Lead capture via a guest reservation |
| Attendance | Check-in (front desk, self, geo) and rosters | Platform + config | Attendance | Geo check-in is an adapter |
| Attendance | Late-cancel and no-show penalty fees | Vertical | Booking, Ledger | Fitness policy on generic billing |
| Commerce | Retail products, rentals, inventory, barcodes | Platform | Catalog, Inventory | |
| Commerce | Point of sale, quick sale at check-in | Platform | Orders | Stripe terminal |
| Commerce | Add-ons pre-ordered with a reservation | Vertical | Orders, Booking | Composes Orders and Booking |
| Commerce | Memberships: sell, renew, freeze, cancel, autopay | Platform | Subscriptions | |
| Commerce | Credit packages | Platform | Entitlements | Prepaid units with expiry |
| Commerce | Intro offers | Vertical | Catalog, Entitlements | An eligibility rule |
| Commerce | Discounts, promo codes, bundles | Platform | Pricing | |
| Commerce | Gift cards | Platform | Ledger | Stored value |
| Commerce | Refunds and returns | Platform | Ledger, Orders | |
| Payments | Card processing, terminal, wallets | Integration | Payments | Stripe only |
| Payments | Payouts, reconciliation, chargebacks | Integration | Payments, Ledger | |
| Messaging | Transactional email and SMS | Platform | Notifications | SMS via Twilio registration |
| Marketing | Segments, automations, campaigns, landing pages (Xplor Growth) | Platform | Segments, Workflows | Runs as a separate system |
| Engagement | Milestones, gamification, loyalty, referrals, NPS | Vertical | Domain events, Segments | |
| Staff | Employees, shifts, pay rates | Platform | Staff | |
| Staff | Instructor pay; payroll with direct deposit | Platform + config | Staff, Ledger | |
| Insights | Sales, finance, utilization, marketing reports | Platform + config | Analytics | Largest help collection |
| Platform | Branded client app, Biz staff app, buy pages | Platform + config | Theming, Public API | |
| Platform | Seven APIs, webhooks, data export | Platform | Public API | Gated partner review |
| Integrations | Livestream and on-demand partners | Integration | Integrations | Forte, Tribe, Intelivideo |
| Integrations | ClassPass, Gympass | Integration | Booking | |
| Integrations | Ad attribution (Google, Meta) | Integration | Analytics | Xplor Ads |
| Migration | Importers from Mindbody, zingfit, ClubReady | (new) | Import | |

## Sources

- [Mariana Tek Knowledge Base](https://support.marianatek.com/en/)
- [Mariana Tek: brand vs. studio settings](https://support.marianatek.com/en/articles/5640165-which-key-settings-in-mariana-tek-are-configured-at-the-brand-vs-studio-level)
- [Mariana Tek: group fitness software](https://www.marianatek.com/group-fitness-studio-software/)
- [Pulse Signal: Mariana Tek plans](https://getpulsesignal.com/compare/classpass-vs-marianatek)
- [Supergood: Mariana Tek API report card](https://supergood.ai/api-report-card/mariana-tek) (sells an unofficial API)
- [Capterra: Mariana Tek](https://capterra.com/p/175281/Mariana-Tek/)
