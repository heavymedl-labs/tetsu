# 0065. An Arm runner for the checks job

- Status: Accepted
- Date: 2026-11-16

## Context

GitHub Free's private repositories get about $12 of Actions compute a month. A standard 2-CPU Linux runner costs
$0.006 a minute on x64 and $0.005 on Arm64, which has been available to private repositories since 2026-01-29.
CI's checks job, the largest, bills about 11 minutes on a typical run ([ADR-0023](0023-ci-on-demand.md),
[ADR-0063](0063-ci-minutes-per-run.md)).

The image is built on an x64 runner and deployed to Render
([ADR-0012](0012-first-hosting-and-deploys.md)), which runs it as built. The checks job runs the tests and the
coverage and mutation passes, none of which ship.

## Decision

- **Try the checks job on `ubuntu-24.04-arm` for one measured run,** against the x64 runs before it, and keep it
  only if it bills less money for the same checks.
- **The image job stays on x64,** so the image that is published is built where it runs.
- **If kept,** the tools that ship per architecture are checked: Betterleaks' Arm64 archive and checksum are
  already in `scripts/install-tools.sh`, and the Postgres service image is published for both.

## Alternatives considered

- **Stay on x64.** No risk, and no saving.
- **Move every job to Arm.** The image would be built on one architecture and run on another, or cross-built,
  which is slower and tests nothing the deploy runs.
- **Larger runners.** Faster, but they need a payment method, and draw from included compute only on paid plans.

## Consequences

- If it holds, the checks job costs about 17% less per minute, more if it also runs faster.
- The tests run on Arm while the deployed binary is x64. Rust's behaviour is the same on both for this code, but
  a fault in a dependency's x64 code would be caught only by the image job's smoke test, which runs the binary.
- Rust links with LLVM's `lld` by default on x64 only since 1.90, so linking on Arm may be slower, which counts
  most in mutation testing, where each mutant links again.

**Weakest evidence.** No run has measured it here yet; the decision waits on one.

**Revisit** if Arm runners' price or availability on Free changes, or if a fault shows up on one architecture only.

## Sources

- [GitHub: Actions runner pricing](https://docs.github.com/en/billing/reference/actions-runner-pricing): $0.006 a
  minute for 2-CPU Linux x64, $0.005 for Arm64, read 2026-11-16.
- [GitHub changelog: Arm64 standard runners in private repositories](https://github.blog/changelog/2026-01-29-arm64-standard-runners-are-now-available-in-private-repositories),
  2026-01-29.
- [Rust blog: lld by default on x86_64-unknown-linux-gnu](https://blog.rust-lang.org/2025/09/01/rust-lld-on-1.90.0-stable),
  2025-09-01.
