# Reference businesses

Two fictional businesses that adopt Tetsu, one for each industry in
[ADR-0002](../adr/0002-platform-and-verticals.md). They let us judge the
product as a customer would, and give every decision, milestone, and test the
same concrete cases to check against.

- [Anvil Row Strength](anvil-row-strength.md): a boutique fitness
  brand with three locations in two time zones.
- [Juniper Lane Salon and Spa](juniper-lane-salon-and-spa.md): a salon and spa
  with one location, appointments, and shared rooms.

## How we use them

- **Decisions.** Each Decision needed issue says how its options play for
  both businesses.
- **Milestones.** After the walking skeleton, each milestone is one business
  going live with its minimum sellable product, defined on its page. Every
  milestone ends with a walkthrough of both businesses, as owner, staff, and
  customer. Friction found there becomes issues.
- **Tests.** When implementation opens, the tables on these pages become data
  files, and a seed loader creates both businesses through public use cases.
  Scenario tests, cross-tenant isolation tests, benchmarks, and demo tenants
  all start from them.

The businesses grow with the milestones: add detail when a milestone needs it,
not before. Change them like code. A change that breaks a scenario is a
product decision, not a test fix.

## Ground rules

- Keep them fictional. Base details on the
  [Market Scan](../research/market-scan/README.md) and, when they happen,
  owner interviews, never on a real business without its consent.
- Use the [glossary](../GLOSSARY.md) terms. Industry words such as "class" or
  "stylist" belong here, not in platform docs.
- Every person, price, and policy stays consistent across both pages and any
  test built on them.

## Not covered yet

Neither business exercises these, so decisions must consider them on their
own: more than one currency, prices that include VAT, franchises, and more
than one brand in a tenant.
