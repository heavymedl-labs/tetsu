# Contributing

People and coding agents follow the same workflow and the rules in
[AGENTS.md](AGENTS.md).

## Workflow

1. Pick or open an issue on the
   [Tetsu project](https://github.com/orgs/heavymedl-labs/projects/1). Use the
   **Task** form for work and **Decision needed** for anything that needs an
   ADR.
2. Branch from `main` with a short name and a type prefix, such as
   `docs/glossary`, `adr/tenant-isolation`, or `ci/coverage`.
3. Keep the branch and pull request to that one issue.
4. Run the [checks](AGENTS.md#checks).
5. Open a pull request with the template filled in and `Closes #N`.

While the maintainer is the only contributor, changes go straight to `main`
instead: run the checks, commit, push, and close the issue by hand. Steps 2, 3,
and 5 return when a second contributor joins.

## Project board

The [Tetsu project](https://github.com/orgs/heavymedl-labs/projects/1) README
explains its fields. Keep the board true as work moves:

- Give every new issue a milestone, priority, and tier, and place it in the
  **Roadmap** view where it will be worked, top first.
- When an issue closes, set its status to Done and move it below open work.
- Post a status update (On track, At risk, or Off track) whenever an issue
  closes or a milestone's scope changes. Say what finished, what is next, and
  any risk.
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
