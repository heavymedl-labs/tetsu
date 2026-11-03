# Contributing

People and coding agents follow the same workflow and the rules in
[AGENTS.md](AGENTS.md).

## Workflow

1. Pick or open an issue on the
   [Tetsu project](https://github.com/users/HeavyMedl/projects/1). Use the
   **Task** form for work and **Decision needed** for anything that needs an
   ADR.
2. Branch from `main` with a short name and a type prefix, such as
   `docs/glossary`, `adr/tenant-isolation`, or `ci/coverage`.
3. Keep the branch and pull request to that one issue.
4. Run the [checks](AGENTS.md#checks).
5. Open a pull request with the template filled in and `Closes #N`.

## Commits

Write the subject in the imperative, under 72 characters, and say why in the
body when it is not obvious. One logical change per commit. Commits and pull
requests carry no agent attribution, such as `Co-Authored-By` trailers or
"Generated with" footers.

## Review

The pull request states its rigor tier, and reviewers check that tier's bar in
[ADR-0005](docs/adr/0005-testing-and-performance.md), not only a green build.
