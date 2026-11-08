# AGENTS.md

Instructions for coding agents working in Tetsu. Humans follow them too.

## Start here

Read [docs/invariants.md](docs/invariants.md), which are hard rules, then only what
your task needs. Use glossary terms exactly, and look one up in
[docs/GLOSSARY.md](docs/GLOSSARY.md) when unsure. Follow ADRs, and treat a proposed
one as current intent. The [ADR index](docs/adr/README.md) says when to read each.

| Task | Also read |
| --- | --- |
| Code or scripts | [CONSTRAINTS.md](CONSTRAINTS.md) |
| Planning a feature or module | [ARCHITECTURE.md](ARCHITECTURE.md), and the ADRs the index names |
| Adopting a tool | [ADR-0010](docs/adr/0010-evaluating-technology.md) |
| Docs, ADRs, or issues | [Docs audit](CONTRIBUTING.md#docs-audit) |
| Committing | [Commits](CONTRIBUTING.md#commits) |

## Current phase

The walking skeleton is in progress. HTTP serving and deployment remain in
[#19](https://github.com/heavymedl-labs/tetsu/issues/19). The [README](README.md)
describes what is implemented.

## Rules

- Adopting a framework, library, service, or tool needs a `Proposed` ADR
  first. Only the maintainer accepts ADRs
  ([ADR-0001](docs/adr/0001-how-we-work.md)).
- Evaluate what you adopt through the steps in
  [ADR-0010](docs/adr/0010-evaluating-technology.md).
- Meet the rigor tier and performance rules for the area you change
  ([ADR-0005](docs/adr/0005-testing-and-performance.md)).
- Settle an [open decision](docs/adr/README.md#open-decisions) only when its
  issue is your task, by proposing an ADR. Otherwise raise it on that issue
  and wait.
- Follow the workflow in [CONTRIBUTING.md](CONTRIBUTING.md).
- Write commit messages in the [Conventional Commits](CONTRIBUTING.md#commits) format.
- Audit every change to docs, issues, or the project board, as
  [CONTRIBUTING.md](CONTRIBUTING.md#docs-audit) describes.
- Read [CONSTRAINTS.md](CONSTRAINTS.md) before writing code, and do not weaken it
  to make a change pass. Add a language only once its row there is filled.

## Duties

Every contribution, in any language, follows these steps
([ADR-0014](docs/adr/0014-checks-and-duties.md)). Each names an agent skill, set up
as [CONTRIBUTING.md](CONTRIBUTING.md#agent-skills) describes. Where a skill and this
repository disagree, this repository wins. Docs changes take the
[docs audit](CONTRIBUTING.md#docs-audit) in place of steps 2 and 7.

1. **Plan** (`planning-and-task-breakdown`, and `spec-driven-development` for new
   behavior). Say how the change fits the existing code, and list each file you
   expect to touch and why. Track work in issues and ADRs, not plan files.
2. **Test first** (`test-driven-development`). Scripts need tests. Pure
   configuration does not.
3. **Reuse.** Search for existing code before writing new code. Repetition in
   tests is fine when it keeps them readable.
4. **Keep boundaries** (`api-and-interface-design`). Raise an ADR rather than
   crossing one. Use `doubt-driven-development` on risky decisions.
5. **Simplify** (`code-simplification`). Once tests pass, make one pass to remove
   or reduce code.
6. **Report.** Compare your plan with `git diff --stat`, give one sentence per
   changed file, and explain any file you did not plan.
7. **Get a fresh review.** Before commit, run [docs/review.md](docs/review.md) in a
   new context.

Also use `security-and-hardening` for Tier 1 and `frontend-ui-engineering` for UI.
Use `git-workflow-and-versioning` for commits, but follow
[CONTRIBUTING.md](CONTRIBUTING.md#workflow) on branches.

## Checks

Run these before you push to `main` or open a pull request:

```bash
scripts/check.sh
```

It needs the Node version in `.nvmrc`, uv, `scripts/install-tools.sh` run once, and a
test Postgres in `TEST_DATABASE_URL`, used by one run at a time, since each run drops every
test database when it ends. `scripts/check.sh --fast` runs only formatters and
linters, as the pre-commit hook does. CI
runs the same script, and also checks links and lints workflows.

## Writing

Every line costs tokens each time it is read, so merge or remove before adding.
Keep docs short and plain, with normal grammar so people can read them. Lead with
the point, prefer short words, specific nouns, and active verbs, and keep to one
idea per sentence. Cut filler, hedging, and sentences that do no work. Do not use
em or en dashes as punctuation. Each concept has one home; link to it instead of
repeating it. New terms go in the glossary.
