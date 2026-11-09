# 0029. Security target, revised

- Status: Proposed
- Date: 2026-11-08
- Supersedes [ADR-0026](0026-security-target.md)

## Context

[ADR-0026](0026-security-target.md) set ASVS 5.0 Level 2 as the target. Three fresh reviews the same day,
checked against the ASVS chapters ([research](../research/security-checks.md#review-on-2026-11-08)), found
gaps:

- **Level 2 requires multi-factor authentication** (`v5.0.0-6.3.3`), and ADR-0026 never said so. How
  people sign in, and whether any relaxation is justified, is open decision
  [#9](https://github.com/heavymedl-labs/tetsu/issues/9). A passkey that verifies the user may meet
  6.3.3 on its own, which #9 can weigh.
- **Its rules could not be checked.** "Level 2" did not say it includes Level 1. Any fixed list of
  chapters per area missed some that apply, such as password hashing in V11 or mass assignment in V15. The
  fresh review cannot see a commit message.
- **Its review came too late.** It sat at go live, but real people can sign up on production in milestone
  1, and an import rehearsal comes before go live.

The [reference businesses](../reference-businesses/README.md) hold minors' records, guardian-signed
waivers, a membership frozen for an injury, private stylist notes, and gift card balances, besides
customer, staff, and payment records. One account can reach other people's data. A guardian's account
holds their child's waiver, for example. Card data stays with the processor
([ADR-0004](0004-security.md)).

## Decision

- **Tetsu targets ASVS 5.0.0 Level 2:** every L1 and L2 requirement in the chapters that apply.
  Requirements are cited as `v5.0.0-<id>`, such as `v5.0.0-8.4.1`, since IDs can change between versions.
- **Multi-factor authentication is required for every account,** as `v5.0.0-6.3.3` says: staff, owners,
  customers, guardians, and any account that can reach more than one tenant, if
  [#24](https://github.com/heavymedl-labs/tetsu/issues/24) creates one.
  [#9](https://github.com/heavymedl-labs/tetsu/issues/9) chooses the method, and recovery under
  `v5.0.0-6.4.3` and `v5.0.0-6.4.4`. It may relax 6.3.3 for an account type only with the documented
  rationale and mitigating controls ASVS asks for, recorded in its ADR.
- **Tier 1 code ([ADR-0005](0005-testing-and-performance.md)) meets Level 2 of every chapter that applies to
  it, as it is built.** Every other change meets at least Level 1 of each chapter covering the code it
  changes, and Level 2 once the Level 2 review below has passed.
- **The issue names the chapters and requirements a change must meet,** before work starts. The
  [fresh review](../review.md), which sees the issue and the diff, checks each one against the diff. It
  also checks for chapters and requirements that apply but the issue left out. The commit repeats them in
  an `ASVS:` footer.
- **No real person's data enters any environment before the Level 2 review passes**
  ([#83](https://github.com/heavymedl-labs/tetsu/issues/83)):
  - **A real person's data** is personal data about anyone but the maintainer, in any form. That includes
    an import or rehearsal, an owner's export, a sign-up or an invite, and notes from an owner interview
    that name their customers.
  - **An environment** is anywhere Tetsu's work keeps data. That includes production, previews, Neon
    branches and backups, logs, CI, issues, and the maintainer's or an agent's machine.
  - **Production enforces it.** Sign-ups and invites are turned off until the review passes, and a test
    checks that they are.
- **The Level 2 review records each requirement in `docs/security/asvs.md`** with one outcome:
  - **Met,** with the test or check that shows it, or for a requirement met by a document or a setting,
    such as a policy or a Render TLS setting, where to read it.
  - **Relaxed,** by the ADR that documents ASVS's rationale and mitigating controls, such as #9's for 6.3.3.
  - **Not applicable,** only for a feature Tetsu does not have, such as WebRTC.
  - **Accepted risk,** a requirement Tetsu should meet but does not yet, with the maintainer's approval, an
    issue, and a fix-by date. A requirement for Tier 1 code cannot be an accepted risk.

  The maintainer declares the review passed.
- **The review depends on decisions still open:**
  - security logging off the host, for `v5.0.0-16.4.3`, in [#27](https://github.com/heavymedl-labs/tetsu/issues/27)
  - data retention, for `v5.0.0-14.1.2`, in [#22](https://github.com/heavymedl-labs/tetsu/issues/22)
  - which privacy laws apply to the data, for `v5.0.0-14.1.1`, recorded in `asvs.md`
  - whether ADR-0012 and ADR-0027's secret stores meet `v5.0.0-13.3.1`

  #83 lists them, and each blocks it.
- **After the review,** a change that touches an applicable requirement, or makes one applicable, updates
  `asvs.md` in the same commit. `check.sh` fails once an accepted risk's fix-by date has passed.

## Alternatives considered

- **ADR-0026 as written.** Hid the multi-factor requirement, and its rules could not be checked.
- **A fixed list of chapters per area.** Easier to read, but every version of the list missed chapters
  that apply.
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
  method. Breached-password checks (`v5.0.0-6.2.12`) need a service or data set, which needs an ADR under
  [ADR-0010](0010-evaluating-technology.md).
- Milestone 1's sign-in is used only by the maintainer until the review passes.
- Each Tier 1 issue lists its ASVS chapters and requirements, and `docs/review.md` gains an ASVS step when
  this is accepted.
- The review cannot pass until #22 and #27 are decided.
- Moving to a new minor ASVS version, such as 5.1, means re-checking `asvs.md`. A patch version does not.

**Weakest evidence.** Which chapters apply to a change is a judgment the issue author and the fresh review
make, not an ASVS mapping. Privacy law is identified only during the review.

**Revisit** when a business asks for a stated security level or audit, before handling card data
directly, if a vertical stores clinical health records, or when ASVS releases a new minor version.

## Sources

- [Security checks research](../research/security-checks.md#security-target), 2026-11-08, including the
  reviews.
- OWASP ASVS 5.0.0: [what is the ASVS](https://github.com/OWASP/ASVS/blob/master/5.0/en/0x03-What-is-the-ASVS.md),
  for levels, versions, and ID format; [chapters](https://github.com/OWASP/ASVS/tree/master/5.0/en)
  V6, V8, V11, V13, V14, V15, and V16, including 6.2.12, 6.3.3, 6.4.3, 6.4.4, 8.2.2, 8.4.1, 11.4.2,
  13.3.1, 14.1.1, 14.1.2, and 16.4.3.
- [OWASP Top 10](https://owasp.org/www-project-top-ten/): "a standard awareness document".
