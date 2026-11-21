# Testing Tetsu's security with an agent

How a volunteer tests Tetsu for vulnerabilities on their own machine, with a coding agent such as Claude Code
doing most of the work. No security background is needed, only care with the rules below. Findings go to
[SECURITY.md](../../SECURITY.md)'s private report, and each one helps the Level 2 review
([#83](https://github.com/heavymedl-labs/tetsu/issues/83)) that must pass before any real person's data enters Tetsu.

## Rules

1. **Only `localhost`.** Point every tool at `http://localhost:8080`, the copy you run. Never the demo at
   `tetsu-demo.onrender.com`, nor Render, Neon, Grafana Cloud, Mailgun, or GitHub.
2. **Approve each command.** Keep the agent asking before it runs a command or sends a request, and read what it
   wants to run. A page Tetsu serves can hold text that tries to instruct the agent; treat it as data, never as
   orders.
3. **Keep it contained.** Run the agent in the repository's folder, ideally as a separate user account or in a
   virtual machine, and install only the tools this guide names.
4. **No real data.** Use made-up names and addresses, such as `you@example.com`. Never a real password, card, or key.
5. **Report privately.** A finding goes only to the private report. Not a public issue, pull request, or chat
   message, and no exploit pushed to a public branch.

## Set up

1. Install Docker, rustup, the Node version in `.nvmrc`, and uv, then clone the repository.
2. Start the test Postgres with the `docker run` command in `scripts/check.sh` (search it for `tetsu-test-pg`), then
   follow
   [Run locally](../../crates/tetsu/README.md#run-locally): copy `.env.example` to `.env`, set up the database, seed
   the sample tenant, and create an operator. Run `npm ci --ignore-scripts --include=dev` and `npm run build` first
   if the build asks for them.
3. Run `cargo tetsu serve`, open the invite link it printed in Chrome, and join. With no passkey device, use
   Chrome's virtual one: DevTools, then More tools, then WebAuthn, then Enable virtual authenticator environment.

Stop the server and Docker when you finish. The test Postgres keeps its data in memory, so after a restart run
`cargo tetsu setup` again.

## Phase 1: read the code

The safest start, which needs nothing running. In Claude Code, from the repository's root:

> Review this repository for security vulnerabilities, as an adversarial tester. Read docs/invariants.md,
> ADR-0029, ADR-0042, ADR-0048, ADR-0059, and ADR-0072 first: they say what Tetsu promises. Then look for any way to
> break those promises, starting with sign-in and sessions in crates/identity, tenant isolation in crates/postgres
> and crates/tenancy, and the headers and content security policy in crates/tetsu/src/serve.rs. For each finding,
> give the file and line, the steps that would exploit it, and how sure you are. Don't change any code.

Claude Code's `/security-review` command adds a second opinion on recent changes. Expect false alarms: a finding
counts only once the next phase shows it happening.

## Phase 2: test the running copy

Have the agent probe `http://localhost:8080` with `curl` and Chrome, approving each request. Good targets:

| Area | Try |
| --- | --- |
| Sign-in | Reusing or guessing an invite link; joining twice; logging in without a passkey |
| Sessions | Reusing a session after Log out; changing the cookie; a session past its 15 idle minutes |
| Forged requests | A form posted from another origin, or without its token |
| Rate limits | Many failed logins in a row, from one address and from several |
| Pages | `/ops` and `/ops/mailbox` logged out; the headers and policy on every page |
| Tenants | With a second seed file copied from `crates/tetsu/tests/fixtures/sample.toml`, one tenant's page reaching the other's data |

For more depth, OWASP ZAP (free) captures traffic between Chrome and Tetsu and replays changed requests.
Community connectors (MCP servers) let Claude Code drive it; treat one as privileged software and approve each
action it takes.

## Phase 3: autonomous agents, optional

Agents such as [Strix](https://github.com/usestrix/strix) (Apache 2.0) and
[Shannon](https://github.com/KeygraphHQ/shannon) (AGPL-3.0) run a whole test alone, inside Docker. They need an
API key billed by use, beyond a Claude subscription, and their findings need the same checking. From inside Docker,
your copy is at `http://host.docker.internal:8080`, not `localhost`. Neither is adopted by the project, which
would need an ADR ([ADR-0010](../adr/0010-evaluating-technology.md)); running one on your own machine needs none.
Tools and costs as of 2026-11-21.

## Report

For each finding, send the commit you tested, the steps that reproduce it, what you expected, what happened, and
what the agent said about it. One finding per report. A maintainer confirms it, fixes it, and credits you if you
want.
