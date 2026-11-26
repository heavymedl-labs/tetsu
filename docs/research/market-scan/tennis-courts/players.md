# Players

Racquet club booking platforms, read 2026-11-23. One player so far. Prices, ownership and features change quickly, so recheck a figure before relying on it. "Not found" means the research did not confirm it, not that the product lacks it. Figures that come from reviewers or review sites are marked as reported. This is research, not a decision. Where it disagrees with an ADR, the ADR wins.

## CourtReserve

CourtReserve is club management software for tennis, pickleball and padel facilities. It prices by court count, publishes its three plan prices, and offers an API with documented webhooks. No campground product was found.

At a glance:

| Field | Value |
| --- | --- |
| Founded | 2016, by Tim and Ashley Owens (its about page); location Not found |
| Ownership | Founder-led; $54M growth investment from Mainsail Partners, announced 22 October 2025. Mainsail's ownership share: Not found |
| Focus | Tennis, pickleball and padel clubs and facilities |
| Customers | 2,400+ clubs, 7M+ players, 105M+ bookings (vendor claim) |
| Ratings | Apple App Store 2.9 of 5 from 366 ratings. Capterra: Not confirmed, see Complaints |

Pricing and business model:

- **Three plans, priced by court count:** Launch $199 a month for 8 courts, Advance $369 for 16, Momentum $549 for 32. Billed annually they fall to $159, $329 and $499 a month. Beyond 32 courts, several locations or multi-club memberships: custom pricing.
- **Add-ons:** Stringing, Access Control, DUPR and World Padel Rating are $25 a month each. Ask Pulse is a $59 a month add-on on Launch, with a 30-day trial, and is included on Advance and Momentum. The pricing page was cut off when read, so its add-on list may be longer.
- **Branded mobile app:** listed as a Momentum feature. Any setup fee or contract terms for it: Not found.
- **Payments:** Stripe (its preferred partner) or SafeSave, on the club's own merchant account. SafeSave serves the US and Canada only. A club still on an older Stripe connection pays an extra 0.2% a transaction on top of Stripe's fees until it updates the integration. Clubs that connect Stripe for the first time from 18 February 2025 do not pay it.
- **Guest fees:** a club may add a convenience fee, capped at 4% of the transaction, with lower caps in some jurisdictions.
- **Contract terms, cancellation and data export:** Not found.

Strengths:

- Wide club feature set on its home page: court reservations, public booking, lessons, events and programming, memberships, leagues and ladders, invoicing and batch billing, pro shop and point of sale, access control, a branded mobile app, reporting, Reserve with Google, and a player network.
- Named partners for door access (Brivo, RemoteLock) and rating or tournament systems (DUPR, USTA Connect).

Complaints:

- Apple App Store reviews, visible ones only: a member could not turn off club marketing notifications (December 2024), the find-players feature is weak (November 2022), and waitlist spots are not filled automatically (March 2023). Each appeared once, so this is not a ranking.
- Capterra scored 4.7 of 5 from 53 reviews, mostly from 2018 and 2021, with complaints of a learning curve, awkward refunds and fee splitting, and no QuickBooks link (reported by an earlier research pass). Capterra refused direct reading on 2026-11-23 (HTTP 403), so this was not re-read.

API:

- **Access:** scoped API keys, limited to 60 requests a minute per key. Swagger documentation is public at api.courtreserve.com.
- **Plan gating conflicts:** the pricing page lists API access on Advance and Momentum. The help article and the API page say Advance, Momentum and Enterprise, and the help article also names a "Start" plan that is not on the pricing page. The integrations page says "Scale and Enterprise", and "Scale" is not a plan name on the pricing page. Its own pages do not settle which plans get the API.
- **Webhooks:** its API page says webhooks need the "API and Webhook Access" add-on, switched on in the club's settings. What they send and how they are signed: Not found. A customer request for Zapier or webhooks, posted about four years ago, is still marked "Waiting on Votes" on its feedback forum, with 29 votes.
- **OpenAPI spec:** Swagger documentation only, and its page does not say OpenAPI.
- **Sandbox:** Not found.

Sources, all read 2026-11-23:

1. [CourtReserve home](https://courtreserve.com/)
2. [About](https://courtreserve.com/about/)
3. [Pricing](https://courtreserve.com/pricing/)
4. [Mainsail announcement](https://courtreserve.com/scaling-courtreserve-with-mainsail-partners/)
5. [Integrations](https://courtreserve.com/integrations/)
6. [Convenience fees](https://intercom.help/courtreserve/en/articles/7172928-convenience-fees)
7. [API access setup](https://intercom.help/courtreserve/en/articles/10414722-set-up-api-access-integration)
8. [Payment partners](https://intercom.help/courtreserve/en/articles/5772805-integrated-payment-partners-in-courtreserve)
9. [Update Stripe integration](https://intercom.help/courtreserve/en/articles/11124180-update-stripe-integration)
10. [API docs](https://api.courtreserve.com/apihelp/index)
11. [Feedback forum: webhooks request](https://feedback.courtreserve.com/communities/1/topics/808-allow-for-integrations-with-mailchimp-zapier-or-webhooks)
12. [Apple App Store listing](https://apps.apple.com/app/id1392556575)
13. [Capterra reviews](https://www.capterra.com/p/152562/CourtReserve/reviews/), not re-read (HTTP 403)
