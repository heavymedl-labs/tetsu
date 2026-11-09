# 0027. Secrets, revised

- Status: Proposed
- Date: 2026-11-08
- Supersedes [ADR-0024](0024-secrets.md)

## Context

[ADR-0024](0024-secrets.md) chose Kingfisher to scan for secrets. Two fresh reviews the same day, each
finding reproduced ([research](../research/security-checks.md#review-on-2026-11-08)), showed it does not
fit:

- **It misses the password forms our secrets take.** Kingfisher leaves out Betterleaks' `generic-password`
  and `generic-api-key` rules, and would not load them. It missed `PGPASSWORD=`, `password=` in a keyword
  connection string, `password = "..."`, and `SESSION_SECRET=`. Two of our four production secrets are
  database passwords.
- **Tuning did not fix it.** It misses a database URL in a `tests/` path at its default confidence. Its
  baseline fingerprints shift with unrelated edits. Scanning `.` reads unreachable local git objects.

Betterleaks v1.10.0, from the authors of Gitleaks, caught every planted form once our config added one
rule. It also passed the edge cases. On GitHub Free, environment secrets are only for public
repositories, so a secret cannot be fenced to one job by GitHub.

## Decision

- **Betterleaks scans for secrets,** pinned to v1.10.0. `scripts/install-tools.sh` downloads its release
  archive for the machine's platform, checks the SHA-256 written in the script, and installs it in
  `.tools/bin`. Local machines and CI run the same binary.
- **`.betterleaks.toml` extends the default rules** with two additions:
  - a `postgres-password` rule for `PGPASSWORD` and keyword connection strings
  - an allowlist matching only a secret that itself starts with `fake-`
- **Every scan passes `-c .betterleaks.toml --redact --ignore-gitleaks-allow`.** So findings never print
  the secret, and inline `betterleaks:allow` comments do nothing. Scans never pass `--validation`, so
  nothing leaves the machine.
- **It runs in three places:**
  - the pre-commit hook, on staged changes (`git --staged`)
  - the pre-push hook, on every commit being pushed, read from the refs git passes the hook
  - `scripts/check.sh`, on all history reachable from `HEAD`, so in CI too

  Any error fails the scan, as Betterleaks exits non-zero.
- **New fake credentials in tests start with `fake-`.** Keys and certificates that tests need are
  generated when the test runs, never committed.
- **`.betterleaksignore` lists findings by commit, file, rule, and line.** Commits never change, so an
  entry cannot drift. It holds only two kinds of entry, each under a comment saying why:
  - **Fake credentials committed before the scan was adopted,** reviewed once in
    [#81](https://github.com/heavymedl-labs/tetsu/issues/81).
  - **Rotated real secrets left in history,** with the date they were rotated.
- **The floor guard protects `.betterleaks.toml` and `.betterleaksignore`.** Any added line needs a
  `Floor-reason:` trailer recording the maintainer's approval.
- **A secret has leaked** once its value is anywhere outside its store. That includes a pushed commit, a
  CI log, an issue, and an agent's or chat's context. Rotate it at its provider first, then remove it and
  record its old commit in `.betterleaksignore`. Agents do not run commands that print a secret's value.
- **Workflows use secrets narrowly.** Secrets are repository-level on GitHub Free, so:
  - Only the deploy workflow references them, and only in the `env:` of the step that needs each one,
    never at job level.
  - That workflow runs only on `workflow_dispatch` or a push to `main`, never on `pull_request` or
    `workflow_run`.
  - The floor guard flags any `secrets` reference in another workflow, including `secrets: inherit` and
    `toJSON(secrets)`.
- **`docs/operations/secrets.md` lists every secret** by name, never by value. It records each one's
  store, scope, use, and the date it was last rotated. That includes the maintainer's local CLI keys for
  Render and Neon that [ADR-0012](0012-first-hosting-and-deploys.md) uses, and each workflow's
  `GITHUB_TOKEN` permissions. The first deploy ([#77](https://github.com/heavymedl-labs/tetsu/issues/77))
  creates it.
- **Secrets rotate** when someone with access leaves, after a leak, and otherwise yearly. `check.sh` fails
  when a date in that list is more than 13 months old.
- **When the scan lands** ([#81](https://github.com/heavymedl-labs/tetsu/issues/81)), CONSTRAINTS.md's
  note that only the fresh review covers secrets names it.

## Alternatives considered

- **Kingfisher.** Rust, and installable from PyPI, but it missed four of the six planted forms, and its
  rules could not be extended with Betterleaks'.
- **Kingfisher with our own rules for the forms it misses.** We would maintain detection for the most
  common secret type ourselves, in a format only Kingfisher reads.
- **Gitleaks.** The same lineage and config format, but feature complete, with security patches only.
  The fallback, since our config would carry over.
- **TruffleHog.** Strong at checking whether keys still work, which we turn off. AGPL-3.0 and heavier.
- **GitHub secret scanning.** Needs a paid Secret Protection plan on Team or Enterprise.

## Consequences

- A secret is stopped at commit, and at push for every commit pushed, including merges and rebases. CI
  checks again when it runs, but by then the secret is on GitHub and must be rotated.
- Writes made on GitHub itself skip both hooks: the web editor, MCP tools that write files, and merges.
  CI's history scan catches them on its next run.
- The scanner is Go, not Rust, a preference given up for detection. Upgrades are deliberate: v2, now a
  release candidate, changes its config and output.
- The `postgres-password` rule is ours, and the fresh review checks changes to it.
- Replacing it means swapping the command in the hooks and `check.sh`. Gitleaks reads the same config and
  ignore file.

**Weakest evidence.** Betterleaks started in 2026-02. Detection was tested once, with planted secrets in
the forms we expect, plus one rule we wrote. The release download is checked against its SHA-256, but its
Sigstore signature is not verified.

**Revisit** under [ADR-0011](0011-server-stack-guardrails.md)'s triggers, when Betterleaks v2 is stable,
or if it misses a planted secret in a form we use.

## Sources

- [Security checks research](../research/security-checks.md), 2026-11-08, including the reviews.
- [Betterleaks](https://github.com/betterleaks/betterleaks) README and v1.10.0 release, with
  `checksums.txt`.
- [Gitleaks README](https://github.com/gitleaks/gitleaks): feature complete, security patches only.
- [GitHub Docs: deployments and environments](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments):
  on GitHub Free, environment secrets are only for public repositories.
- [OWASP DevSecOps Guideline: secrets management](https://owasp.org/www-project-devsecops-guideline/latest/01a-Secrets-Management).
- [OWASP Secrets Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Secrets_Management_Cheat_Sheet.html):
  least privilege, rotation, and revoking a compromised secret.
