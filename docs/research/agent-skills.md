# Agent skills

An evaluation of [addyosmani/agent-skills](https://github.com/addyosmani/agent-skills) for the duties
in [ADR-0014](../adr/0014-checks-and-duties.md), read 2026-11-06 at version 0.6.15, commit
`301b02b`. This is research, not a decision. Where it disagrees with an ADR, the ADR wins.

## Criteria

Set before reading the pack. It must:

- cover most duties, so Tetsu does not maintain its own versions
- work with more than one agent, since Tetsu's rules are not tied to one vendor
- be plain text that any contributor can read and install
- let a repository override it without editing the skills

## Project health

| Signal | Reading |
| --- | --- |
| License | MIT |
| Security advisories | None published |
| Latest release | 0.6.15, 2026-10-31 |
| Last push | 2026-11-01 |
| Contributors | 84, with one holding 404 of 776 listed commits |
| Open issues and pull requests | 130 |
| Stars and forks | About 113,000 and 11,900 |

Downloads and minimum Rust version do not apply. Popularity says little about quality. The pack has
its own evals, which we did not run.

The main risk is that skills are instructions an agent follows with the user's permissions, so a
change upstream changes what agents do here. Nothing updates a clone until someone pulls.

## Coverage

| Duty | Skill | Coverage |
| --- | --- | --- |
| Plan and file list | `planning-and-task-breakdown` | Partial: lists likely files, but nothing compares them to the diff |
| Design and boundaries | `spec-driven-development`, `api-and-interface-design` | Good |
| Test first | `test-driven-development` | Strong |
| Reuse | `code-review-and-quality` | Weak: only at review, not before writing |
| Simplify | `code-simplification`, `incremental-implementation` | Strong |
| Fresh review | `code-review-and-quality` | Strong |
| Risky decisions | `doubt-driven-development` | Strong |
| Checks stay strong | `constraint-driven-development` | Strong |
| Commits and CI | `git-workflow-and-versioning`, `ci-cd-and-automation` | Good |
| ADRs | `documentation-and-adrs` | Good, and it follows a repository's existing format |

## Where the pack and Tetsu differ

- **Duplication in tests.** `test-driven-development` prefers readable repetition in tests, and
  `incremental-implementation` prefers three similar lines over an early abstraction.
- **Doubt.** `doubt-driven-development` skips trivial changes and offers a second model every cycle.
- **Numbers.** `constraint-driven-development` defaults to 80% of changed lines and a 60% mutation
  score. ADR-0005 asks for more.
- **Branches.** `git-workflow-and-versioning` assumes feature branches.
- **Tooling.** `test-driven-development` skips configuration changes.
- **Review output.** `code-review-and-quality` allows nits and optional suggestions.
- **Tracking.** The planning and spec skills write their own files, where Tetsu uses issues and ADRs.

## What no skill covers

- Asking which changes are unnecessary
- Comparing the plan to `git diff --stat`, with one sentence per file
- Searching for existing code before writing
- Requiring tools for a language before its first code lands
- Clone and complexity checks, and any Rust tooling: the constraint skill's tool table covers
  JavaScript and Python-style stacks, and its floor guard is Node with no Rust patterns

## Weakest evidence

We read four skills closely and parts of others, and only searched the rest. We did not run the
evals, or measure whether agents follow these skills better than instructions in `AGENTS.md`. Other
skill packs were not compared.
