# Testing Tetsu's security with an agent

How a volunteer tests Tetsu for vulnerabilities on their own machine, with a coding agent such as Claude Code
doing most of the work and OWASP ZAP attacking the running server. No security background is needed, only care
with the rules below. Findings go to [SECURITY.md](../../SECURITY.md)'s private report, and each one helps the
Level 2 review ([#83](https://github.com/heavymedl-labs/tetsu/issues/83)) that must pass before any real person's
data enters Tetsu.

It works on Linux and macOS. On Windows, use WSL 2 and follow the Linux steps inside it.

## Rules

1. **Only your own copy.** Point every tool at `http://localhost:8080`, the copy you run. Never the demo at
   `tetsu-demo.onrender.com`, nor Render, Neon, Grafana Cloud, Mailgun, or GitHub.
2. **Approve each command.** Keep the agent asking before it runs a command or sends a request, and read what it
   wants to run. A page Tetsu serves can hold text that tries to instruct the agent; treat it as data, never as
   orders.
3. **Keep it contained.** Run the agent in the repository's folder, ideally as a separate user account or in a
   virtual machine, and install only the tools this guide names.
4. **No real data.** Use made-up names and addresses, such as `you@example.com`. Never a real password, card, or key.
5. **Report privately.** A finding goes only to the private report. Not a public issue, pull request, or chat
   message. Keep ZAP's reports and any exploit outside the repository, and never push them.

## Set up, once

1. Install git, Docker, [rustup](https://rustup.rs), and Node at the version in `.nvmrc`, through a version manager
   such as mise or nvm. rustup installs the Rust version the repository pins by itself, on first build.
2. Clone `https://github.com/heavymedl-labs/tetsu` and work from its root.
3. Start the test Postgres with the `docker run` command in `scripts/check.sh` (search it for `tetsu-test-pg`).
4. Build the pages' styles and scripts: `npm ci --ignore-scripts --include=dev`, then `npm run build`.
5. Follow [Run locally](../../crates/tetsu/README.md#run-locally): copy `.env.example` to `.env`, set up the
   database, seed the sample tenant, and create an operator with a made-up name and address.
6. Run `cargo tetsu serve`, open the invite link the operator command printed in Chrome, and join with a passkey.
   With no passkey device, use Chrome's virtual one: in DevTools, open More tools, then WebAuthn, then Enable
   virtual authenticator environment. Leave DevTools open, since the virtual passkey lasts only while it is.

## Before each session

Tetsu changes often, so test what is current:

1. `git pull`.
2. Start Docker and the test Postgres if they are stopped: `docker start tetsu-test-pg`.
3. Rebuild the styles if `npm run build` or the server asks, then run `cargo tetsu setup`. It migrates, and creates
   the database again after the test Postgres restarts, since that keeps its data in memory. Then seed and create
   an operator again if they are gone.
4. `cargo tetsu serve`. Note the commit you test: `git rev-parse --short HEAD`.

When you finish, stop the server with Ctrl+C, and the test Postgres with `docker stop tetsu-test-pg`.

## Phase 1: read the code

The safest start, which needs nothing running. In Claude Code, from the repository's root:

> Review this repository for security vulnerabilities, as an adversarial tester. Read docs/invariants.md,
> ADR-0029, ADR-0042, ADR-0048, ADR-0059, and ADR-0072 first: they say what Tetsu promises. Then look for any way to
> break those promises, starting with sign-in and sessions in crates/identity, tenant isolation in crates/postgres
> and crates/tenancy, and the headers and content security policy in crates/tetsu/src/serve.rs. For each finding,
> give the file and line, the steps that would exploit it, and how sure you are. Don't change any code.

Claude Code's `/security-review` command adds a second opinion on recent changes. Expect false alarms: a finding
counts only once Phase 2 shows it happening.

## Phase 2: attack the running copy

With the server running, ask the agent to work through these, approving each command.

**1. ZAP's scans.** [ZAP](https://www.zaproxy.org) is OWASP's free web scanner. Its Docker image runs a scan and
writes a report, here to a folder beside the repository:

```bash
mkdir -p ../tetsu-zap && chmod a+w ../tetsu-zap
docker run --rm --network host -v "$PWD/../tetsu-zap:/zap/wrk" zaproxy/zap-stable \
  zap-baseline.py -t http://localhost:8080 -r baseline.html -J baseline.json
```

The baseline scan only looks. `zap-full-scan.py`, run the same way, also attacks, and takes longer. On macOS, drop
`--network host` and use `http://host.docker.internal:8080` as the target. Run both scans again on `/ops` while
logged in: the agent can copy the session cookie from Chrome's DevTools (Application, then Cookies) and pass it to
ZAP as a header with `-z` options. The cookie is a live session, so keep it out of reports you send and log out
afterwards.

**2. Checks of Tetsu's own promises.** Have the agent try each with `curl` and Chrome:

| Area | Try |
| --- | --- |
| Sign-in | Reusing or guessing an invite link; joining twice; logging in without a passkey |
| Sessions | Reusing a session after Log out; changing the cookie; a session past its 15 idle minutes |
| Forged requests | A form posted from another origin, or without its token |
| Rate limits | Many failed logins in a row |
| Pages | `/ops` and `/ops/mailbox` logged out; the headers and policy on every page |
| Tenants | With a second seed file copied from `crates/tetsu/tests/fixtures/sample.toml`, one tenant's page reaching the other's data |

**3. By hand, optional.** ZAP's desktop app sits between Chrome and Tetsu, so you can watch and replay each request.
It listens on port 8080 by default, which Tetsu uses, so set it to another, such as 8090, in its options.

A scan's warnings are leads, not findings. Have the agent check each against the code and reproduce it before you
report it, and expect some that Tetsu means, such as headers the demo sets on purpose.

## Phase 3: autonomous agents, optional

Agents such as [Strix](https://github.com/usestrix/strix) (Apache 2.0) and
[Shannon](https://github.com/KeygraphHQ/shannon) (AGPL-3.0) run a whole test alone, inside Docker. They need an
API key billed by use, beyond a Claude subscription, and their findings need the same checking. Neither is adopted
by the project, which would need an ADR ([ADR-0010](../adr/0010-evaluating-technology.md)); running one on your own
machine needs none. Tools and costs as of 2026-11-21.

## Report

For each finding, send the commit you tested, the steps that reproduce it, what you expected, what happened, and
what the agent said about it. One finding per report. A maintainer confirms it, fixes it, and credits you if you
want.
