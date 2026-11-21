# 0077. Doubt mode in the fresh review

- Status: Proposed
- Date: 2026-11-21
- Supersedes, for the changes it names, [ADR-0014](0014-checks-and-duties.md)'s rule that the fresh review's
  findings are rule violations only

## Context

[ADR-0014](0014-checks-and-duties.md) has an agent that did not write a change review it before commit, seeing only
the issue and the diff, with findings that are rule violations only. Risky decisions have also had their own
`doubt-driven-development` cycles ([agent skills research](../research/agent-skills.md)), run separately and often,
so a Tier 1 change got two reviews that read the same diff.

The doubt skill differs in stance, not in what it reads: it assumes the author is overconfident, tries to break
named claims rather than check rules, and loops on what it finds. On 2026-11-21 one fresh review run that way, on
the local Mailpit work ([#149](https://github.com/heavymedl-labs/tetsu/issues/149)), found ten real problems:
lost `.env` edits, a closed terminal leaving mail pointed at nothing, a mutant StrykerJS could not kill, and others
a rule check would not look for. Eight were fixed in code, one in the issue, and one is this ADR.

The skill also offers, every cycle, a second opinion from another vendor's model, which Tetsu adopts no tool for.

## Decision

- **The fresh review has a doubt mode,** which [docs/review.md](../review.md) holds: the same reviewer, prompt, and
  inputs, plus a block that names claims to doubt, as questions without the author's reasons, and asks for the
  input, state, or setting that breaks each, or a plain statement that none was found.
- **It is for some changes, not all:** Tier 1 changes, ones that cross a module or isolation boundary, ones that
  change trust (TLS, sessions, secrets, who may do what), and ones the issue or the author calls risky. Other
  changes keep the ordinary review.
- **Its findings may be broken claims,** not only rule violations. "None" stays a valid answer.
- **It loops at most three times.** Each fix gets a new pass on its own diff; real findings after three mean the
  change is not ready, and the maintainer decides.
- **A second vendor's model is only ever offered.** With the maintainer present, the author offers it, given only
  the prompt, the issue, and the diff, read-only, on the maintainer's own account. The maintainer decides each
  time, and the report says whether it ran. The project adopts no such tool, and no check depends on one.
- **A separate doubt cycle stays for decisions made mid-work,** before there is a diff, as AGENTS.md's duties say.

## Alternatives considered

- **Keep the two separate.** Two reviews of the same diff for a risky change, with one framed to agree.
- **Doubt mode for every change.** Three passes cost too much for a typo or a docs change, and the skill itself
  skips trivial changes.
- **Adopt a second vendor's model as a standing reviewer.** A new service in the workflow, needing its own
  evaluation under [ADR-0010](0010-evaluating-technology.md), when an offer the maintainer can decline does the job.

## Consequences

- A risky change gets one review instead of two, and that review hunts for what breaks it.
- Up to three passes cost more agent time on the changes that matter most.
- Which changes count as risky is a judgment, made by the author or the issue; the maintainer can always ask for
  doubt mode.
- A second model's findings come from outside the project's tools, so they are sorted like any other finding,
  never taken as a verdict.

**Weakest evidence.** One review run, on one change. Whether doubt mode keeps finding real problems, or drifts into
noise, shows only over more runs. Revisit if most of its findings turn out to be noise.

## Sources

- [addyosmani/agent-skills: doubt-driven-development](https://github.com/addyosmani/agent-skills), read 2026-11-21.
- [Agent skills research](../research/agent-skills.md), on which skills the duties name.
