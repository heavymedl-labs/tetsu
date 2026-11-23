# Players

Racquet club booking platforms, read 2026-11-23. One player so far. Prices, ownership and features change quickly, so recheck a figure before relying on it. "Not found" means the research did not confirm it, not that the product lacks it. This is research, not a decision. Where it disagrees with an ADR, the ADR wins.

## CourtReserve

CourtReserve is club management software for tennis, pickleball and padel facilities. It prices by court count, publishes its three plan prices, and has a documented API. It looks strong for racquet clubs and has no campground product, so it is a reference for court and facility booking rather than a direct competitor for a campground vertical.

Takeaways for us:

- Its published per-court tiers and public API docs make a detailed comparison possible, which few competitors in the fitness scan allow.
- It lets the club's own merchant account take the money (Stripe or SafeSave), which fits our plan for a choice of payment processor.
- No webhooks in its public docs, and a customer request for them sits unbuilt on its feedback forum. A webhook-first API is an opening.

At a glance:

| Field | Value |
| --- | --- |
| Founded | Not found |
| Ownership | Founders Tim and Ashley Owens; $54M growth investment from Mainsail Partners, announced 22 October 2025. Mainsail's ownership share: Not found |
| Focus | Tennis, pickleball and padel clubs and facilities |
| Customers | 2,400+ clubs and 7M+ players (vendor claim) |
| Ratings | Capterra 4.7 of 5 from 53 reviews, most from 2018 and 2021, so stale. Apple App Store 2.9 of 5 from 302 ratings (read 2026-11-23) |

Pricing and business model:

- **Three plans, priced by court count:** Launch $199 a month for 8 courts, Advance $369 for 16, Momentum $549 for 32. Billed annually they fall to $159, $329 and $499 a month. Beyond 32 courts, several locations or multi-club memberships: custom pricing.
- **Add-ons:** Stringing, Access Control, DUPR and World Padel Rating are $25 a month each. Ask Pulse is $59 a month on Launch and included on Advance and Momentum. The full add-on list was not read, because the pricing page was cut off when fetched.
- **Branded mobile app:** listed as a Momentum feature. A separate $499 setup fee and a one-year contract were reported by an earlier research pass but not confirmed on the pricing page. Recheck.
- **Payments:** Stripe (preferred) or SafeSave, on the club's own merchant account. Older Stripe connections carry an extra 0.2% CourtReserve fee until the club updates the integration.
- **Guest fees:** a club may add a convenience fee, capped at 4% of the transaction, with lower caps in some jurisdictions.
- **Contract terms, cancellation and data export:** Not found.

Strengths:

- Wide club feature set: court and public booking, memberships, batch billing, lessons, events, leagues, point of sale, access control, a branded mobile app and Reserve with Google.
- Named partners for door access (Brivo, RemoteLock) and rating or tournament systems (DUPR, USTA Connect).
- Reviewers on Capterra praise responsive support, deep admin tools and easy member booking (reported).

Complaints:

- Capterra reviewers report a learning curve, awkward refunds and fee splitting, and no QuickBooks link (reported, mostly 2018 and 2021).
- The iOS app rates 2.9 of 5. The complaint text was not read.

API:

- **Access:** scoped API keys, limited to 60 calls a minute per key, with public docs at api.courtreserve.com.
- **Plan gating conflicts:** the pricing page lists API access on Advance and Momentum. The help article adds Enterprise, and also names a "Start" plan that is not on the pricing page. The integrations page says "Scale and Enterprise", and "Scale" is not a plan name on the pricing page. Which plans get the API is not settled by its own pages.
- **Webhooks:** Not found. A request for Zapier or webhooks is open on its feedback forum.
- **OpenAPI spec and sandbox:** Not found.

Sources, all read 2026-11-23:

1. [CourtReserve home](https://courtreserve.com/)
2. [Pricing](https://courtreserve.com/pricing/)
3. [Mainsail announcement](https://courtreserve.com/scaling-courtreserve-with-mainsail-partners/)
4. [Integrations](https://courtreserve.com/integrations/)
5. [Convenience fees](https://intercom.help/courtreserve/en/articles/7172928-convenience-fees)
6. [API access setup](https://intercom.help/courtreserve/en/articles/10414722-set-up-api-access-integration)
7. [Payment partners](https://intercom.help/courtreserve/en/articles/5772805-integrated-payment-partners-in-courtreserve)
8. [Update Stripe integration](https://intercom.help/courtreserve/en/articles/11124180-update-stripe-integration)
9. [API docs](https://api.courtreserve.com/apihelp/index)
10. [Feedback forum: webhooks request](https://feedback.courtreserve.com/communities/1/topics/808-allow-for-integrations-with-mailchimp-zapier-or-webhooks)
11. [Capterra reviews](https://www.capterra.com/p/152562/CourtReserve/reviews/)
12. [Apple App Store listing](https://apps.apple.com/app/id1392556575)
