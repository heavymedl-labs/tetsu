# 0076. Pull requests from contributors

- Status: Accepted
- Date: 2026-11-21
- Supersedes [ADR-0014](0014-checks-and-duties.md)'s return of branches, pull requests, and branch protection when a
  second contributor joins

## Context

[ADR-0014](0014-checks-and-duties.md) has the maintainer commit straight to `main` while solo, and brings branches,
pull requests, and branch protection back for everyone once a second contributor joins. The first one has joined: a
volunteer testing Tetsu's security under [SECURITY.md](../../SECURITY.md), who may also propose fixes. They have
read access only, so they cannot push to the repository at all, and every change of theirs already arrives as a
pull request: from a fork, or for a vulnerability, from its report's temporary private fork.

Moving the maintainer's own work to pull requests too would add a step to every change, for no reviewer: the
maintainer would approve their own pull requests.

## Decision

- **A contributor's change comes as a pull request,** following CONTRIBUTING.md's workflow, with the duties and
  checks as for any change. A vulnerability's fix comes in its report's temporary private fork, where CI does not
  run, so the contributor's full local check stands in for it and the pull request says which commit passed.
- **The maintainer reviews each one with its contributor,** and merges it.
- **The maintainer's own changes still go straight to `main`,** as ADR-0014's solo rule says.
- **No branch protection yet.** Only the maintainer can push, so it would guard nothing.

## Alternatives considered

- **Pull requests for everyone, as ADR-0014 says.** A pull request per change, approved by its own author, and branch
  protection that guards against no one else.
- **Contributors send patches some other way,** such as in the report. Loses the review tools and CI that a pull
  request brings.

## Consequences

- A contributor's work is reviewed before it reaches `main`. The maintainer's is not, as before, and the hook and the
  fresh review stay its gate.
- A pull request runs CI, which [ADR-0023](0023-ci-on-demand.md) already allows. One in a private fork runs none.
- **Revisit** when anyone else gets write access, or a real business adopts Tetsu: then pull requests and branch
  protection apply to everyone, as ADR-0014 planned.

## Sources

- [GitHub Docs: Collaborating in a temporary private fork to resolve a repository security vulnerability](https://docs.github.com/en/code-security/security-advisories/working-with-repository-security-advisories/collaborating-in-a-temporary-private-fork-to-resolve-a-repository-security-vulnerability)
- [GitHub Docs: About protected branches](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches)
