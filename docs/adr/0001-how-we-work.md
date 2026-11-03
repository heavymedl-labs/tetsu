# 0001. How we work

- Status: Proposed
- Date: 2026-11-02

## Context

Tetsu is long-lived and built largely by coding agents. Decisions need a
durable record, and every agent needs the same rules.

## Decision

- **Decisions.** Significant decisions are ADRs in `docs/adr`, citing reputable
  sources. Adopting a framework, library, service, or tool counts. Agents write
  ADRs as `Proposed`; only the maintainer accepts them. An accepted ADR changes
  only by being superseded.
- **Quality bar.** Testing, rigor tiers, and performance follow
  [ADR-0005](0005-testing-and-performance.md).
- **Tooling.** CI runs on GitHub Actions with every action pinned to a commit
  SHA. Work is tracked as issues on the Tetsu project.
- **Agent instructions.** `AGENTS.md` is the single source of agent rules;
  tool-specific files such as `CLAUDE.md` only point to it.

## Alternatives considered

- **Decisions in a wiki or commit messages.** They drift from the code and lose
  the rejected options.
- **One instruction file per agent tool.** The copies drift apart.

## Consequences

Every new dependency costs a short ADR. Rules change in one place.

## Sources

- Michael Nygard, "Documenting Architecture Decisions," 2011.
- GitHub, "Security hardening for GitHub Actions": pin actions to a full SHA.
- AGENTS.md, agents.md.
