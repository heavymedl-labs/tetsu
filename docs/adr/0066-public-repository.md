# 0066. A public repository

- Status: Proposed
- Date: 2026-11-16

## Context

The organization is on GitHub Free. Its private repositories get 2,000 Actions minutes a month, about $12 of
compute, and November had about 330 left on 2026-11-16. [ADR-0023](0023-ci-on-demand.md),
[ADR-0063](0063-ci-minutes-per-run.md), [ADR-0064](0064-ci-skips-unchanged-work.md), and
[ADR-0065](0065-arm-runner-for-checks.md) each exist to spend fewer of them. GitHub bills nothing for standard
hosted runners in a public repository.

Making the repository public exposes its whole history, not only its files. A scan of every commit with Betterleaks
found nine connection strings, all test fixtures with placeholder hosts and passwords; the real credentials live
only in GitHub's and Render's secrets. Every commit carries the maintainer's email, which the maintainer accepts.

## Decision

- **The repository is public, with no license.** The code is all rights reserved: it may be read, and forked on
  GitHub as its terms allow, but no one may use, copy, modify, or deploy it. The README says so. A license is a
  separate decision, for when Tetsu has a name and a business model.
- **CI keeps its shape.** It still runs on demand and on pull requests, and skips unchanged work. The pre-push hook
  already runs the checks, so CI on every push would repeat them, and a skipped check is still a faster run. Minutes
  stop being the reason; nothing in those ADRs has to change to stay correct.
- **A pull request from outside the organization runs CI only once the maintainer approves it,** for every outside
  contributor, not only first-time ones. Such a run gets no secrets and a read-only token, and the workflows that
  use secrets run only when the maintainer starts them.
- **The container image stays private.** Package visibility is separate from the repository's, and Render pulls the
  image with its registry credential.

## Alternatives considered

- **Stay private and buy minutes.** About $0.006 a minute on x64, for a project with no revenue yet.
- **Stay private and spend fewer minutes.** ADR-0063 and ADR-0064 already cut a run from about 19 billed minutes to
  about 11, and a typical month still nears the limit.
- **Public under an open-source license now.** A license is a grant that cannot be taken back for code already
  released, and Tetsu is still a working name.

## Consequences

- CI minutes no longer run out, so a run can be started whenever it helps.
- ADR-0065's Arm trial no longer saves money; [#143](https://github.com/heavymedl-labs/tetsu/issues/143), which
  tracks it, can close.
- Anything committed is public at once and for good, since copies and archives keep it even if the repository goes
  private again. Betterleaks already runs on every commit; it now guards a public history.
- Docs that name infrastructure, such as the Render service and the Neon project, are public. None of it is a
  secret.

**Revisit** if the repository goes private again, when the minute budget and the ADRs above apply as written, or
when Tetsu takes a license.

## Sources

- [GitHub: billing for GitHub Actions](https://docs.github.com/en/billing/concepts/product-billing/github-actions):
  standard GitHub-hosted runners are free in public repositories, read 2026-11-16.
- [GitHub: approving workflow runs from forks](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/approve-runs-from-forks):
  a repository can require approval for runs from all outside collaborators.
- [GitHub: licensing a repository](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/licensing-a-repository):
  without a license, default copyright laws apply.
