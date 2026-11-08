# 0024. Secrets

- Status: Proposed
- Date: 2026-11-08

## Context

[ADR-0012](0012-first-hosting-and-deploys.md) puts each secret in a store: GitHub Actions holds the deploy
key and migration URL, and Render holds the app's database URL and a registry read token. The floor in
[CONSTRAINTS.md](../../CONSTRAINTS.md) bans secrets in source, but only the fresh review looks for them.
The first deploy ([#77](https://github.com/heavymedl-labs/tetsu/issues/77)) creates the first real
credentials.

GitHub's own secret scanning is paid on private repositories. The common free tool, Gitleaks, now gets
security patches only, and its authors have moved to Betterleaks, a Go project whose v2 is not yet
stable. The [research](../research/security-checks.md#secret-scanners) has the evidence.

## Decision

- **Kingfisher scans for secrets,** pinned to one version, always with `--no-validate` and
  `--no-update-check` so a scan sends nothing off the machine.
- **It runs in three places:** on staged changes in the pre-commit hook, on all of git history in
  `scripts/check.sh`, and so in CI. Build output and tool directories are excluded.
- **Known findings go in a committed baseline** of fingerprints, `.kingfisher-baseline.yml`. Only fake
  test credentials belong there, and adding an entry loosens the floor, so the maintainer approves it.
- **Machines install it with `cargo install --locked`.** CI downloads the same version's release binary
  and checks its SHA-256 digest, since building it takes about 9 minutes.
- **A leaked secret is rotated first,** at its provider, then removed from the code. Rewriting history
  is optional, since the old value no longer works.
- **Each secret has one use and the narrowest scope** its provider allows. Secrets reach only the workflow
  job that needs them, never a pull request from a fork. Local development uses only the passwordless test
  database in `.cargo/config.toml`.
- **Secrets rotate** when someone with access leaves, when a provider reports exposure, and otherwise
  yearly. Each store lists its secrets by name in [#77](https://github.com/heavymedl-labs/tetsu/issues/77)'s
  deploy docs, never by value.
- **On acceptance,** CONSTRAINTS.md's note that only the fresh review covers secrets is updated to name
  this scan.

## Alternatives considered

- **Betterleaks.** Made by the Gitleaks authors, so it will likely become the common choice. Written in Go,
  and v2 changes its config and output while in release candidate. The fallback if Kingfisher falters.
- **Gitleaks.** The most used today, but feature complete, so new secret formats arrive slowly.
- **TruffleHog.** Strong at checking whether keys still work, which we turn off. AGPL-3.0 and heavier.
- **ripsecrets.** Rust and small, but no history scan, fewer rules, and no release since 2025-05.
- **GitHub secret scanning and push protection.** Best in class, but $19 per committer a month here.
- **A secrets manager, such as Doppler or Vault.** Worth it with many services and people. Today there are
  four secrets in two stores.

## Consequences

- A secret is caught before commit, and again in CI if the hook was skipped. Past commits are covered too.
- Kingfisher is a departure from the common default, justified by staying in Rust, carrying the same
  Betterleaks rules, and passing our tests. Few projects run it, and one MongoDB engineer writes most of
  it, so we lean on MongoDB keeping it.
- Each Kingfisher upgrade may need a newer Rust, since it tracks the latest stable.
- Fake credentials in tests need a baseline entry, or a value the rules skip.
- Replacing it means swapping one command in `check.sh`, the hook, and CI, and regenerating the baseline.

**Weakest evidence.** Adoption: 23 to 43 workflow files on GitHub use Kingfisher, against about 59,000
for Gitleaks. Detection was tested once, with planted secrets in common formats.

**Revisit** under [ADR-0011](0011-server-stack-guardrails.md)'s triggers, or if Kingfisher stops syncing
Betterleaks' rules, misses a secret another scanner finds, or Betterleaks v2 is stable and in wide use.

## Sources

- [Security checks research](../research/security-checks.md), 2026-11-08.
- [Kingfisher](https://github.com/mongodb/kingfisher) README, usage guide, and changelog, v2.17.0.
- [Gitleaks README](https://github.com/gitleaks/gitleaks): feature complete, security patches only.
- [OWASP DevSecOps Guideline: secrets management](https://owasp.org/www-project-devsecops-guideline/latest/01a-Secrets-Management):
  scan before commit and in CI.
- [OWASP Secrets Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Secrets_Management_Cheat_Sheet.html):
  rotation, least privilege, and responding to a leak.
