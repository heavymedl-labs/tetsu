# Fernhollow Salon and Spa

A fictional salon and spa, and Tetsu's [reference business](README.md) for
salons and spas. Avery Okafor owns it.

## Profile

- **Location:** one, in Denver (America/Denver), open Tuesday to Sunday.
- **People:** 6 stylists at three levels (junior, senior, master), 3 massage
  and facial therapists, 1 nail technician, and 2 front-desk staff. Avery
  also takes clients as a master stylist.
- **Resources:** 6 styling chairs, 2 treatment rooms, 1 couples room, and 2
  pedicure chairs. Rooms need 15 minutes to reset between clients.
- **Clients:** about 2,400, most of them booking every 4 to 8 weeks.

## What it sells

| Service | Length | Needs | Price (USD) |
| --- | --- | --- | --- |
| Haircut | 60 min | Stylist, chair | $65 junior, $85 senior, $110 master |
| Color | 120 min | Stylist, chair | From $140 by level; stylist is free during the 45-minute processing time |
| Massage | 60 or 90 min | Therapist, treatment room | $110 or $155 |
| Couples massage | 60 min | Two therapists, couples room | $230 |
| Facial | 60 min | Therapist, treatment room | $120 |
| Pedicure | 45 min | Nail technician, pedicure chair | $55 |

| Offer | Price (USD) | Rules |
| --- | --- | --- |
| 5-massage package | $500 | Prepaid 60-minute massages; no expiry |
| Monthly Massage membership | $95/month | One 60-minute massage a month, up to 3 roll over; 10% off retail |
| Gift cards | Any amount | Spent across visits until the balance is gone |
| Retail | $12 to $60 | Hair and skin products with stock counts; taxed, unlike services |

## Policies

- Color and couples bookings need a 50% deposit.
- Cancelling within 24 hours forfeits the deposit. A no-show pays in full, and
  after two no-shows every booking needs a deposit.
- Clients tip on the card after the service. A tip for two staff, such as a
  couples massage, is split between them.
- Stylists earn 45% commission on services and 10% on retail. Payroll runs
  outside Tetsu.
- Stylists keep private notes on each client, such as color formulas, that
  clients never see.

## A typical week

- **Avery, the owner,** sets next month's staff shifts and time off, checks
  that chairs and rooms stay busy, and reviews commissions before payday.
- **The front desk** rebooks clients at checkout, sells a shampoo, and takes
  tips on the card terminal.
- **A stylist** sees the day's clients and fits a junior's haircut into a
  color client's processing time.
- **A client** books a massage online, choosing a therapist or "anyone", and
  pays a deposit for a color appointment.

## Clients to test with

| Client | Situation |
| --- | --- |
| Lee Park | Also a Tallow Street Strength customer, with the same email address |
| Grace Liu | Books color with a senior stylist who is leaving the salon |
| Hana Sato and Jules Moreau | Couples massage; Hana pays for both |
| Omar Haddad | Monthly Massage member with 3 rolled-over massages, cancelling |
| Beth Collins | Received a $150 gift card and has $40 left |
| A walk-in | No account; pays cash for a haircut |
| Riley Brooks | Two no-shows, so every booking now needs a deposit |

## Edge cases it exercises

- **Several resources per booking:** a person and a room, or two people and a
  room, all free at once.
- **Gaps inside an appointment,** when a stylist can serve someone else.
- **Price by staff level** for the same service.
- **Deposits, forfeits, tips, and commissions** on one ledger.
- **Unused credits** when a membership ends.
- **Guest checkout** and cash, with no customer account.
- **Staff-only notes** on a client.

## Moving to Tetsu

Avery runs on a basic booking app, a spreadsheet of client notes, and paper
gift cards. The move must bring 2,400 clients with their notes and formulas,
future appointments, package balances, and outstanding gift card balances.

## What success looks like

- No double-booked chairs, rooms, or staff.
- Clients book online without calling, and most rebook before they leave.
- Commissions and tips are ready for payroll without a spreadsheet.

Avery would leave over double bookings, deposit disputes the software cannot
explain, or losing client notes.
