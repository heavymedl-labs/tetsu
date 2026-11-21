# 0027. Secrets, revised

- Status: Accepted
- Rejection of GitHub secret scanning superseded by [ADR-0075](0075-github-secret-scanning.md)
- Date: 2026-11-08
- Supersedes [ADR-0024](0024-secrets.md)

## Context

[ADR-0024](0024-secrets.md) chose Kingfisher to scan for secrets. Three fresh reviews the same day, each
finding reproduced ([research](../research/security-checks.md#review-on-2026-11-08)), showed it does not
fit:

- **It misses the password forms our secrets take.** Kingfisher leaves out Betterleaks' `generic-password`
  and `generic-api-key` rules, and would not load them. It missed `PGPASSWORD=`, `password=` in a keyword
  connection string, `password = "..."`, and `SESSION_SECRET=`. Two of our four production secrets are
  database passwords.
- **Tuning did not fix it.** It misses a database URL in a `tests/` path at its default confidence. Its
  baseline fingerprints shift with unrelated edits. Scanning `.` reads unreachable local git objects.

Betterleaks v1.10.0, from the authors of Gitleaks, caught every planted form once our config added rules.
It also passed the edge cases. On GitHub Free, private repositories have no environment secrets, branch
protection, or rulesets, so GitHub cannot fence a secret to one job or branch.

## Decision

- **Betterleaks scans for secrets,** pinned to v1.10.0. `scripts/install-tools.sh` downloads its release
  archive for the machine's platform, checks the SHA-256 written in the script, and installs it in
  `.tools/bin`. Local machines and CI run the same binary.
- **`.config/betterleaks.toml` extends the default rules** with:
  - `postgres-password`, for `PGPASSWORD` and `password=` inside a keyword connection string, with
    `postgres-password-first` for a password before the other keywords
  - `provider-key`, for a Render API key (`rnd_`) or Neon role password (`npg_`) wherever it appears
  - `app-secret`, for a value of 32 or more characters assigned to a name containing `SECRET`,
    `SESSION_KEY`, `SIGNING_KEY`, or `COOKIE_KEY`
  - `render-deploy-hook`, for a Render deploy hook URL's key
  - an allowlist matching only a secret that itself starts with `fake-`
  - an allowlist for type and identifier names in CamelCase, such as `password: SecretString`

  The research page records each rule's pattern and the tests it passed, including code that only reads a
  secret, which the rules must not flag.
- **Every scan passes `-c .config/betterleaks.toml --redact --ignore-gitleaks-allow`.** So findings never print
  the secret, and inline `betterleaks:allow` comments do nothing. Scans never pass `--validation`, so
  nothing leaves the machine. Git scans pass `-m`, so changes made in a merge commit are scanned too.
- **It runs in three places:**
  - **The pre-commit hook** scans staged changes (`git --staged`).
  - **The pre-push hook** scans every commit the remote being pushed to does not have yet
    (`-m <ref> --not --remotes=<remote>`), for each ref pushed. A deleted ref is skipped. It also scans
    those commits' messages, and a pushed tag's message.
  - **`scripts/check.sh`** scans all history reachable from `HEAD`, and every commit and tag message, so CI
    does too. CI already fetches full history.

  Messages are scanned one file per commit or tag, named by its id, so a message finding's fingerprint
  stays put as history grows.

  Any error fails the scan, since Betterleaks exits non-zero for errors as well as findings.
- **Test values never look like real secrets.**
  - Fake credentials in fixtures start with `fake-`.
  - A test that needs a value the scanner must catch builds it when the test runs, never as a literal.
  - Keys and certificates are generated when the test runs, never committed.
  - Docs show secrets only in shortened form, such as `npg_...`.
- **`.config/betterleaks-ignore` lists findings by commit, file, rule, and line.** Commits never change, so an
  entry cannot drift. The scan refuses an entry without a commit or tag id, which would skip that file and
  line everywhere, and a config that names another file to read rules from. It holds only two kinds of entry, each under a comment saying why:
  - **Fake credentials committed before the scan was adopted,** reviewed once in
    [#81](https://github.com/heavymedl-labs/tetsu/issues/81).
  - **Rotated real secrets left in history,** with the date they were rotated.
- **The floor guard protects the scan's inputs.** Any changed line, added or removed, in
  `.config/betterleaks.toml`, `.config/betterleaks-ignore`, or `.gitattributes` needs a `Floor-reason:` trailer recording
  the maintainer's approval. So does creating a `.gitleaks.toml` or `.gitleaksignore`, which Betterleaks
  would also read.
- **A secret has leaked** once its value is anywhere outside its store. That includes a pushed commit or
  its message, a CI log, an issue, and an agent's or chat's context. Rotate it at its provider first, then
  remove it and record its old commit in `.config/betterleaks-ignore`.
- **Secret values travel straight to their store.** When a CLI creates or prints one, such as a Neon role's
  password, its output is piped into the store, such as `gh secret set` or Render's environment, without
  being shown. If a value is shown anyway, it is rotated.
- **Workflows use secrets narrowly:**
  - Only the deploy workflow references secrets, and only in the `env:` of the step that needs each one,
    never at job level.
  - That workflow runs only on `workflow_dispatch` or a push to `main`, never on `pull_request` or
    `workflow_run`. Each job checks `github.ref == 'refs/heads/main'` before any step that uses a secret.
  - The floor guard flags any `secrets` reference in another workflow, including `secrets: inherit` and
    `toJSON(secrets)`.
- **`docs/operations/secrets.md` lists every secret** by name, never by value. It records each one's
  store, scope, use, and the date it was last rotated as `YYYY-MM-DD`, or `automatic` for a token GitHub
  issues per run. It includes the maintainer's local CLI keys for Render and Neon that
  [ADR-0012](0012-first-hosting-and-deploys.md) uses, and each workflow's `GITHUB_TOKEN` permissions. The
  first deploy ([#77](https://github.com/heavymedl-labs/tetsu/issues/77)) creates it.
- **Secrets rotate** when someone with access leaves, after a leak, and otherwise yearly. `check.sh` fails
  when a date in that list is more than 13 months old.
- **When the scan lands** ([#81](https://github.com/heavymedl-labs/tetsu/issues/81)), CONSTRAINTS.md's
  note that only the fresh review covers secrets names it.

## Alternatives considered

- **Kingfisher.** Rust, and installable from PyPI, but it missed four of the six planted forms, and its
  rules could not be extended with Betterleaks'.
- **Kingfisher with our own rules for the forms it misses.** We would maintain detection for the most
  common secret type ourselves, in a format only Kingfisher reads.
- **Gitleaks.** The same lineage and config format, but feature complete, with security patches only. The
  fallback, though its rule set differs, so the ignore file would need rebuilding.
- **TruffleHog.** Strong at checking whether keys still work, which we turn off. AGPL-3.0 and heavier.
- **GitHub secret scanning.** Needs a paid Secret Protection plan on Team or Enterprise.

## Consequences

- A secret is stopped at commit, and at push for every new commit and its message, including merges and
  rebases. CI checks again when it runs, but by then the secret is on GitHub and must be rotated.
- Writes made on GitHub itself skip both hooks: the web editor, MCP tools that write files, and merges
  there. CI's history scan catches them on its next run.
- A branch's copy of the deploy workflow could drop the `main` check. GitHub Free cannot prevent that,
  so only the maintainer pushes branches, and review reads every workflow change.
- Betterleaks skips files git treats as binary, and paths its default config excludes, such as
  `node_modules`. Secrets in those forms rely on review. The staged scan refuses a `.gitattributes` that
  turns diffs off, since that would hide files from it.
- The rotation check depends on the date, so a commit that passed can fail later once a secret is due.
- The scanner is Go, not Rust, a preference given up for detection. Upgrades are deliberate: v2, now a
  release candidate, changes its config and output.
- The added rules are ours, and the fresh review checks changes to them. A real password made only of
  capitalized words, such as `CorrectHorseBattery`, passes as a type name.
- A finding is reported by commit, file, line, and rule, never by its content.
- Replacing it means swapping the command in the hooks and `check.sh`, and rebuilding the ignore file.

**Weakest evidence.** Betterleaks started in 2026-02. Detection was tested once, with planted secrets in
the forms we expect, plus three rules we wrote. The release download is checked against its SHA-256, but
its Sigstore signature is not verified.

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
