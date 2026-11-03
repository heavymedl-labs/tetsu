# WellnessLiving

WellnessLiving is the founder-led generalist: the widest vertical range (fitness through salon and spa) at published prices, with a large OpenAPI-documented API. Its weak spots are a locked-in payment processor, a poorly rated client app, and billing practices that draw formal complaints.

## Takeaways for us

- One spec for internal and partner clients is the right model; we can generate SDKs in several languages from ours.
- Processor lock-in is a top complaint, so a Payments port with more than one adapter is a selling point.
- Regional hosts point to a later need: a tenant registry that routes tenants to regional databases.

## At a glance

| Field | Value |
| --- | --- |
| Founded | 2009, Richmond Hill (Toronto area), Ontario, Canada |
| Ownership | Founder-led; US$66M minority growth round in Aug 2022 (McCarthy Capital equity, CIBC debt) |
| Staff | About 385 (LinkedIn, Apr 2026) |
| Verticals | Yoga, Pilates, fitness, martial arts, dance, salon, spa, wellness |
| Data centers | United States and Australia |
| Ratings | Capterra 4.4 (611 reviews); G2 about 4.5; BBB 1.1 (26 reviews, 24 complaints in 3 years) |

## Pricing and business model

| Tier | Monthly | Annual, per month | Staff |
| --- | --- | --- | --- |
| Starter | $69 | $62 | 1 |
| Business | $199 | $179 | Unlimited |
| BusinessPro | $349 | $314 | Unlimited |
| Enterprise | Quote | Quote | Unlimited |

- **Processing is locked to Paragon or Nuvei;** Stripe isn't offered. Reviewers report PCI and annual processor fees on top.
- **Contracts** auto-renew annually with a 30-day notice window.
- **Free data migration** on all plans.

## Strengths and complaints

### Strengths

- Broadest vertical coverage, which suits multi-format wellness businesses.
- Published prices and a low entry tier.
- Onboarding and marketing automation are consistently praised.
- Founder-led, without a private-equity parent.

### Complaints

- Processor lock-in, with fees reviewers say double or triple real cost.
- The client app rates about 2 on Google Play and 3 on the App Store.
- Reports of add-ons enrolled without consent and hard cancellations.
- In-product AI upsell pop-ups.
- Weaker fit for one-to-one appointment businesses.

## API

- An official OpenAPI spec (about 324 paths) that its own Achieve and Elevate apps also use; a generated PHP SDK plus JavaScript; US and Australia hosts.
- Access is gated by a questionnaire, NDA, API agreement and account-executive approval.

## Feature map

Of 40 feature areas, 25 are platform or platform with config, 3 are fitness-specific, 10 are integrations, and 2 have no home in our plan yet. WellnessLiving leans on partners more than anyone else here.

| Area | Feature | Tag | Platform capability | Notes |
| --- | --- | --- | --- | --- |
| Identity | Staff profiles, logins, role permissions (e.g. who can refund or edit memberships) | Platform | Identity, Authorization | |
| Identity | Client accounts; option to disable self check-in | Platform | Identity | |
| Org structure | Enterprise Cloud: parent and HQ accounts, cross-location visits, multi-location reporting | Platform | Tenancy, Entitlements, Analytics | Enterprise |
| Customers | Client profiles, history, document attachments | Platform | Party | |
| Customers | Online waivers; contracts with control over re-signing | Platform | Documents | |
| Customers | Custom forms and surveys at registration, booking, purchase | Platform | Documents | |
| Scheduling | Online booking for appointments, classes, events | Platform + config | Scheduling, Booking | |
| Scheduling | Events and workshops managed end to end | Platform | Scheduling, Catalog | |
| Scheduling | Book-a-Spot: equipment with its own schedule | Vertical | Scheduling, Booking | Modelled as bookable assets |
| Scheduling | Appointment types; option to hide duration | Platform + config | Scheduling | |
| Booking | Waitlists for classes and events | Platform + config | Booking | |
| Booking | Reserve with Google | Integration | Booking | No extra fee |
| Booking | ClassPass, Wellhub, FitReserve | Integration | Booking | |
| Attendance | Self check-in; attendance web app | Platform + config | Attendance | |
| Attendance | Brivo door access | Integration | Attendance | |
| Commerce | Purchase options: passes, memberships, contracts | Platform | Subscriptions, Entitlements | |
| Commerce | Discount codes across services and store | Platform | Pricing | |
| Commerce | POS, online store, shopping cart | Platform | Orders, Catalog | |
| Payments | Integrated processing; mobile card reader | Integration | Payments | Paragon or Nuvei only |
| Payments | Debt collection service | Integration | Ledger | Aldous & Associates |
| Finance | QuickBooks sync | Integration | Ledger | |
| Messaging | Message Center for two-way SMS | Platform | Notifications | |
| Marketing | Marketing Suite: email and SMS automations | Platform | Segments, Workflows | Business+; AI features are an add-on |
| Marketing | Direct-mail postcards | Integration | Notifications | |
| Marketing | Customer reviews shared online | Platform + config | Party | |
| Loyalty | Rewards points redeemable for prizes | Vertical | Domain events, Ledger | Business+ |
| Staff | Staff management and payroll | Platform | Staff, Ledger | |
| Insights | Advanced reporting | Platform + config | Analytics | |
| AI | Isaac business intelligence and churn predictor | Platform + config | Analytics, Domain events | |
| AI | CAASI 24/7 AI front-desk sales agent | Integration | Notifications | |
| Fitness | FitBUILDER workout builder | Vertical | None | Programming content |
| Fitness | Spivi heart-rate leaderboards; AccuroFit tracking | Integration | Integrations | |
| Virtual | Go Virtual: live and on-demand without third parties | (new) | Media | |
| Platform | Achieve client app; white-label version | Platform + config | Theming | White-label on BusinessPro |
| Platform | Elevate staff app | Platform + config | Public API | |
| Platform | Presence website builder | Platform + config | Theming | Extra fee |
| Platform | Website widgets | Platform + config | Public API | |
| Platform | API with OpenAPI spec and SDKs | Platform | Public API | Gated |
| Integrations | Zapier | Integration | Public API | |
| Verticals | Salon, spa, med spa, music, tattoo, swim (HIPAA-verified) | (new) | None | Same platform, many verticals |

## Sources

- [WellnessLiving feature menu and June 2018 release notes](https://www.wellnessliving.com/blog/june-2018-release-notes/), [August 2018 release notes](https://www.wellnessliving.com/blog/august-2018-release-notes)
- [WellnessLiving PHP SDK (OpenAPI)](https://github.com/wellnessliving/wl-openapi-php)
- [StackScored: WellnessLiving pricing](https://www.stackscored.com/pricing/fitness-studio-management/wellnessliving/)
- [API Evangelist: WellnessLiving OpenAPI subset](https://raw.githubusercontent.com/api-evangelist/wellnessliving/refs/heads/main/openapi/wellnessliving-payments-api-openapi.yml)
- [Vibefam: WellnessLiving review 2026](https://vibefam.com/wellnessliving-review-pricing-features-pros-cons-2026/) (competitor)
- [Supergood: WellnessLiving API report card](https://supergood.ai/api-report-card/wellnessliving) (sells an unofficial API)
