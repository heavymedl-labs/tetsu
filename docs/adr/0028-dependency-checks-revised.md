# 0028. Dependency checks, revised

- Status: Proposed
- Date: 2026-11-08
- Supersedes [ADR-0025](0025-dependency-checks.md)

## Context

[ADR-0025](0025-dependency-checks.md) chose cargo-deny. A fresh review the same day, with each finding
checked against the workspace ([research](../research/security-checks.md#review-on-2026-11-08)), found
it could never pass as written:

- webpki-roots, which Postgres TLS needs, is `CDLA-Permissive-2.0`, a permissive license missing from
  the list.
- Our crates are already `publish = false`. cargo-deny skips them only with `private.ignore`.
- Some advisories cannot be cleared. The newest actix-http still requires the h2 0.3 line, which has an
  unfixed advisory. basic-toml, fxhash, and pin-utils are unmaintained with no successor. basic-toml
  comes through Askama, our chosen HTML engine ([ADR-0007](0007-server-stack.md)), so it will reach a
  crate we ship.
- cargo-deny ignore entries never expire, which would hide [ADR-0011](0011-server-stack-guardrails.md)'s
  trigger for an advisory unpatched for 30 days.
- Dependabot security updates open pull requests, and each runs CI, the cost ADR-0025 gave for turning
  off version updates.

With the corrected config below, licenses, bans, and sources pass on the workspace today. Only the
benchmark harness's advisories remain ([#80](https://github.com/heavymedl-labs/tetsu/issues/80)).

## Decision

- **cargo-deny checks the whole workspace** in `scripts/check.sh`, and so in the pre-push hook and CI. It
  is pinned in `scripts/install-tools.sh` and listed in `check.sh`'s missing-tool check.
- **`deny.toml` sets every rule explicitly,** since several of cargo-deny's defaults only warn:
  - **Targets:** Linux and macOS, x86-64 and ARM, so Windows-only crates add no noise.
  - **Advisories:** a vulnerability, an unsound or unmaintained crate, or a yanked version fails.
  - **Licenses:** MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unicode-3.0, and
    CDLA-Permissive-2.0 pass. `private.ignore` skips our own unpublished crates.
  - **Sources:** crates.io only. Unknown registries and git sources fail.
  - **Bans:** wildcard versions fail, except path dependencies. Duplicate versions warn.
- **An advisory we cannot clear gets an ignore entry** in `deny.toml`. Each entry gives a reason, the issue
  that tracks it, and a review date no more than 30 days out. `check.sh` fails once a review date has
  passed, which is ADR-0011's trigger: the issue then decides to replace the crate or set a new date.
- **The floor guard protects `deny.toml`,** so a new ignore entry, license, or source needs a
  `Floor-reason:` trailer recording the maintainer's approval.
- **Dependabot alerts are on, and its automatic pull requests are off.** Alerts warn about a
  vulnerability published between pushes, at no CI cost. The fix is made by hand, and checked by the next
  push. `.github/dependabot.yml` keeps only GitHub Actions.
- **This lands in [#82](https://github.com/heavymedl-labs/tetsu/issues/82),** after
  [#80](https://github.com/heavymedl-labs/tetsu/issues/80) updates actix and files ignore entries for what
  remains.

## Alternatives considered

- **ADR-0025 as written.** Failed on a shipped crate's license and could not pass on the harness.
- **`unmaintained = "workspace"`.** Would quietly pass basic-toml, which arrives through Askama rather
  than directly.
- **Leaving the harness out of the check.** Its unmaintained crates are the same kind the app will meet,
  so ignore entries handle both the same way.
- **Dependabot security updates.** Each pull request costs a CI run ([ADR-0023](0023-ci-on-demand.md)),
  including for harness-only advisories.
- **cargo-audit, cargo-vet, Dependabot alone.** As in ADR-0025.

## Consequences

- A push fails when a new advisory lands on any crate we use, even with no change of ours. It can
  interrupt unrelated work.
- cargo-deny fetches the advisory database each run, so the pre-push hook needs the network. A push
  while offline fails, and waits until the network returns.
- The same commit can pass now and fail tomorrow, as the advisory database grows. A failing CI run on an
  old commit may mean a new advisory, not a regression.
- Dependabot alerts and cargo-deny ignore entries are separate lists, so dismissing one does not dismiss
  the other. GitHub's alerts cover vulnerabilities; cargo-deny is what reports unmaintained crates.
- Replacing it means swapping one command in `check.sh` and the config file.

**Weakest evidence.** Two trial runs on one day. How often a new advisory will block a push is not
known.

**Revisit** if advisories block pushes often enough to slow work, if ignore entries pile up, or when a
second contributor makes reviewed dependencies, through cargo-vet, worth the effort.

## Sources

- [Security checks research](../research/security-checks.md), 2026-11-08, including the review.
- [cargo-deny book](https://embarkstudios.github.io/cargo-deny/): advisory, license (`private.ignore`),
  bans, and sources settings.
- [RustSec advisory database](https://rustsec.org/).
- [GitHub Docs: about Dependabot alerts](https://docs.github.com/en/code-security/dependabot/dependabot-alerts/about-dependabot-alerts):
  available for organization-owned and user-owned repositories.
