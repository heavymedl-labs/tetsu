# Security

Tetsu is early and holds no real person's data yet. Its bar, OWASP ASVS Level 2 before any real data, is in
[ADR-0029](docs/adr/0029-security-target-revised.md).

## Report a vulnerability privately

[Report it here](https://github.com/heavymedl-labs/tetsu/security/advisories/new), through GitHub's private
vulnerability reporting. Never in a public issue, pull request, or discussion: the repository is public
([ADR-0066](docs/adr/0066-public-repository.md)), so a public report shows everyone how to attack Tetsu.

Include what you tested (the commit), the steps that show the problem, what you expected, and what happened. A
report about code on `main` is the most useful kind.

A hardening idea, with nothing anyone could exploit, such as a missing header, can go straight to a public issue
instead, with a pull request if you like. When unsure which it is, report it privately.

## What happens next

A report can come with a proposed fix, in a pull request in the report's temporary private fork, as [testing with
an agent](docs/security/testing-with-an-agent.md#fix-it-optional) describes. Then the report and the fix are reviewed
together with you. Within a week, the maintainer answers on the report with one of these:

- **A vulnerability:** fixed privately in the report's own workspace, with your fix or the maintainer's, then
  published as an advisory, crediting you if you want. You are asked to check the fix.
- **A hardening idea,** reported privately: moved to a public issue, with your agreement, and the report closed.
- **Intended:** closed, with the reason, such as the ADR that decides it.
- **Already known:** closed, with a link to where it is tracked.

## Test only your own copy

Test Tetsu on your own machine, as [testing with an agent](docs/security/testing-with-an-agent.md) describes. Never
test the demo at `tetsu-demo.onrender.com` or the services it runs on (Render, Neon, Grafana Cloud, Mailgun,
GitHub): they are shared and run on free plans, and testing them needs their owners' permission, which no one has.
