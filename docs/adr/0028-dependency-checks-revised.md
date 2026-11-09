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
- Ignore entries never expire, and accept a bare advisory ID with no reason. That would hide
  [ADR-0011](0011-server-stack-guardrails.md)'s triggers.
- The floor guard only flags removed lines in a protected file, so a new ignore entry would pass.
  cargo-deny also reads `deny.exceptions.toml` files that no one would think to guard.
- Dependabot security updates open pull requests, and each runs CI.

A trial in a clone cleared most of the harness's advisories:

- Turning off actix-web's default features drops h2 and its advisory, since the harness serves only
  HTTP/1.
- Turning off Askama's `config` feature drops the unmaintained basic-toml, and we use no `askama.toml`.
- Updating actix past the Rust version that the vendored `cornucopia` crate declares (1.85) clears the
  other four advisories. That declared version also explains the downgrades seen earlier, and a plain
  `cargo update` would bring them back.

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
- **A fix for an advisory sets a minimum version** in `Cargo.toml`, such as `actix-web = "4.18"`, so a
  later `cargo update` cannot resolve back to a vulnerable one.
- **An advisory we cannot clear gets an ignore entry,** a table whose `reason` ends with
  `Tracked in #<issue>. Review by <YYYY-MM-DD>.` `check.sh` fails on any entry without that ending,
  including a bare advisory ID:
  - The issue is a **Decision needed** issue, as ADR-0011 asks: replace the crate, or keep it with a stated
    reason.
  - The date is at most 30 days after the commit that added or renewed the entry, which `check.sh`
    reads from git.
  - `check.sh` fails once a date has passed in UTC.
  - When the issue decides to keep the crate, the entry stays and is still renewed monthly, so each year
    holds twelve reviews of it. The script that reads the entries has tests.
- **The floor guard protects `deny.toml` for any changed line,** added or removed, and flags creating any
  other file cargo-deny reads: `.deny.toml`, `.cargo/deny.toml`, and the `deny.exceptions.toml` forms. So
  every ignore entry, renewal, license, or source change needs a `Floor-reason:` trailer recording the
  maintainer's approval.
- **Advisories need the network; the rest do not.** `check.sh` fetches the advisory database each run. If
  the fetch fails, it reruns with `--offline`, which accepts a cached copy under 7 days old
  (`maximum-db-staleness`) and fails otherwise. Licenses, bans, and sources run offline.
- **cargo-deny covers one of ADR-0011's triggers, not all.** It reports advisories, including RustSec's
  notices that a crate is unmaintained. "No release in 18 months", a maintainer stepping away, and a
  license change before the lockfile moves stay duties, checked when a dependency is added or updated.
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
- Each ignore entry needs renewing at least monthly, with approval, for as long as it stays.
- Yanked status comes from cargo's local index cache, which CI restores with its cache. A crate yanked
  after the cache was saved may pass in CI until `Cargo.lock` changes.
- The floor guard accepts one `Floor-reason:` trailer for every change to a file in a push, so approval is
  per push, not per entry. Commits that change `deny.toml` say what each change is.
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
