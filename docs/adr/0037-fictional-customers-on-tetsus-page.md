# 0037. Fictional customers on Tetsu's page

- Status: Accepted
- Date: 2026-11-10
- Supersedes the naming rule in [ADR-0018](0018-demo-businesses-as-data.md)

## Context

[ADR-0018](0018-demo-businesses-as-data.md) says Tetsu's repository names no business outside its docs, so that
no tenant is ever code ([#19](https://github.com/heavymedl-labs/tetsu/issues/19)). The maintainer wants Tetsu's
root page to read as an established product in the demo environment
([#97](https://github.com/heavymedl-labs/tetsu/issues/97)). Its marketing names customers, quotes them, and gives
figures and prices, all fictional. Those names are part of Tetsu's own page, not businesses that use Tetsu, but
they do sit in the binary's templates. A review in November 2026 found that the rule as written forbids them, and
that outside a demo the page would present the fiction as fact.

## Decision

**Tetsu's root page may name fictional customers, but only in a demo environment.**

- The rule that matters stays: no template or code names a business that uses Tetsu. Tenants are data.
- The root page shows its fictional customers, quotes, figures, plans, and changelog only when `TETSU_DEMO` is
  set, under the demo note that says they are fictional. Outside a demo it shows the hero alone. A test checks
  both.
- The fictional names must not match a real business we know of, and must not repeat a demo tenant's details.

## Alternatives considered

- **Keep the rule, and load the marketing content from data.** The rule would hold to the letter, but it adds a
  content store for one page and protects nothing the narrower rule does not.
- **Show the content in every environment, with the note always on.** A real launch would then ship fictional
  claims behind a disclaimer, which is worse than writing real copy when there is something real to say.

## Consequences

The demo shows Tetsu's site as it is meant to look. The root page has two shapes, and a test covers both. Before
Tetsu launches outside a demo, real copy replaces the fictional content, and a new ADR supersedes this one.

## Sources

- [ADR-0018](0018-demo-businesses-as-data.md) and [#19](https://github.com/heavymedl-labs/tetsu/issues/19), the
  rule this narrows.
- Federal Trade Commission, [Guides Concerning the Use of Endorsements and Testimonials in
  Advertising](https://www.ecfr.gov/current/title-16/chapter-I/subchapter-B/part-255), 16 CFR Part 255, on why an
  endorsement must reflect a real endorser's honest opinion, so fictional ones must never appear as real.
