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
5. **Report privately.** A finding goes only to the private report, and its fix only to that report's private
   fork. Not a public issue, pull request, or chat message. Keep ZAP's reports and any exploit outside the
   repository, and never push them.

## Set up, once

1. Install git, Docker, [rustup](https://rustup.rs), and Node at the version in `.nvmrc`, through a version manager
   such as mise or nvm. rustup installs the Rust version the repository pins by itself, on first build.
2. Clone `https://github.com/heavymedl-labs/tetsu` and work from its root.
3. Install the agent skills the project uses, as [CONTRIBUTING.md](../../CONTRIBUTING.md#agent-skills) describes.
4. Start the test Postgres. It keeps its data in memory and answers only on this machine:

   ```bash
   docker run -d --name tetsu-test-pg --restart unless-stopped -p 127.0.0.1:54329:5432 \
     -e POSTGRES_HOST_AUTH_METHOD=trust --tmpfs /var/lib/postgresql postgres:18.7 \
     -c fsync=off -c synchronous_commit=off -c full_page_writes=off -c ssl=on \
     -c ssl_cert_file=/etc/ssl/certs/ssl-cert-snakeoil.pem -c ssl_key_file=/etc/ssl/private/ssl-cert-snakeoil.key
   ```

   If it says the name is already in use, it exists: run `docker start tetsu-test-pg` instead.
5. Build the pages' styles and scripts: `npm ci --ignore-scripts --include=dev`, then `npm run build`.
6. Follow [Run locally](../../crates/tetsu/README.md#run-locally): copy `.env.example` to `.env`, set up the
   database, and seed the sample tenant.
7. Create your operator **yourself, in a terminal**, not through the agent: `cargo tetsu operator create "Avery
   Tester" avery@example.com`. It prints the invite link only to a terminal, and refuses when an agent runs it. The
   link works once, for an hour; keep it to yourself.
8. Run `cargo tetsu serve`, open the invite link in Chrome, and join with a passkey.
   With no passkey device, use Chrome's virtual one: in DevTools, open More tools, then WebAuthn, then Enable
   virtual authenticator environment. Leave DevTools open, since the virtual passkey lasts only while it is.

## Before each session

Tetsu changes often, so test what is current:

1. `git pull`.
2. Start Docker and the test Postgres if they are stopped: `docker start tetsu-test-pg`.
3. Rebuild the styles if `npm run build` or the server asks, then run `cargo tetsu setup`. It migrates, and creates
   the database again after the test Postgres restarts, since that keeps its data in memory.
4. Run `cargo tetsu operator list`. If it lists no operator, seed and create one again, as in set up. If yours has
   no passkey and its link has closed, get a new link in a terminal: `cargo tetsu operator invite <id>`.
5. `cargo tetsu serve`. Note the commit you test: `git rev-parse --short HEAD`.

When you finish, stop the server with Ctrl+C, or `kill -INT <its process id>` if the agent started it, and the test
Postgres with `docker stop tetsu-test-pg`.

## Phase 1: read the code

The safest start, which needs nothing running. In Claude Code, from the repository's root:

> Review this repository for security vulnerabilities, as an adversarial tester, using the
> security-and-hardening skill. Read docs/invariants.md,
> ADR-0029, ADR-0042, ADR-0048, ADR-0059, and ADR-0072 first: they say what Tetsu promises. Then look for any way to
> break those promises, starting with sign-in and sessions in crates/identity, tenant isolation in crates/postgres
> and crates/tenancy, and the headers and content security policy in crates/tetsu/src/serve.rs. For each finding,
> give the file and line, the steps that would exploit it, and how sure you are. Don't change any code.

Claude Code's `/security-review` command adds a second opinion on recent changes. Expect false alarms: a finding
counts only once Phase 2 shows it happening.

## Phase 2: attack the running copy

With the server running, ask the agent to work through these, approving each command.

**1. ZAP's scans.** [ZAP](https://www.zaproxy.org) is OWASP's free web scanner. Its Docker image runs a scan and
writes a report, here to a folder beside the repository. Its crawler finds little from the home page alone, so scan
each starting point: the home page, the operator login, and the sample tenant's page.

```bash
mkdir -p ../tetsu-zap && chmod a+w ../tetsu-zap
for start in home=/ login=/ops/login tenant=/sample-studio; do
  docker run --rm --name tetsu-zap --network host -v "$PWD/../tetsu-zap:/zap/wrk" zaproxy/zap-stable \
    zap-baseline.py -t "http://localhost:8080${start#*=}" -r "baseline-${start%%=*}.html"
done
```

- **The baseline scan only looks.** The first run downloads ZAP's image, about a minute. Each scan takes a minute
  or two.
- **`zap-full-scan.py` also attacks.** Run it the same way, with `-m 10` to cap its crawl at ten minutes; it takes a
  few minutes per starting point. `docker stop tetsu-zap` ends a scan early.
- **Exit code 2 means warnings to read, not a failure.** Exit code 1 means ZAP found a failure; 3 means it broke.
- **On macOS,** drop `--network host` and use `http://host.docker.internal:8080` as the target.
- **Logged in:** after joining, copy the `__Host-tetsu-operator` cookie's value from Chrome's DevTools (Application,
  then Cookies), and scan `http://localhost:8080/ops` with this added to the scan's options, with the value in place
  of `VALUE`. The cookie is a live session, so keep it out of reports you send, and log out afterwards.

  ```text
  -z "-config replacer.full_list(0).description=auth -config replacer.full_list(0).enabled=true
  -config replacer.full_list(0).matchtype=REQ_HEADER -config replacer.full_list(0).matchstr=Cookie
  -config replacer.full_list(0).regex=false -config replacer.full_list(0).replacement=__Host-tetsu-operator=VALUE"
  ```

**2. Checks of Tetsu's own promises.** Have the agent try each with `curl` and Chrome. The operator pages are at
`/ops`, `/ops/login`, `/ops/join`, and `/ops/mailbox`. Their requests go to `/ops/login/begin` and `/finish`,
`/ops/join/begin` and `/finish`, `/ops/passkeys/begin` and `/finish`, `/ops/logout`, and `/ops/session`. Each of
those takes JSON (`Content-Type: application/json`) from `Origin: http://localhost:8080`, and a logged-in one also
carries the session cookie and the page's `form` token.

| Area | Try |
| --- | --- |
| Sign-in | Reusing or guessing an invite link; joining twice; logging in without a passkey |
| Sessions | Reusing a session after Log out; changing the cookie; a session past its 15 idle minutes |
| Forged requests | A form posted from another origin, or without its token |
| Rate limits | Many failed logins in a row. Do this last: it can stop you joining or logging in for up to an hour |
| Pages | `/ops` and `/ops/mailbox` logged out; the headers and policy on every page |
| Tenants | With a second seed file copied from `crates/tetsu/tests/fixtures/sample.toml`, one tenant's page or theme showing the other's name, locations, or colors. Tenants hold only public data so far |

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

1. **Check it first.** Run `git pull`, start the server again, and reproduce the finding on the current code. Then
   have the agent try to disprove it with the doubt-driven-development skill. Report only what you saw happen, not
   what the agent only suspects.
2. **Report everything privately, even small things.** On the repository's Security tab, choose Report a
   vulnerability, and fill in one report per finding. A missing header goes there too: deciding what is harmless
   enough to make public is the maintainer's job, not yours.
3. **Say what happened.** The commit you tested, the steps that reproduce it, what you expected, what happened, and
   the agent's explanation in your own copy. Never paste a whole transcript: it may hold your session cookie or
   invite link.
4. **Fix it too, if you like,** as the next section says. Either way, [SECURITY.md](../../SECURITY.md#what-happens-next)
   says what the maintainer does, and when. Keep the finding to yourself until the report is closed.
5. **Check the fix.** When the maintainer says it is fixed, pull, repeat your steps, and reply on the report whether
   it is closed.

## Fix it, optional

Once you have reported a finding, you can propose its fix, privately, without waiting for an answer:

1. **Once per clone,** do the rest of [CONTRIBUTING.md](../../CONTRIBUTING.md#workflow)'s setup: install uv, run
   `scripts/install-tools.sh`, and run `git config core.hooksPath .githooks`. The hooks then run the linters before
   each commit and the full checks before each push.
2. **On the report, create a temporary private fork,** which you can as its reporter. Clone it, and branch from
   `main`, such as `fix/session-reuse`.
3. **Have the agent fix it the project's way.** Claude Code reads [AGENTS.md](../../AGENTS.md) by itself: a test that
   fails first, then the fix, `scripts/check.sh` passing, and a fresh review as [docs/review.md](../review.md) says.
   Sign-in, sessions, and tenant isolation are Tier 1: every line and branch tested, and no surviving mutants.
4. **Push the branch to the private fork,** and open one pull request into its `main`. CI does not run there, so
   your full check is the only one: say in the pull request that it passed, and on which commit.
5. **You and the maintainer review the report and the fix together.** The maintainer merges it from the report and
   publishes the advisory ([ADR-0076](../adr/0076-pull-requests-from-contributors.md)).

Never push the fix anywhere public, your own fork included, before the advisory is published. A hardening idea the
maintainer moves to a public issue follows [CONTRIBUTING.md](../../CONTRIBUTING.md#workflow)'s usual workflow
instead, where CI runs.
