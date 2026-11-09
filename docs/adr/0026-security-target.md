# 0026. Security target

- Status: Accepted
- Date: 2026-11-08
- Revision proposed in [ADR-0029](0029-security-target-revised.md)

## Context

[ADR-0004](0004-security.md) asks for a target OWASP ASVS level. ASVS 5.0 has three levels, and leaves
the choice to each organization by risk: Level 1 is the minimum, about 20% of requirements, "most
applications should be striving to achieve" Level 2, and Level 3 is for the highest assurance. Its own
example says an early-stage startup holding little sensitive data may start at Level 1, while a bank needs
Level 3.

Tetsu will hold several businesses' customer records, staff accounts, and payment records. Card data stays
with the processor ([ADR-0004](0004-security.md)). The walking skeleton holds only fictional data, but its
sign-in ([#20](https://github.com/heavymedl-labs/tetsu/issues/20)) is the one real businesses will use.

## Decision

- **Tetsu targets ASVS 5.0 Level 2.**
- **Tier 1 areas meet Level 2 as they are built.** These are identity and access, tenant isolation, and
  money, as [ADR-0005](0005-testing-and-performance.md) defines them. So sign-in in #20 is built to Level 2
  from its first version, not raised later.
- **Everything else meets Level 1 during the walking skeleton,** while only fictional data exists.
- **The whole application meets Level 2 before the first real business's data arrives** in milestone 2. A
  review against each applicable Level 2 requirement, with any requirement that does not apply and why, is
  recorded in `docs/security/asvs.md` and gates that milestone.
- **Each Tier 1 change names the ASVS requirements it meets** in its pull request or commit, and the fresh
  review checks them.

## Alternatives considered

- **Level 1 throughout.** ASVS's own startup example, and the least work. But sign-in and tenant isolation
  built to Level 1 would need rework before real data, and Tetsu holds many businesses' data, not one.
- **Level 2 for everything from the start.** Safest, but slows the walking skeleton on areas, such as the
  demo page, that hold nothing sensitive yet.
- **Level 3.** Mostly defense in depth that is hard to build. Card data stays with the processor, so the
  bank-like case does not apply.
- **No ASVS level, only the OWASP Top 10.** The Top 10 is an awareness list, not a set of requirements to
  verify against.

## Consequences

- Sign-in and tenant isolation take longer, since each needs its Level 2 requirements met and named.
- Milestone 2 gains a review before go live, tracked in
  [#83](https://github.com/heavymedl-labs/tetsu/issues/83).
- A requirement may turn out not to fit, such as one for a feature Tetsu does not have. The review records
  it as not applicable rather than skipping it.

**Weakest evidence.** The split between Tier 1 and the rest is our judgment, not an ASVS rule. How much
Level 2 adds to #20 is not yet measured.

**Revisit** when a business asks for a stated security level or audit, before handling card data directly,
which ADR-0004 rules out today, or when ASVS releases a new major version.

## Sources

- [Security checks research](../research/security-checks.md#security-target), 2026-11-08.
- [OWASP ASVS 5.0: what is the ASVS](https://github.com/OWASP/ASVS/blob/master/5.0/en/0x03-What-is-the-ASVS.md):
  the three levels and how to choose one.
- [OWASP Top 10](https://owasp.org/www-project-top-ten/): an awareness document, not a standard to verify
  against.
