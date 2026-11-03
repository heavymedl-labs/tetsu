# Walla

Walla is the smallest, most focused player: a premium, published-price platform for boutique studios that sells retention intelligence ("WallaPredict") and polish rather than breadth. It has a public API and webhooks, but a small review base that is mostly vendor-referred.

## Takeaways for us

- A premium price works when reliability and support are visibly better, which is what our engineering standards aim at.
- Its "no unpaid reservations" rule is worth enforcing as an invariant in our Booking types.
- Retention prediction fits our Analytics read models over domain events; no separate system needed.
- A single responsive web app is a credible v1, and a data point for PWA vs. native.

## At a glance

| Field | Value |
| --- | --- |
| Founded | 2020, San Diego, by Laura Munkholm (CEO) and Doug Hecht |
| Funding | About $18M: seed, $8M Series A (2022), and $5M (Oct 2025, Social Leverage and Ankona Capital) |
| Markets | United States and Canada |
| Focus | Pilates, Lagree, yoga, boutique fitness; single to multi-location |
| Ratings | Capterra 4.9 (25 reviews), but most were invited by the vendor with an incentive |

## Pricing and business model

| Plan | Per location, monthly | Notes |
| --- | --- | --- |
| Core | From $320 | Unlimited users |
| Pro | From $599 | Adds the full marketing suite |

- **The highest published entry price in the group,** framed as replacing several separate tools.
- **No free trial or free version** listed.

## Strengths and complaints

### Strengths

- Reviews consistently praise ease of use for staff and clients, and reliability compared with Mindbody and WellnessLiving.
- Responsive, founder-visible support (monthly webinars, frequent releases).
- Key integrations such as ClassPass are built in, with Zapier for the rest.

### Complaints

- Price is high for small studios.
- Reporting gaps (flexible date ranges, intro-offer models).
- Inventory and detailed accounting were thin or missing per older reviews.
- The review base is small and mostly vendor-referred, so ratings say less than they appear to.

## API

- Public REST API for members, classes, bookings and payments, plus webhooks for booking, payment, cancellation and membership events (e.g. `booking.confirmed`).
- Access, auth and rate limits aren't publicly documented; confirm in a demo.
- One responsive web app across devices rather than separate native apps, per a reviewer.

## Feature map

Of 30 feature areas, 23 are platform or platform with config, 4 are fitness-specific, and 3 are integrations. Walla's help center sits behind a login, so this map is thinner than the others and leans on its blog and public support articles.

| Area | Feature | Tag | Platform capability | Notes |
| --- | --- | --- | --- | --- |
| Identity | Staff roles; a protected "sensitive info" tab; edit rights on others' notes | Platform | Authorization | |
| Org structure | Multi-location management | Platform | Tenancy | |
| Customers | Client profiles, details shown on rosters, unique emails enforced | Platform | Party | |
| Customers | Digital waivers with roster alerts | Platform | Documents | |
| Customers | Client milestones celebrated | Vertical | Domain events | |
| Scheduling | Classes, appointments, private sessions, events | Platform + config | Scheduling | |
| Booking | Waitlists | Platform + config | Booking | |
| Booking | Book-a-Spot | Vertical | Booking | Core plan |
| Booking | Buddy and guest passes | Vertical | Booking, Entitlements | |
| Booking | No unpaid reservations; comp passes instead | Platform + config | Booking, Entitlements | A strict invariant |
| Attendance | Self check-in | Platform + config | Attendance | |
| Attendance | Automated late-cancel and no-show fees | Vertical | Booking, Ledger | |
| Commerce | Plans and packages, recurring plans, start on first visit | Platform | Subscriptions, Entitlements | |
| Commerce | Discounts and promotions | Platform | Pricing | |
| Commerce | POS, retail, rentals; retail report | Platform | Orders, Catalog | Inventory was thin per older reviews |
| Payments | Integrated processing with merchant support | Integration | Payments | |
| Messaging | Automatic confirmations, reminders, payment alerts | Platform | Notifications | |
| Messaging | Two-way texting | Platform | Notifications | Pro |
| Marketing | Marketing suite, custom automations, unlimited segments | Platform | Segments, Workflows | Pro |
| Sales | Lead tracker and sales tools; Google and Meta lead forms | Platform | Party, Segments | |
| Reputation | Ratings and reviews | Platform + config | Notifications | |
| Staff | Pay rates by class, shift and headcount bonus; commissions; time clock; pay stubs | Platform + config | Staff, Ledger | |
| Staff | Payroll exports for external providers (no processing) | Platform | Staff | |
| Insights | Reporting and AI dashboards | Platform + config | Analytics | |
| Insights | WallaPredict: performance against goals, drop-off prediction | Platform + config | Analytics, Domain events | |
| Platform | Branded client app and web experience | Platform + config | Theming | |
| Platform | Walla Websites (separate design team) | Platform + config | Theming | |
| Platform | Public API and webhooks | Platform | Public API, Domain events | |
| Video | Zoom livestreams, on-demand library, Vimeo | Integration | Integrations | |
| Integrations | ClassPass, Referrizer, Zapier | Integration | Integrations | |

## Sources

- [Walla: integrations, API and webhooks](https://www.hellowalla.com/features/integrations-2), [Pilates studio software](https://hellowalla.com/made-for/pilates), [retail report](https://www.hellowalla.com/blog/retail-report-gives-you-insight-into-inventory)
- Walla support: [payroll](https://support.hellowalla.com/en/articles/9080392-payroll), [payroll settings](https://support.hellowalla.com/payroll-settings-walla-support-center), [important things to know](https://support.hellowalla.com/en/articles/9203767-important-things-to-know-about-using-walla)
- [Capterra: Walla](https://capterra.com/p/265361/Walla/)
- [Techjockey: Walla plans](https://www.techjockey.com/detail/walla)
- [Techleap: Walla raises $5M](https://finder.techleap.nl/news/feed/walla-secures-5m-for-ai-fitness-software), [Social Leverage: Walla Series A](https://socialleverage.beehiiv.com/p/social-leverage-letter-issue-46)
