# 0033. Deploys on demand

- Status: Accepted
- Date: 2026-11-09
- Supersedes the deploy trigger and image build in [ADR-0012](0012-first-hosting-and-deploys.md)

## Context

[ADR-0012](0012-first-hosting-and-deploys.md) deploys on every merge to `main`, after GitHub Actions builds,
tests, and pushes the image. [ADR-0023](0023-ci-on-demand.md) then made CI run on demand to save GitHub Free's
2,000 private-repository minutes a month, and requires a passing CI run on the exact commit before it deploys.
[ADR-0027](0027-secrets-revised.md) forbids starting a secret-using workflow from `workflow_run`.

By 2026-11-09 the month had used about 940 minutes: 736 on 94 CI runs, most from before ADR-0023, and 204 on four
benchmark runs. A warm CI run bills about 10 minutes, since each job rounds up, and a cold one about 21. A deploy
on every merge that starts CI and builds the image again would bill 16 to 28 minutes per merge batch, so a few
batches a day would spend the month.

## Decision

- **CI publishes the image it checked.** On a manual run on `main`, a last job runs once every check has passed,
  pushes that exact image to the GitHub Container Registry, tagged with its commit, and records its digest as an
  artifact of the run. It holds the run's only `packages: write` token, not a secret, and runs only GitHub's own
  actions. A pull request never publishes.
- **The maintainer starts each deploy** with `gh workflow run deploy`. The deploy workflow refuses unless a CI
  run passed on the exact commit, then takes the digest that run recorded, never the tag, which anything with
  write access could replace. It runs `tetsu migrate` with that image and deploys the digest with the pinned
  Render CLI. It builds nothing and runs no checks of its own.
- **Each deploy is recorded** as a GitHub deployment once Render reports success, so the live commit is known.
- **Rollback starts the same workflow with a commit of `main` older than the live one** that CI passed. It deploys
  that image without migrating: an older binary refuses migrations it does not know, and schema changes follow
  expand and contract.
- **Old images are pruned** after each deploy, keeping the ten newest and the one deployed, by a tested script
  that deletes nothing if the deployed image is not in the list.

The rest of ADR-0012 stands: Render and Neon, migrating before rollout and stopping if it fails, deploying by
digest, and where each secret lives.

## Alternatives considered

- **Deploy every merge, starting CI from the deploy workflow.** Matches ADR-0012, but bills 16 to 28 minutes per
  merge batch.
- **Deploy every merge after waiting for the maintainer's CI run.** The wait itself bills minutes, and a merge with
  no CI run would wait and fail.
- **Build the image in the deploy workflow.** Repeats the build CI just ran, and can produce a different image
  from the one CI checked.
- **Make the repository public,** for unlimited free minutes. A product decision, not a deploy detail; ADR-0014
  already names it as a later option.

## Consequences

- A merge reaches the demo environment only when the maintainer deploys it. With only fictional data there, a
  late deploy costs a walkthrough, not a customer.
- A deploy bills about 3 to 5 minutes. About 60 CI runs, 20 deploys, and occasional benchmarks fit in about 900
  minutes a month.
- The image deployed is the one CI built and checked, byte for byte.
- GitHub's docs say Container registry storage and bandwidth are free for now, with a month's notice before that
  changes. Otherwise a free plan's 500 MB of package storage and 1 GB of transfer a month apply, which pruning
  keeps within.

**Weakest evidence.** The per-run minutes are from November's runs, and a deploy's minutes are estimated, not yet
measured.

**Revisit** if deploys need to follow merges for a real business, if GitHub starts billing Container registry
storage, if the repository goes public, or when Tetsu moves to a paid plan (ADR-0012).

## Sources

- [ADR-0012](0012-first-hosting-and-deploys.md), [ADR-0023](0023-ci-on-demand.md), and
  [ADR-0027](0027-secrets-revised.md).
- GitHub Docs: [GitHub Packages billing](https://docs.github.com/en/billing/concepts/product-billing/github-packages)
  and [GitHub Actions billing](https://docs.github.com/billing/managing-billing-for-github-actions/about-billing-for-github-actions).
- Billed minutes for November 2026, read from the repository's workflow runs on 2026-11-09.
