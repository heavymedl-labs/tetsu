# AGENTS.md

Instructions for coding agents working in Tetsu. Humans follow them too.

## Start here

Read [README.md](README.md), then [ARCHITECTURE.md](ARCHITECTURE.md). Its
invariants are hard rules. Use the terms in [docs/GLOSSARY.md](docs/GLOSSARY.md)
exactly. Follow ADRs in [docs/adr](docs/adr/README.md); until accepted, treat
them as current intent.

## Current phase

The repository holds documentation, ADRs, CI, and project tooling only. Do not
add Rust crates, `Cargo.toml`, or application code until an issue opens the
implementation phase.

## Rules

- Adopting a framework, library, service, or tool needs a `Proposed` ADR
  first. Only the maintainer accepts ADRs
  ([ADR-0001](docs/adr/0001-how-we-work.md)).
- Meet the rigor tier and performance rules for the area you change
  ([ADR-0005](docs/adr/0005-testing-and-performance.md)).
- Settle an [open decision](docs/adr/README.md#open-decisions) only when its
  issue is your task, by proposing an ADR. Otherwise raise it on that issue
  and wait.
- Follow the workflow in [CONTRIBUTING.md](CONTRIBUTING.md).

## Checks

Run these before you push to `main` or open a pull request:

```bash
scripts/check-adrs.sh
npx markdownlint-cli2@0.24.0 "**/*.md"
```

CI also checks links and lints workflows. Keep the markdownlint-cli2 version
matched to the one the CI action bundles.

## Writing

Keep docs short and plain. Lead with the point, prefer specific nouns and
active verbs, and cut sentences that do no work. Do not use em or en dashes as
punctuation. Each concept has one home; link to it instead of repeating it.
New terms go in the glossary.
