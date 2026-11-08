# 0023. CI on demand

- Status: Accepted
- Date: 2026-11-08

## Context

[ADR-0014](0014-checks-and-duties.md) runs `scripts/check.sh` three times: on commit, the formatters and linters;
before push, everything; and after push, everything again in CI. The organization is on GitHub Free, whose private
repositories get 2,000 Actions minutes a month ([#70](https://github.com/heavymedl-labs/tetsu/issues/70)).

November used 797 minutes in its first eight days, 427 of them on 2026-11-08, read from GitHub's billing API. Each
push to `main` runs five jobs, and GitHub bills each job rounded up to a whole minute: the checks take 3, the image
2, the benchmark harness 2, and the link check and workflow lint 1 each, about 9 minutes in all. Each push gets its
own run, docs-only pushes run everything, and the pre-push hook has already run the same checks on the
maintainer's machine. At that pace the month's minutes run out within days, and GitHub then stops running
workflows until the month resets.

What CI adds beyond the hook is a clean machine, the image build, the benchmark harness compile, the link check,
and workflow lint.

## Decision

- **While the maintainer is the only contributor, CI runs on demand and on pull requests, not on every push.**
  The pre-push hook stays the gate on every push. Anyone can start a run from the Actions tab or with
  `gh workflow run CI`.
- **A run checks a batch.** It runs mutation testing against the last commit a successful CI run checked, so the
  run covers every change since, however many pushes that was.
- **A run uses two jobs, not five:** the checks in one, and the image, benchmark harness compile, link check, and
  workflow lint in the other, which saves the minutes each short job is rounded up to.
- **A session that pushed code ends with a CI run,** and its status update, which CONTRIBUTING.md already asks
  for, says whether that run passed.
- **A CI run passes on the exact commit before it deploys,** once deploys exist
  ([#19](https://github.com/heavymedl-labs/tetsu/issues/19)), so nothing a clean machine has not checked goes
  live.
- **On acceptance,** ADR-0014's third run is marked superseded by this ADR, for as long as this ADR stands.

## Alternatives considered

- **Keep CI on every push, with path filters and fewer jobs.** Docs-only pushes would cost about a minute, but a
  push that touches code still costs about 6, and most pushes here touch code or scripts. It slows the drain
  without stopping it.
- **A monthly spending budget.** Overage costs $0.006 a minute, so a few dollars covers a busy month. It needs a
  payment method, and still spends minutes repeating what the hook checked. It is a safety net to weigh on its
  own, not instead of this.
- **A public repository.** Standard runners are then free, but the repository is private on purpose.
- **A self-hosted runner on the maintainer's machine.** Free minutes, but workflow jobs would run on a personal
  computer, and it is the same machine the hook already checks on.

## Consequences

- Pushes cost no minutes. The first batch runs, on 2026-11-08, billed about 9 minutes each, so ten a month use
  about 90 of the 2,000.
- A problem only a clean machine shows, such as a tool cached locally or an untracked file the tests read, is
  found at the next run rather than at the push. The run then fails, and is fixed before other work, as
  ADR-0014 says of a red `main`.
- `main` can hold commits no CI run has checked, so the end-of-session run matters more than before.
- Replacing this is one change to the workflow's triggers.

**Weakest evidence.** Eight days of usage, from one busy week. Whether a clean machine finds problems the hook
misses has not been measured.

**Revisit** when a second contributor joins and pull requests return, when the plan gives more minutes, or if a
batch run finds something the hook missed twice in a month.

## Sources

- GitHub's billing usage API for heavymedl-labs, read 2026-11-08: 797 Actions minutes in November.
- [GitHub's plans](https://docs.github.com/en/get-started/learning-about-github/githubs-plans) and
  [Microsoft Learn: GitHub billing](https://learn.microsoft.com/en-us/training/modules/github-introduction-products/4-github-billing):
  2,000 minutes a month on Free for private repositories.
- [Blacksmith: how to reduce spend in GitHub Actions](https://www.blacksmith.sh/blog/how-to-reduce-spend-in-github-actions):
  each job is billed rounded up to a minute, so fewer jobs cost less.
- [Imperial College RSE: a more rational use of CI with GitHub Actions](https://blogs.imperial.ac.uk/research-software-engineering/2024/06/26/adopting-a-more-rational-use-of-continuous-integration-with-github-actions):
  trigger fewer runs, and use `workflow_dispatch` to run checks when they are wanted.
