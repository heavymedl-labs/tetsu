# 0075. GitHub secret scanning

- Status: Proposed
- Date: 2026-11-21
- Supersedes [ADR-0027](0027-secrets-revised.md)'s rejection of GitHub secret scanning

## Context

[ADR-0027](0027-secrets-revised.md) chose Betterleaks for the hooks and CI, and turned GitHub secret scanning down
because it needed a paid Secret Protection plan. That held while the repository was private. Since
[ADR-0066](0066-public-repository.md) made it public, GitHub provides secret scanning and push protection on it for
free, though each must be turned on. On 2026-11-21 the repository's settings showed both off.

ADR-0027 names the gaps Betterleaks leaves. Writes made on GitHub itself, such as the web editor and tools that
write files through its API, skip the hooks, so CI's history scan finds a secret there only on its next run. A push
from anyone without the hooks set up skips them too, and the repository now has an outside contributor, testing
its security under [SECURITY.md](../../SECURITY.md).

## Decision

- **Turn on secret scanning and push protection** in the repository's settings. Push protection refuses a push,
  or a commit made on GitHub, that holds a secret in a format GitHub knows, such as a provider's API key.
- **Leave non-provider patterns and validity checks off.** Betterleaks already covers generic passwords, with our
  own rules, and ADR-0027 turned off checking whether a found key still works.
- **Betterleaks stays** as ADR-0027 decides, in the hooks and CI. GitHub's scanning is a second layer for what
  reaches GitHub without them.
- **A blocked push is fixed, not bypassed.** Remove the secret and rotate it. A bypass is for a finding that is
  certainly not a secret, such as a published test value, and its reason says so.

## Alternatives considered

- **Keep Betterleaks alone, as now.** It misses writes on GitHub until CI's next run, and any push without hooks.
- **Turn on non-provider patterns too.** Generic matches overlap Betterleaks' own rules and add alerts to triage
  for no new coverage.

## Consequences

- A known-format secret is stopped before it lands on GitHub, whoever pushes and however. Alerts and every bypass
  appear under the repository's Security tab, which only admins see.
- Betterleaks remains the only check for our database passwords, which match no provider's format.
- A test value in a provider's format can block a push. Keep test values out of real formats, as tests do now.
- Nothing to install or maintain, and no data reaches a new party: GitHub already holds the code.
- If the repository goes private again, these features need the paid plan, and this decision is revisited.

**Weakest evidence.** GitHub's docs, read on 2026-11-21, are the only source for what push protection covers. No
secret has been pushed to test it.

## Sources

- [GitHub Docs: Push protection](https://docs.github.com/en/code-security/concepts/secret-security/push-protection)
- [GitHub Docs: Enabling push protection for your repository](https://docs.github.com/en/code-security/secret-scanning/enabling-secret-scanning-features/enabling-push-protection-for-your-repository)
- [GitHub Docs: Enabling secret scanning for your repository](https://docs.github.com/en/code-security/how-tos/secure-your-secrets/detect-secret-leaks/enabling-secret-scanning-for-your-repository)
- [GitHub Blog: Push protection is generally available, and free for all public repositories](https://github.blog/news-insights/product-news/push-protection-is-generally-available-and-free-for-all-public-repositories/)
