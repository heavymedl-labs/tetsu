# 0051. Coverage reporting

- Status: Proposed
- Date: 2026-11-13

## Context

Every check run measures each crate's coverage and fails below the floor [ADR-0016](0016-crate-layout-and-coverage.md)
sets: 100% at Tier 1 and 90% elsewhere. The numbers then vanish with the CI runner. The README cannot show them,
and no one can see a crate's coverage fall over time while it stays above the floor.

A badge needs the latest numbers somewhere it can read them when the page loads. The repository is private, so
shields.io cannot read it, and CI cannot store them there, since its token can only read the repository
([secrets.md](../operations/secrets.md)). Only the deploy workflow may read repository secrets
([ADR-0027](0027-secrets-revised.md)).

Criteria, before looking at options: the README shows the measured coverage; nothing adds a stored secret or write
access to CI; no source code leaves GitHub; the free tier covers our use; and the floor stays enforced by our own
check, not by the service.

## Decision

- **Upload each CI run's coverage to [Codecov](https://about.codecov.io),** and show its badge in the README's row.
  `cargo llvm-cov` already measures it, and writes Codecov's format with `--codecov`, so the CI coverage step writes
  one more file.
- **Authenticate with GitHub's OIDC token,** through `codecov/codecov-action` with `use_oidc: true`. The upload
  step's job gains `id-token: write`, and no upload token is stored, so ADR-0027's rule on secrets holds.
- **Pin both the action and the uploader:** the action by commit, as every action is, and its `version` input to an
  exact Codecov CLI release, with the action's integrity check left on. By default it fetches the latest CLI.
- **Our check stays the gate.** Codecov reports and draws the badge; `scripts/check_coverage.py` still fails a run
  below the floor. Codecov's own status checks stay off, so there is one bar, in one place.
- **Upload from CI only,** never from a local run, so the badge reflects `main` as CI checked it.

## Alternatives considered

- **A badge of the floor itself,** such as "Tier 1 100%, others 90% or more". Always true, since the check fails
  below it, and free of tools. It shows the bar, not the score. It is the fallback if this ADR is rejected.
- **CI writes a badge image into the repository.** No new provider, but CI would need write access to `main`,
  which it has never had.
- **Coveralls.** Similar, but Codecov's Rust and OIDC support are better documented, and it is the common default.
- **A Gist or a public bucket behind shields.io.** Needs a stored token with write access, in a workflow that may
  not read secrets.

## Consequences

- Codecov's free Developer plan is one user with unlimited private repositories and 250 private uploads a month,
  read on third-party listings on 2026-11-13, to confirm at sign-up. CI runs on demand
  ([ADR-0023](0023-ci-on-demand.md)), a few times a day at most, so this fits. A second contributor needs a paid seat.
- Codecov stores coverage reports: file paths and which lines ran. It does not store source code. It shows code by
  fetching it from GitHub when someone views a report, through its GitHub App, so installing that app grants
  Codecov read access to the repository. That is the main cost, and a reason to revisit before real business data
  or a second contributor arrives.
- `codecov/codecov-action` was at v7.1.1, released 2026-09-17, MIT, and the Codecov CLI at v11.3.2, released
  2026-10-28, read 2026-11-13.
- The change touches `.github/workflows/ci.yml`, which the floor guard protects, so it needs a `Floor-reason:`, and
  the job's new `id-token: write` goes in [secrets.md](../operations/secrets.md) with the other permissions.
  The maintainer makes the Codecov account and installs its GitHub App.
- Replacing it means deleting one step and one badge.
- Revisit if the free tier's terms change, the repository goes public, which makes uploads tokenless and free, or a
  second contributor arrives.

## Sources

- Codecov, [codecov-action README](https://github.com/codecov/codecov-action), "Using OIDC", read 2026-11-13.
- Codecov, [Frequently asked questions](https://docs.codecov.com/docs/frequently-asked-questions): source code is not
  stored.
- Codecov, [Codecov tokens](https://docs.codecov.com/docs/codecov-tokens).
- [Codecov free tier, 2026](https://agentdeals.dev/vendor/codecov) and
  [Codecov pricing](https://toolradar.com/tools/codecov/pricing), third-party listings, read 2026-11-13.
- [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov), `--codecov`.
