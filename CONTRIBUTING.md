# Contributing

People and coding agents follow the same workflow and the rules in
[AGENTS.md](AGENTS.md).

## Workflow

1. Pick or open an issue on the
   [Tetsu project](https://github.com/orgs/heavymedl-labs/projects/1). Use the
   **Task** form for work and **Decision needed** for anything that needs an
   ADR, or `scripts/file_issue.py` with a body that follows the form.
2. Branch from `main` with a short name and a type prefix, such as
   `docs/glossary`, `adr/tenant-isolation`, or `ci/coverage`.
3. Keep the branch and pull request to that one issue.
4. Follow the [duties](AGENTS.md#duties), run the [checks](AGENTS.md#checks), and
   do a [docs audit](#docs-audit) if you changed docs or issues. Once per
   clone, install [uv](https://docs.astral.sh/uv/) and [rustup](https://rustup.rs/), run `scripts/install-tools.sh` and `git config core.hooksPath .githooks`,
   so formatters and linters run
   before each commit, and the full checks before each push.
5. Open a pull request with the template filled in and `Closes #N`.

While the maintainer is the only contributor, changes go straight to `main`
instead: follow the duties, run the checks, commit, push, and close the issue by
hand. Steps 2, 3, and 5 return when a second contributor joins or a real business
adopts Tetsu ([ADR-0014](docs/adr/0014-checks-and-duties.md)).

## Agent skills

The [duties](AGENTS.md#duties) name skills from
[addyosmani/agent-skills](https://github.com/addyosmani/agent-skills) (MIT). We refer to them by name
and never edit them, so any way of installing them works.

If your agent already has them, there is nothing to do. Otherwise, install the whole pack, because
some skills share files:

```bash
npx skills add addyosmani/agent-skills
```

Or clone the repository and follow its README for your agent. If a skill is unavailable, say so
rather than skipping the step.

We last [reviewed](docs/research/agent-skills.md) the pack at version 0.6.15. Review it again before
relying on a newer version's changes to these skills.

## Docs audit

Every change to docs, ADRs, issues, or the project board gets an audit before it
lands, so what we write stays true to what we decide.

**Scope.** Start with what you changed. Then search repo docs, open issues, and issue comments for
whatever refers to it, by name and by number. Accepting an ADR or closing an issue always counts as a change
to search for. Widen to the whole project when a change alters a decision, a term, or the project's direction.

Passing checks do not count as an audit. They catch formatting and broken links, not whether something is
still true.

**Check each of these, and fix what you find:**

- **Accuracy.** Claims match accepted ADRs, the code, and their sources. Facts that age
  have a date, and evidence says how strong it is.
- **Duplication.** Each concept has one home, and everything else links to it.
- **Misleading text.** Nothing stale, nothing that presents an open decision as settled,
  and nothing fitted to one industry in platform docs.
- **Merge or remove.** Merge or delete any document or section that no longer earns its
  place, and retarget links to it. Fewer documents is the better outcome. Ask what this change
  removed or merged, and when it only adds, why nothing could go.

**What stays.** Accepted ADRs change only as [ADR-0001](docs/adr/0001-how-we-work.md)
allows, so a wrong decision gets a superseding ADR. Research pages stay dated records:
add a note rather than rewriting what was found, and when shortening one, keep what ADRs and issues
cite and point to the full version in git history. When a fix is out of scope, open an issue.

**Record it.** Add an `Audit:` footer to the commit or a line in the pull request, saying the scope, what you
searched, what changed, and what was removed, such as `Audit: scoped, searched docs and issues for #29, updated two`. Say so if
you sampled rather than read. For issue and board changes, say the same in your report of the work.

## Project board

The [Tetsu project](https://github.com/orgs/heavymedl-labs/projects/1) README
explains its fields. GitHub adds each new issue and pull request to the board,
sets its status to Todo, and to Done when it closes. Keep the rest true:

- Every issue needs a milestone, a tier, and a priority. `scripts/file_issue.py`
  files one with all three in one step; set them by hand on an issue filed any
  other way, such as from a form or as a sub-issue.
- The **Roadmap** view groups work by status and sorts it by priority, so set
  priority rather than moving items by hand.
- Post a status update (On track, At risk, or Off track) when a milestone's
  scope or risk changes, and at the end of a working session that closed
  issues. Say what finished, what is next, any risk, and whether the CI run
  that ended the session passed ([ADR-0023](docs/adr/0023-ci-on-demand.md)).
- Update the project README when milestones or field meanings change.

## Commits

Messages follow [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/),
the format [Commitizen](https://commitizen-tools.github.io/commitizen/) checks by default:

```text
<type>(<optional scope>): <description>

<optional body>

<optional footers>
```

| Type | Use it for |
| --- | --- |
| `feat` | New product behavior |
| `fix` | A bug fix in product behavior or tooling |
| `docs` | Documentation, including ADRs and research |
| `test` | Tests and benchmarks |
| `refactor` | A code change that keeps behavior the same |
| `perf` | A code change that makes Tetsu faster |
| `style` | Formatting with no effect on meaning |
| `build` | The build, dependencies, or container image |
| `ci` | CI workflows and checks |
| `chore` | Other repository upkeep |
| `revert` | Undoing an earlier commit |

- **Scope** is optional and names the area changed, such as `adr`, `research`,
  `benchmarks`, or a module's name. Use one when a change stays in one area.
- **Description** is lowercase and imperative, and the whole first line stays under
  72 characters.
- **Breaking changes** add `!` before the colon and a `BREAKING CHANGE:` footer.
- **The body** says why when it is not obvious.

For example: `docs(adr): accept first hosting and deploys`.

One logical change per commit. Commits and pull requests carry no agent attribution,
such as `Co-Authored-By` trailers or "Generated with" footers.

## Review

The pull request states its rigor tier, and reviewers check that tier's bar in
[ADR-0005](docs/adr/0005-testing-and-performance.md), not only a green build.
