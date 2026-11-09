# 0029. Security target, revised

- Status: Proposed
- Date: 2026-11-08
- Supersedes [ADR-0026](0026-security-target.md)

## Context

[ADR-0026](0026-security-target.md) set ASVS 5.0 Level 2 as the target. Two fresh reviews the same day,
checked against the ASVS chapters ([research](../research/security-checks.md#review-on-2026-11-08)), found
gaps:

- **Level 2 requires multi-factor authentication** (`v5.0.0-6.3.3`), and ADR-0026 never said so. How
  people sign in, and whether any relaxation is justified, is open decision
  [#9](https://github.com/heavymedl-labs/tetsu/issues/9). Passkeys with user verification can meet 6.3.3
  on their own.
- **Its rules could not be checked.** "Level 2" did not say it includes Level 1. The chapter list for
  sign-in missed password hashing (V11), secrets management (V13), and data protection (V14). The fresh
  review cannot see a commit message.
- **The gate came too late.** It sat at go live, but real people can sign up on production in milestone
  1, and an import rehearsal comes before go live.

The [reference businesses](../reference-businesses/README.md) hold minors' records, guardian-signed
waivers, a membership frozen for an injury, private stylist notes, and gift card balances, besides
customer, staff, and payment records. One account can reach other people's data. A guardian's account
holds their child's waiver, for example. Card data stays with the processor
([ADR-0004](0004-security.md)).

## Decision

- **Tetsu targets ASVS 5.0.0 Level 2:** every L1 and L2 requirement in the chapters that apply.
  Requirements are cited as `v5.0.0-<id>`, such as `v5.0.0-8.4.1`, since IDs can change between versions.
- **Multi-factor authentication is required for every account,** as `v5.0.0-6.3.3` says. That covers
  staff, owners, customers, guardians, and Tetsu's own support and operator accounts.
  [#9](https://github.com/heavymedl-labs/tetsu/issues/9) chooses the method, such as passkeys, and
  recovery under 6.4.3 and 6.4.4. It may relax 6.3.3 for an account type only with the documented
  rationale and mitigating controls ASVS asks for, recorded in its ADR.
- **Each Tier 1 area ([ADR-0005](0005-testing-and-performance.md)) meets these chapters at Level 2 as it
  is built:**

  | Area | Chapters |
  | --- | --- |
  | Identity and access | V6 Authentication, V7 Session Management, V8 Authorization, V3 Web Frontend Security, V11 Cryptography, V13 Configuration, V14 Data Protection, V16 Security Logging and Error Handling |
  | Tenant isolation | V8 Authorization, V1 Encoding and Sanitization, V13 Configuration, V14 Data Protection, V16 Security Logging and Error Handling |
  | Money | V2 Validation and Business Logic, V8 Authorization, V16 Security Logging and Error Handling |

- **Every change meets at least Level 1 of each chapter its code touches,** and Level 2 once the gate has
  passed. A Tier 1 change also meets Level 2 of its area's chapters.
- **The issue names the requirements a change must meet,** before work starts. The
  [fresh review](../review.md), which sees the issue and the diff, checks each one against the diff. It
  also looks for requirements in those chapters that the issue missed. The commit repeats them in an
  `ASVS:` footer.
- **The gate: no real person's data enters any environment before the Level 2 review passes**
  ([#83](https://github.com/heavymedl-labs/tetsu/issues/83)). "Any environment" means production, a
  preview, and the maintainer's or an agent's machine. "Real person's data" means an import or a
  rehearsal, an owner's export file, or a sign-up by anyone other than the maintainer. Until then,
  production holds only fictional data and the maintainer's own accounts.
- **The review records each requirement in `docs/security/asvs.md`,** with the test or check that shows
  Tetsu meets it.
  - **Not applicable** covers only a feature Tetsu does not have, such as WebRTC.
  - **An accepted risk** is a requirement Tetsu should meet but does not yet. It needs the maintainer's
    approval, an issue, and a date to fix it by. Tier 1 requirements cannot be accepted risks.
  - The maintainer declares the review passed.
- **After the gate,** a change that touches an applicable requirement, or makes one applicable, updates
  `asvs.md` in the same commit.

## Alternatives considered

- **ADR-0026 as written.** Hid the multi-factor requirement, and its rules could not be checked.
- **Relaxing multi-factor for customers here.** Decided before #9 weighs passkeys, and justified by a claim
  that one account exposes one person, which guardians disprove.
- **Level 1 throughout.** ASVS's own example for a startup with little sensitive data. Tetsu holds many
  businesses' data, including minors' records.
- **Level 3.** Hardware-based authentication for every user, and defense in depth beyond what the data
  needs. Revisit if a vertical stores clinical health records.
- **No ASVS level, only the OWASP Top 10.** The Top 10 is an awareness list, not requirements to verify
  against.

## Consequences

- Sign-in ([#20](https://github.com/heavymedl-labs/tetsu/issues/20)) waits on #9 choosing a multi-factor
  method. Breached-password checks (6.2.12) and notifications need services that each need an ADR under
  [ADR-0010](0010-evaluating-technology.md).
- Milestone 1's sign-in is used only by the maintainer until the gate passes.
- Each issue for Tier 1 work lists its ASVS requirements, and `docs/review.md` gains an ASVS step when this
  is accepted.
- Moving to a new minor ASVS version, such as 5.1, means re-checking `asvs.md`. A patch version does not.

**Weakest evidence.** The chapter list per area is our mapping, not an ASVS one. The injury freeze is
health data in a small way, and privacy law beyond ASVS is not covered here.

**Revisit** when a business asks for a stated security level or audit, before handling card data
directly, if a vertical stores clinical health records, or when ASVS releases a new minor version.

## Sources

- [Security checks research](../research/security-checks.md#security-target), 2026-11-08, including the
  reviews.
- OWASP ASVS 5.0.0: [what is the ASVS](https://github.com/OWASP/ASVS/blob/master/5.0/en/0x03-What-is-the-ASVS.md),
  for levels, versions, and ID format; [chapters](https://github.com/OWASP/ASVS/tree/master/5.0/en)
  V1, V2, V3, V6, V7, V8, V11, V13, V14, and V16, including 6.3.3, 6.4.3, 6.4.4, 8.2.2, 8.4.1, 11.4.2,
  13.3.1, and 14.1.1.
- [OWASP Top 10](https://owasp.org/www-project-top-ten/): "a standard awareness document".
