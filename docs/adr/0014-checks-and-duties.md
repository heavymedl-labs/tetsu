# 0014. Checks and duties

- Status: Accepted
- Date: 2026-11-06
- CI after every push suspended by [ADR-0023](0023-ci-on-demand.md)
- Return of pull requests and branch protection for everyone superseded by [ADR-0076](0076-pull-requests-from-contributors.md)

## Context

Coding agents write most of Tetsu, and we want them to test every change, avoid
duplication, respect boundaries, and simplify as they go. Instructions alone cannot guarantee
that. Anthropic's docs say `CLAUDE.md` is context rather than enforced configuration, and an
ETH Zurich study found that context files tend to lower task success and raise cost. OpenAI
reports better results from custom linters and structural tests.

Tools alone cannot do it either. No check can tell whether a change was the simplest one,
or whether two pieces of code express the same idea. This repository has also seen the
weakness of self-reporting: docs audit footers overstated what was searched until the
maintainer noticed.

The maintainer commits directly to `main` until a second contributor joins, and the repository
is private on GitHub Free, which offers no branch protection. CI therefore reports after code
lands and cannot block it. [ADR-0005](0005-testing-and-performance.md) sets test bars by risk,
but does not cover what agents do before CI runs, or other languages. The landing page
([#32](https://github.com/heavymedl-labs/tetsu/issues/32)) may bring TypeScript.

## Decision

**Every contribution, in any language, meets the same rules, through two kinds of control:
checks and duties.** A check is automated and gives the same answer every time. A duty is
judgment that an agent or person carries out. We catch all we can with checks, and accept
that duties cover the rest.

### The rules

| Rule | Check | Duty |
| --- | --- | --- |
| Changes come with tests that would catch a mistake | Coverage never drops, and mutation testing runs on changed code | Write a failing test first, and test behavior |
| No new duplication | New clones outside tests fail | Search before writing, and merge duplicated ideas |
| Boundaries hold | Structural tests | Check the design against boundaries, and raise an ADR to change one |
| Code gets simpler | Complexity limits only tighten | After tests pass, make one pass to remove or reduce code |
| Checks stay strong | Suppressions, stubs, skipped or deleted tests, and raised limits fail without a written reason | Fix the problem rather than silencing the check |

The steps for duties live in [AGENTS.md](../../AGENTS.md#duties), as
[ADR-0001](0001-how-we-work.md) requires. Only the maintainer approves loosening a limit.

Tests may repeat themselves when that keeps them readable. Pure configuration needs no tests.

### Skills carry the how

We adopt [addyosmani/agent-skills](https://github.com/addyosmani/agent-skills) for how to carry out
duties, rather than writing our own versions. [AGENTS.md](../../AGENTS.md#duties) names each skill,
[CONTRIBUTING.md](../../CONTRIBUTING.md#agent-skills) says how to get them, and our docs hold only
what the skills lack or where Tetsu differs. Where they disagree, this repository wins. We do not
edit the skills, because they are shared across projects and track upstream. The evidence is in
[Agent skills research](../research/agent-skills.md).

### Rules name no tools

[CONSTRAINTS.md](../../CONSTRAINTS.md) maps each rule to a tool, one row per language, in the format
the `constraint-driven-development` skill reads. Apart from the skill pack, this ADR adopts no tool,
and each still goes through [ADR-0010](0010-evaluating-technology.md).

A language lands only once its row is filled. If no good tool exists for a rule, the row records
the gap and the fresh review covers that rule until one does. Adding a language therefore
includes evaluating whether it can meet these rules.

### The bar follows risk, not language

ADR-0005's tiers set the limits, in any language. Tier 1 keeps its bar of no surviving mutants.
Tier 2 and Tier 3 limits for mutation testing are set when
[#30](https://github.com/heavymedl-labs/tetsu/issues/30) has real results to tune against.
Changes to tooling alone keep having no tier, so no numeric limit, but still get tests and
duties. Docs cannot have tests, so the [docs audit](../../CONTRIBUTING.md#docs-audit) replaces
the test and review duties there.

### Fresh review

Before commit, an agent that did not write the change reviews it, using the
`code-review-and-quality` skill and [docs/review.md](../review.md), and seeing only the issue and the
diff. Its main question is which changes are unnecessary, because that gives it a job, where asking
the author why something changed invites a defense. Findings are rule violations only, and "none" is
a valid answer.

The maintainer stays the final check, and spot-checks changes rather than reading every line.

### Where each control runs

1. **While working:** duties, and optional agent hooks for fast feedback.
2. **Before push:** one script, `scripts/check.sh`, run by a local git hook.
3. **After push:** CI runs that same script. A red `main` gets fixed or reverted before other work.

### While the maintainer is solo

Direct commits to `main` continue, and the hook and the script are the gate, knowing they can be
bypassed. Tier 1 changes are no exception: the maintainer gives the review ADR-0005 requires before
the push. When a second contributor joins or a real business adopts Tetsu, whichever comes first,
branches and pull requests return, and we add branch protection.

### Other repositories

Another Tetsu repository, such as the landing page, carries a copy of these rules and its own
table, with a note that this repository's version is the source.

## Alternatives considered

- **Instructions only, in `AGENTS.md` or `CONTRIBUTING.md`.** Cheap, but advisory, and longer
  instruction files cost attention on every task.
- **CI only.** Deterministic, but it cannot judge simplicity or duplicated ideas.
- **Writing our own duties in full.** Works without the pack, but duplicates what it maintains.
- **Editing the skills for Tetsu.** Puts rules where they fit, but would reach other projects on the
  same machine, conflict with upstream, and stay out of contributors' reach.
- **Rules per language.** Fits each ecosystem, but the bar would drift with whichever language
  a change happens to use.
- **Enforcing test-first order.** CI cannot see the order. Hook-based tools such as TDD Guard
  try, but mutation testing shows the result that matters: whether tests catch mistakes.
- **Failing on any duplication.** Pushes agents toward premature abstractions, which works
  against simplifying.
- **A limit on files or lines per change.** No honest threshold exists, since a rename can touch
  50 files. Spread stays a question for the fresh review.
- **Branch protection now.** Needs a paid plan or a public repository. The repository holds
  unredacted studio screenshots, and one contributor can bypass their own gate anyway.

## Consequences

Agents do more work before each commit, and each commit costs a review run. Contributors need the
skill pack installed, and an upstream change can change agent behavior, so we review it before
relying on a new version. Limits hold from the
first line of code rather than being retrofitted. Adding a language costs more up front.

**Tradeoffs.** Hooks can be bypassed and CI cannot block, so until branch protection exists these
rules rest partly on discipline. Fresh reviews are not deterministic, and can miss things or
invent them. Tools differ in quality, so a mutation score in one language will not exactly match
another.

**Weakest evidence.** We found no measurement of which setup works best. Anthropic and OpenAI
describe their own practice, the ETH study's benchmark is mostly Python, and GitClear's duplication
data is vendor research showing correlation. The removal question comes from a Reddit thread we
could read only through another agent's summary. Candidate tools have not been evaluated, and we
did not run the skill pack's evals.

**Next.**

- [#33](https://github.com/heavymedl-labs/tetsu/issues/33) builds `scripts/check.sh`, the hook, and
  the floor guard. [#30](https://github.com/heavymedl-labs/tetsu/issues/30) adds the Rust row, so the
  checks start with the first code.
- [#25](https://github.com/heavymedl-labs/tetsu/issues/25) provides the boundary checks.
- [#32](https://github.com/heavymedl-labs/tetsu/issues/32) must weigh whether its options can meet
  these rules.

**Revisit** when a second contributor joins, when a real business adopts Tetsu, if the repository
goes public, or if fresh reviews mostly produce noise or miss real defects that later surface.

## Sources

- Anthropic, [Claude Code best practices](https://code.claude.com/docs/en/best-practices) and
  [memory docs](https://code.claude.com/docs/en/memory): instruction files as context, hooks for
  rules with no exceptions, and fresh reviewers.
- OpenAI, "Harness engineering," openai.com/index/harness-engineering, 2026: a short
  `AGENTS.md`, custom linters with fix instructions, structural tests, and recurring cleanup.
- Gloaguen et al., [Evaluating AGENTS.md](https://arxiv.org/abs/2602.11988), 2026.
- GitClear, AI Copilot Code Quality report, 2025, as covered by
  [DevClass](https://devclass.com/2025/02/20/ai-is-eroding-code-quality-states-new-in-depth-report/).
- [Agent skills research](../research/agent-skills.md): the pack's coverage, health, and conflicts,
  2026-11-06.
- [cargo-mutants `--in-diff`](https://mutants.rs/in-diff.html) and
  [jscpd](https://github.com/kucherenko/jscpd): examples showing that checks on changed code and on
  new clones exist. Not adopted here.
