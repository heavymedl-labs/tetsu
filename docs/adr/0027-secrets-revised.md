# 0027. Secrets, revised

- Status: Proposed
- Date: 2026-11-08
- Supersedes [ADR-0024](0024-secrets.md)

## Context

[ADR-0024](0024-secrets.md) chose Kingfisher to scan for secrets. A fresh review the same day, with
the failures reproduced, found it would not work as written
([research](../research/security-checks.md#review-on-2026-11-08)):

- At the default confidence, Kingfisher misses a real database URL inside a `tests/` path.
- Scanning `.` reads unreachable local git objects and ignored files. So pushes fail on the
  maintainer's machine while a clean CI checkout passes.
- A rotated secret left in history fails every scan, but the baseline allowed only fake credentials.
- Baseline fingerprints include file offsets, so they break on unrelated edits. Inline
  `kingfisher:ignore` comments silence any finding.
- GitHub Free has no environment secrets for private repositories, so a secret cannot be limited to
  one job.

The tool still fits: it is Rust and free, carries Betterleaks' rules, and runs offline. Its PyPI
wheel is the same release prebuilt, with 67,000 downloads last month.

## Decision

- **Kingfisher scans for secrets,** pinned as the PyPI package `kingfisher-bin` in
  `scripts/install-tools.sh`. It installs through uv into `.tools`, like shellcheck and shfmt, so local
  machines and CI run the same binary.
- **Every scan passes these flags:**
  - `--no-validate --no-update-check`, so nothing leaves the machine.
  - `--confidence low`, so test paths are not downgraded out of the report.
  - `--no-ignore`, so inline comments cannot silence a finding.
  - `--skip-regex '^fake-[a-z0-9-]+$'`.
- **It runs in two places:** on staged changes in the pre-commit hook, and on the history reachable from
  `HEAD` (`--branch HEAD`) in `scripts/check.sh`, so in the pre-push hook and CI. The pre-push hook is
  the gate; CI runs after the push.
- **Fake credentials in tests start with `fake-`,** such as `postgres://tetsu:fake-pw@...`. The pattern
  matches only the secret itself, so a real password next to a `fake-` word is still caught.
- **The baseline holds only rotated real secrets** that remain in history, each added with the
  maintainer's approval and the date it was rotated. The floor guard protects
  `.kingfisher-baseline.yml`, and treats `kingfisher:ignore` as a suppression.
- **A secret has leaked** once its value is anywhere outside its store: a commit, a CI log, an issue, or
  an agent's or chat's context. Rotate it at its provider first, then remove it. Rewriting history is
  optional, since the old value no longer works and the baseline records it.
- **Each secret has one use and the narrowest scope** its provider allows. Secrets are repository-level
  on GitHub Free, so only the deploy workflow references them, and that workflow never runs on
  `pull_request`. Review checks every new `secrets.` reference.
- **`docs/operations/secrets.md` lists every secret** by name: its store, scope, what uses it, and when
  it was last rotated. That includes the maintainer's local CLI keys for Render and Neon that
  [ADR-0012](0012-first-hosting-and-deploys.md) uses. It never holds a value. The first deploy
  ([#77](https://github.com/heavymedl-labs/tetsu/issues/77)) creates it.
- **Secrets rotate** when someone with access leaves, after a leak, and otherwise yearly, the date read
  from that list.
- **When the scan lands** ([#81](https://github.com/heavymedl-labs/tetsu/issues/81)), CONSTRAINTS.md's
  note that only the fresh review covers secrets names it.

## Alternatives considered

- **ADR-0024 as written.** Missed test-path secrets and diverged between local and CI.
- **A committed baseline for test fixtures.** Fingerprints shift with every edit above a fixture.
- **A skip word instead of a pattern.** Tested: `--skip-word fake-` also hid a real password when `fake-`
  appeared elsewhere in the URL.
- **`cargo install` or a release download in CI.** The build takes about 9 minutes and 3.7 GB. A download
  needs its own checksum step. The wheel avoids both.
- **Betterleaks.** Its rules already ship inside Kingfisher. Written in Go, and v2 is a release
  candidate. The fallback.
- **Gitleaks, TruffleHog, ripsecrets, GitHub secret scanning.** As in ADR-0024. GitHub's Secret
  Protection also needs a Team or Enterprise plan.

## Consequences

- A secret is stopped at commit, and at push if the commit hook was skipped. CI checks again later, but
  by then the secret is on GitHub, so it must be rotated.
- Merge commits skip the pre-commit hook, so pre-push catches them.
- Existing fixtures (`s3cret-pw`, `hunter2`, `mysslmodepw`, `secret`, and `sslmode` used as a password)
  are renamed to the `fake-` form in #81.
- Kingfisher upgrades follow its releases, roughly weekly. We take them on purpose, not automatically.
- Replacing it means swapping one command in `check.sh` and one line in `install-tools.sh`.

**Weakest evidence.** Few projects run Kingfisher in CI: 23 to 43 workflow files, against about 59,000
for Gitleaks. One MongoDB engineer writes most of it. Detection was tested on planted secrets in common
formats, and one outside test had it find 9 of 10.

**Revisit** under [ADR-0011](0011-server-stack-guardrails.md)'s triggers. Also revisit if Kingfisher
stops syncing Betterleaks' rules, misses a planted secret in a format we use, or Betterleaks v2 is
stable and in wide use.

## Sources

- [Security checks research](../research/security-checks.md), 2026-11-08, including the review.
- [Kingfisher](https://github.com/mongodb/kingfisher) README, usage guide, and changelog, v2.17.0;
  [kingfisher-bin on PyPI](https://pypi.org/project/kingfisher-bin/).
- [GitHub Docs: deployments and environments](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments):
  on GitHub Free, environment secrets are only for public repositories.
- [OWASP DevSecOps Guideline: secrets management](https://owasp.org/www-project-devsecops-guideline/latest/01a-Secrets-Management).
- [OWASP Secrets Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Secrets_Management_Cheat_Sheet.html):
  least privilege, rotation, and revoking a compromised secret.
