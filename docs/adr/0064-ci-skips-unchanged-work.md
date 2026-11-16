# 0064. CI skips work whose inputs are unchanged

- Status: Proposed
- Date: 2026-11-16

## Context

A manual CI run checks every change since the last commit a successful manual run checked, its base
([ADR-0023](0023-ci-on-demand.md)). Mutation testing already uses that base, and only mutates the diff. The rest
runs whole every time: coverage twice, stable then nightly for branches
([ADR-0020](0020-branch-coverage.md)); the doc tests twice; the Python tests of `scripts/`; and the front end's
tests and mutation testing.

Outside CI, `scripts/check.sh` already skips coverage and mutation testing when a fingerprint of everything they
read matches the last full pass on that machine. CI turns this off, since a runner starts clean. That is ADR-0023's
point: a clean machine finds what a cached one hides.

Read from the 34 successful batches before 2026-11-16, the Rust inputs were unchanged since the base in 10 of them,
`scripts/` in 26, and the front end and its lockfile in 23. On a run with no Rust change, coverage and the doc
tests are about 5.5 of the checks job's 11 minutes. Two dispatches on 2026-11-16 checked the same commit twice,
about 12 minutes for nothing.

## Decision

- **A run whose base is its own commit stops at once,** and says so: a successful run has already checked every
  change.
- **Each group of checks runs only when one of its inputs changed since the base.** The inputs are those the
  fingerprint already names, by group:
  - Rust coverage and the doc tests: `crates/`, the manifests, the lockfile, the toolchains, `.cargo/`,
    `scripts/check.sh`, and `scripts/check_coverage.py`.
  - The Python tests: `scripts/`.
  - The front end's tests and mutation testing: `web/`, the templates Vite reads, `package.json`,
    `package-lock.json`, and their configuration.
- **Formatters, linters, the dependency audits, the secret scan, and the floor guard always run.** They are quick,
  and an advisory or a leaked secret can appear without any change here.
- **A skipped group is named in the log,** with the base it compares with, so a reader sees what ran.
- **This supersedes ADR-0023's assumption that every run checks everything on a clean machine,** on acceptance.
  A clean machine still checks every change; unchanged code was checked by the run that passed the base.

## Alternatives considered

- **Path filters on the workflow's triggers.** GitHub's `paths:` filter decides whether a run starts at all, not
  which checks run in it, and a manual dispatch ignores it.
- **Keep checking everything every run.** Simplest, and it finds what only a clean machine shows in unchanged
  code at once rather than at the next change. It costs about 5.5 minutes on a run with no Rust change, and some
  minutes on most runs.
- **A separate workflow per area.** Each job pays about 3 minutes of setup and is rounded up to a minute, so more
  jobs cost more minutes here, not fewer ([ADR-0023](0023-ci-on-demand.md)).

## Consequences

- A run with no Rust change should bill about 5.5 minutes less, and most runs about a minute less.
- A problem only a clean machine shows, in code no change touched, such as a tool that behaves differently on a
  new runner image, is found at the next run that changes that area, or at the monthly fresh build cache, rather
  than at once.
- The groups must name every input their checks read. A missing input lets a change skip the checks that would
  catch it, so the groups come from the fingerprint `check.sh` already keeps, and a test checks they agree.

**Weakest evidence.** The share of batches with no change in each area comes from one busy month.

**Revisit** if a skipped group would have failed on a later run more than once, or when a second contributor
joins and pull requests return.

## Sources

- The Actions API's run history for heavymedl-labs, read 2026-11-16: the diff of each successful batch since its
  base, and each step's time.
- [GitHub: workflow syntax, `on.<push|pull_request>.paths`](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#onpushpull_requestpull_request_targetpathspaths-ignore):
  path filters decide whether a workflow runs, for push and pull request events only.
- [Corrode: tips for faster Rust CI builds](https://corrode.dev/blog/tips-for-faster-ci-builds/): skip work whose
  inputs have not changed.
