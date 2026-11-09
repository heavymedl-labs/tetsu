# 0029. Security target, revised

- Status: Proposed
- Date: 2026-11-08
- Supersedes [ADR-0026](0026-security-target.md)

## Context

[ADR-0026](0026-security-target.md) set ASVS 5.0 Level 2 as the target. A fresh review the same day,
checked against the ASVS chapters, found gaps
([research](../research/security-checks.md#review-on-2026-11-08)):

- **Level 2 requires multi-factor authentication.** ASVS 6.3.3 says so, and ADR-0026 never mentioned it.
  ASVS allows relaxing 6.3.3 only with "a fully documented rationale and a comprehensive set of
  mitigating controls".
- **The target needs a precise meaning.** Meeting Level 2 means every L1 and L2 requirement, not only
  those tagged L2.
- **ASVS is organized by chapter, not by our areas,** so "Tier 1 meets Level 2" needs a list of chapters
  to check.
- **The gate came after real data could arrive.** It sat at go live, but an import rehearsal happens
  earlier.
- **Nothing kept the level after the gate.**

The data is also more sensitive than ADR-0026 said. The [reference businesses](../reference-businesses/README.md)
hold minors' records, guardian-signed waivers, a membership frozen for an injury, private stylist notes,
and gift card balances, besides customer, staff, and payment records. Card data stays with the processor
([ADR-0004](0004-security.md)).

## Decision

- **Tetsu targets ASVS 5.0.0 Level 2:** every L1 and L2 requirement in the chapters that apply.
  Requirements are cited as `v5.0.0-<id>`, such as `v5.0.0-8.4.1`, since IDs can change between
  versions.
- **Staff and owners must use multi-factor authentication.** They can see every customer's data, so
  6.3.3 applies to them unrelaxed.
- **Customers are offered multi-factor authentication but not forced to use it.** This is a documented
  relaxation of 6.3.3: forcing it would keep gym and salon customers from signing in, and one customer's
  account exposes only that customer's data.
- **Customer sign-in mitigates the relaxation with:**
  - 6.3.1: controls against brute force and credential stuffing.
  - 6.2.12: checking passwords against breached sets.
  - 6.3.5 and 6.3.7: notifying customers of suspicious sign-ins and of changed credentials. These are
    Level 3, taken on to offset the relaxation.
- **Each Tier 1 area ([ADR-0005](0005-testing-and-performance.md)) meets these chapters at Level 2 as it
  is built:**

  | Area | Chapters |
  | --- | --- |
  | Identity and access | V6 Authentication, V7 Session Management, V8 Authorization, V3 Web Frontend Security (cookies), V16 Security Logging |
  | Tenant isolation | V8 Authorization (including 8.2.2 and 8.4.1), V16 Security Logging |
  | Money | V2 Validation and Business Logic, V8 Authorization, V16 Security Logging |

- **Every other change meets Level 1 of the chapters it touches** until the gate, and Level 2 after.
- **Each change names the requirements it meets** in its commit, and the [fresh review](../review.md)
  checks them, including any requirement in those chapters that the change should have named.
- **The gate: no real business's data enters any environment, including an import rehearsal, before the
  Level 2 review passes** ([#83](https://github.com/heavymedl-labs/tetsu/issues/83)). That review records
  each applicable requirement in `docs/security/asvs.md`, and how Tetsu meets it.
- **"Not applicable" covers only a feature Tetsu does not have,** such as WebRTC or OAuth as a provider.
  A requirement Tetsu should meet but does not is an accepted risk. It goes in a separate list in
  `asvs.md`, which the maintainer approves.
- **After the gate,** a change that touches an applicable requirement updates `asvs.md` in the same
  commit.

## Alternatives considered

- **ADR-0026 as written.** Hid the multi-factor requirement, and its rules could not be checked.
- **Forced multi-factor for customers.** Meets 6.3.3 in full, but would turn away the customers the
  reference businesses rely on.
- **Level 1 throughout.** ASVS's own example for a startup with little sensitive data. Tetsu holds many
  businesses' data, including minors' records.
- **Level 3.** Hardware-based authentication for every user, and defense in depth beyond what the data
  needs. Revisit if a vertical stores clinical health records.
- **No ASVS level, only the OWASP Top 10.** The Top 10 is an awareness list, not requirements to verify
  against.

## Consequences

- Sign-in ([#20](https://github.com/heavymedl-labs/tetsu/issues/20)) grows. It needs multi-factor
  authentication for staff, optional for customers, and breach checks and notifications. Multi-factor
  needs a vetted crate, such as for TOTP or WebAuthn, which needs its own ADR under ADR-0004.
- Customer notifications need a channel, such as email, before #20 is done.
- Tenant isolation and money take longer, since each names its requirements.
- `docs/review.md` gains an ASVS step when this is accepted.
- Moving to a new minor ASVS version, such as 5.1, means re-checking `asvs.md`. A patch version does
  not.

**Weakest evidence.** The relaxation for customers rests on our judgment of adoption, not measurement.
The chapter list per area is ours too, not an ASVS mapping.

**Revisit** when a business asks for a stated security level or audit, before handling card data
directly, if a vertical stores clinical health records, or when ASVS releases a new minor version.

## Sources

- [Security checks research](../research/security-checks.md#security-target), 2026-11-08.
- OWASP ASVS 5.0.0: [what is the ASVS](https://github.com/OWASP/ASVS/blob/master/5.0/en/0x03-What-is-the-ASVS.md),
  for levels, versions, and ID format;
  [V6 Authentication](https://github.com/OWASP/ASVS/blob/master/5.0/en/0x15-V6-Authentication.md),
  6.2.12, 6.3.1, 6.3.3, 6.3.5, and 6.3.7;
  [V8 Authorization](https://github.com/OWASP/ASVS/blob/master/5.0/en/0x17-V8-Authorization.md),
  8.2.2 and 8.4.1.
- [OWASP Top 10](https://owasp.org/www-project-top-ten/): "a standard awareness document".
