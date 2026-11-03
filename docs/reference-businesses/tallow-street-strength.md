# Tallow Street Strength

A fictional boutique fitness brand, and Tetsu's
[reference business](README.md) for fitness. Rowan Ellis owns it.

## Profile

| Location | Time zone | Rooms | Opened |
| --- | --- | --- | --- |
| Denver, LoDo | America/Denver | Strength room (20), Ride room (24 bikes) | 2019 |
| Denver, Highlands | America/Denver | Strength room (16) | 2021 |
| Scottsdale | America/Phoenix (no daylight saving) | Strength room (20), Ride room (24 bikes) | 2024 |

- **People:** 14 instructors who teach at any location, 6 front-desk staff,
  and a manager per location. Rowan and an operations lead see everything.
- **Customers:** about 1,800 active, 1,100 of them on a membership.
- **Hours:** classes from 5:30 a.m. to 8:00 p.m. The 6:00 a.m. and 5:30 p.m.
  classes fill first.

## What it sells

| Class | Length | Capacity | Notes |
| --- | --- | --- | --- |
| Strength | 50 min | Room size | Barbells and racks |
| Ride | 45 min | 24 | Customers pick a bike |
| Mobility | 30 min | 15 | Taught in the Strength room |

| Offer | Price (USD) | Rules |
| --- | --- | --- |
| Drop-in | $32 Denver, $28 Scottsdale | One class at the location it was bought for |
| 10-class pack | $280 | Any location; expires 6 months after purchase |
| 8 classes a month | $149/month | Unused classes do not roll over |
| Unlimited | $189/month | Any location; month to month |
| Unlimited, annual | $1,990 prepaid | Prorated refund if the customer moves away |
| Founding rate | $159/month | Unlimited for members who joined in 2019; kept after price rises |
| Intro offer | $49 | Two weeks unlimited, new customers only, once |
| Towel, water, shake | $3 to $9 | Front-desk retail with stock counts |

## Policies

- Booking opens 7 days ahead, at the class's local time.
- Cancelling within 12 hours of class is late. Pack and monthly customers lose
  the credit; unlimited members pay $15. A no-show costs $20 for everyone.
- When a class is full, customers join the waitlist. A spot that opens more
  than 2 hours before class goes to the first person automatically; after
  that, everyone waiting gets a text and the first to claim it gets it.
- Members can freeze for up to 2 months a year at $10 a month.
- Customers under 18 need a guardian to sign the waiver, and the guardian
  pays.
- Instructors earn $45 a class plus $2 for each customer over 10. Payroll runs
  outside Tetsu.

## A typical week

- **Rowan, the owner,** compares revenue and attendance across locations on
  Monday, moves an underfilled Mobility class, and raises Scottsdale's
  drop-in price for next month.
- **The front desk** checks customers in at 6:00 a.m., sells a water, and
  fixes a membership whose renewal payment failed overnight.
- **An instructor** checks the roster before class, marks two no-shows, and
  asks for a substitute next Thursday.
- **A customer** books bike 12 in Ride from their phone, joins the waitlist
  for a full Strength class, gets promoted, then cancels 10 hours before.

## Customers to test with

| Customer | Situation |
| --- | --- |
| Maya Chen | Unlimited member at Highlands; books in Scottsdale when visiting family |
| Theo Grant | 10-class pack bought 5 months ago, 3 classes left |
| Priya Nair | Unlimited member, frozen for November with an injury |
| Sam Ortiz | Card expired, so this month's renewal failed |
| Jordan Reyes and Ari Reyes | Jordan pays for 15-year-old Ari and signed Ari's waiver |
| Lee Park | Also a Fernhollow client, with the same email address |
| Casey Hart | Founding rate since 2019 |
| Morgan Blake | Cancelled in October and asked for their data to be deleted |

## Edge cases it exercises

- **Time zones.** A recurring 6:00 a.m. class stays at 6:00 a.m. local time in
  both cities. Because Scottsdale skips daylight saving, the hour between
  Denver and Scottsdale changes twice a year.
- **Location pricing.** The same drop-in costs less in Scottsdale.
- **Shared credits.** Packs and memberships work at every location, but a
  drop-in does not.
- **Grandfathered prices** survive a price increase.
- **Refund partway through a term** on an annual membership.
- **Guardians** who pay for and consent for a minor.

## Moving to Tetsu

Rowan is leaving an incumbent over processor lock-in and a mid-contract price
increase. The move must bring 1,800 profiles, active memberships with their
next billing dates, pack balances, stored cards (moved between processors as
tokens, never as card numbers), signed waivers, and three years of visit
history.

## What success looks like

- A customer books a class in a few taps, and a busy 6:00 a.m. check-in never
  waits on the software.
- Month-end revenue matches the processor's payouts to the cent.
- Rowan sees all three locations in one place.

Rowan would leave over surprise fees, an outage during the morning rush, or
being unable to take the data out.

## Minimum sellable product

What Tallow Street needs before it switches. Going live with it is
[milestone 2](https://github.com/HeavyMedl/tetsu/milestones).

**Needed to go live:**

- Import from the incumbent, as described in [Moving to Tetsu](#moving-to-tetsu).
- Recurring classes at three locations in two time zones, with rooms,
  instructors, and substitutes.
- Booking from a phone browser, bike selection, the waitlist rules, and
  late-cancel and no-show fees.
- Front-desk check-in.
- Every offer in [What it sells](#what-it-sells) except retail stock counts,
  with card payments, failed-payment retries, and freezes.
- Waivers, including a guardian signing for a minor.
- Booking, waitlist, and payment messages by email and text.
- Revenue that reconciles with processor payouts, attendance by location, and
  an instructor pay report.
- Staff roles, data export and deletion, support access, and a Tetsu bill
  Rowan understands.

**Can wait:** stock counts, native apps, self-serve sign-up (we provision
Tallow Street ourselves), partner API access, payroll integration, marketing
tools, and analytics beyond the reports above.
