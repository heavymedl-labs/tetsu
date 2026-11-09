# 0028. Dependency checks, revised

- Status: Proposed
- Date: 2026-11-08
- Supersedes [ADR-0025](0025-dependency-checks.md)

## Context

[ADR-0025](0025-dependency-checks.md) chose cargo-deny. Two fresh reviews the same day, with each finding
checked against the workspace ([research](../research/security-checks.md#review-on-2026-11-08)), found
it could never pass as written:

- webpki-roots, which Postgres TLS needs, is `CDLA-Permissive-2.0`, a permissive license missing from
  the list.
- Our crates are already `publish = false`. cargo-deny skips them only with `private.ignore`.
- Ignore entries accept only `id` and `reason`, and never expire. That would hide
  [ADR-0011](0011-server-stack-guardrails.md)'s triggers.
- The floor guard only flags removed lines in a protected file, so a new ignore entry would pass.
- Dependabot security updates open pull requests, and each runs CI.

A trial in a clone cleared most of the harness's advisories:

- Turning off actix-web's default features drops h2 and its advisory, since the harness serves only
  HTTP/1.
- Turning off Askama's `config` feature drops the unmaintained basic-toml, and we use no `askama.toml`.
- Updating actix past the Rust version that the vendored `cornucopia` crate declares (1.85) clears the
  other four advisories. That declared version also explains the downgrades seen earlier.

Only fxhash and pin-utils remain. Both are unmaintained and come through monoio, in the harness only.

## Decision

- **cargo-deny checks the whole workspace** in `scripts/check.sh`, and so in the pre-push hook and CI. It
  is pinned in `scripts/install-tools.sh`. `check.sh` checks the installed version matches the pin, as
  well as that the tool exists.
- **`deny.toml` sets every rule explicitly,** since several of cargo-deny's defaults only warn:
  - **Targets:** Linux and macOS, x86-64 and ARM, so Windows-only crates add no noise.
  - **Advisories:** a vulnerability, an unsound or unmaintained crate, or a yanked version fails.
    `check.sh` passes `-D advisory-not-detected`, so an ignore entry that no longer matches fails too.
  - **Licenses:** MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unicode-3.0, and
    CDLA-Permissive-2.0 pass. `private.ignore` skips our own unpublished crates.
  - **Sources:** crates.io only. Unknown registries and git sources fail.
  - **Bans:** wildcard versions fail, except path dependencies. Duplicate versions warn.
- **Dependencies enable only the features we use,** so optional crates with advisories stay out.
- **An advisory we cannot clear gets an ignore entry,** whose `reason` ends with
  `Tracked in #<issue>. Review by <YYYY-MM-DD>.`:
  - The issue is a **Decision needed** issue, as ADR-0011 asks: replace the crate, or keep it with a stated
    reason.
  - The date is at most 30 days after the entry is added or renewed.
  - `check.sh` reads the dates and fails once one has passed in UTC.
  - The script that reads them has tests.
- **The floor guard protects `deny.toml` for any changed line,** added or removed. So every ignore entry,
  renewal, license, or source change needs a `Floor-reason:` trailer recording the maintainer's approval.
- **Advisories need the network; the rest do not.** `check.sh` fetches the advisory database each run. If
  the fetch fails, it uses a cached copy under 7 days old (`maximum-db-staleness`), and fails otherwise.
  Licenses, bans, and sources run offline.
- **Dependabot alerts are on, and its automatic pull requests are off.** Alerts warn about a vulnerability
  published between pushes, at no CI cost, and the fix is made by hand. GitHub Actions version updates
  move from weekly to monthly, grouped into one pull request, since each pull request costs a CI run
  ([ADR-0023](0023-ci-on-demand.md)).
- **All of this lands in [#82](https://github.com/heavymedl-labs/tetsu/issues/82),** with the harness
  fixes above.

## Alternatives considered

- **ADR-0025 as written.** Failed on a shipped crate's license and could not pass on the harness.
- **Ignore entries for h2 and basic-toml.** Turning off unused features removes them.
- **`unmaintained = "workspace"`.** Would quietly pass unmaintained crates that arrive indirectly.
- **Leaving the harness out of the check.** Its crates run on CI runners and the maintainer's machine, and
  two ignore entries cover what remains.
- **Dependabot security updates.** Each pull request costs a CI run, including for harness-only advisories.
- **cargo-audit, cargo-vet, Dependabot alone.** As in ADR-0025.

## Consequences

- A push fails when a new advisory lands on any crate we use, even with no change of ours. It can
  interrupt unrelated work.
- The same commit can pass now and fail tomorrow, as the advisory database grows. A failing CI run on an
  old commit may mean a new advisory, not a regression.
- Each ignore entry needs renewing at least monthly, with approval, until its Decision needed issue closes.
- Dependabot alerts and cargo-deny ignore entries are separate lists, so dismissing one does not dismiss
  the other. cargo-deny is what reports unmaintained crates.
- Duplicate-version warnings print on every run and are left alone unless they cost build time.
- Replacing it means swapping one command in `check.sh` and the config file.

**Weakest evidence.** Trial runs on one day. How often a new advisory will block a push is not known.

**Revisit** if advisories block pushes often enough to slow work, if ignore entries pile up, or when a
second contributor makes reviewed dependencies, through cargo-vet, worth the effort.

## Sources

- [Security checks research](../research/security-checks.md), 2026-11-08, including the reviews.
- [cargo-deny book](https://embarkstudios.github.io/cargo-deny/): advisory settings
  (`maximum-db-staleness`, ignore entries), license (`private.ignore`), bans, and sources.
- [RustSec advisory database](https://rustsec.org/).
- [GitHub Docs: about Dependabot alerts](https://docs.github.com/en/code-security/dependabot/dependabot-alerts/about-dependabot-alerts):
  available for organization-owned and user-owned repositories.
