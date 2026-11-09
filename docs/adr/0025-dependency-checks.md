# 0025. Dependency checks

- Status: Accepted
- Date: 2026-11-08
- Revision proposed in [ADR-0028](0028-dependency-checks-revised.md)

## Context

[ADR-0004](0004-security.md) asks for dependency vulnerability and license checks, and
[ADR-0011](0011-server-stack-guardrails.md) says advisory scanning needs its own ADR. Its revisit triggers,
such as an advisory left unpatched for 30 days, need something that sees advisories. Today Dependabot
watches only GitHub Actions, and nothing checks crates.

A trial run on 2026-11-08 found five advisories and three unmaintained crates, all in the benchmark
harness's dependencies ([research](../research/security-checks.md#dependency-checks),
[#80](https://github.com/heavymedl-labs/tetsu/issues/80)).

## Decision

- **cargo-deny checks the whole workspace** in `scripts/check.sh`, and so in CI, pinned in
  `scripts/install-tools.sh`. It checks four things, configured in `deny.toml`:
  - **Advisories.** A vulnerability, an unmaintained crate, or a yanked version fails the check.
  - **Licenses.** Only permissive licenses pass: MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, and
    Unicode-3.0. Our own crates are marked `publish = false`, so the check skips them.
  - **Sources.** Crates come from crates.io only.
  - **Bans.** Wildcard versions fail, except path dependencies. Duplicate versions warn.
- **An advisory with no fix yet gets an ignore entry** in `deny.toml`, with a reason and the issue that
  tracks it. Adding one loosens the floor, so the maintainer approves it.
- **The benchmark harness is checked too.** It runs on CI runners and the maintainer's machine, and the
  check needs no special case.
- **Dependabot alerts and security updates are turned on for Cargo,** so an advisory published between
  pushes still raises an alert. Routine version updates stay off, since each pull request runs CI.
- **This lands in [#82](https://github.com/heavymedl-labs/tetsu/issues/82), after
  [#80](https://github.com/heavymedl-labs/tetsu/issues/80) clears the harness's advisories,** so the check
  starts green.

## Alternatives considered

- **cargo-audit.** From the RustSec team, but advisories only, so licenses and sources need another tool.
- **cargo-vet or cargo-crev.** Record human reviews of each dependency. Worth it with more people. Too much
  for one maintainer now.
- **Dependabot alone.** Free and needs no install, but runs only on GitHub, misses licenses and sources,
  and does not stop a push.
- **Dependabot version updates for Cargo.** Keeps crates current, but a weekly pull request costs CI minutes
  ([ADR-0023](0023-ci-on-demand.md)) for updates nothing asked for.

## Consequences

- A push fails when a new advisory lands on any crate we use, even with no change of ours. That is the
  point, but it can interrupt unrelated work.
- The check needs the advisory database, so it needs the network, as `cargo` already does.
- A new dependency with a license outside the list fails until the list changes, which needs this ADR's
  reasoning applied again.
- Replacing it means swapping one command in `check.sh` and the config file.

**Weakest evidence.** One trial run. How often a new advisory will block a push is not known.

**Revisit** if advisories block pushes often enough to slow work, or when a second contributor makes
reviewed dependencies, through cargo-vet, worth the effort.

## Sources

- [Security checks research](../research/security-checks.md), 2026-11-08.
- [cargo-deny book](https://embarkstudios.github.io/cargo-deny/): advisories, licenses, bans, and sources
  checks.
- [RustSec advisory database](https://rustsec.org/).
- [GitHub Docs: about Dependabot alerts](https://docs.github.com/en/code-security/dependabot/dependabot-alerts/about-dependabot-alerts).
