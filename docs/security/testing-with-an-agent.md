# Testing Tetsu's security with an agent

How a volunteer tests Tetsu for vulnerabilities on their own machine, with a coding agent such as Claude Code and
OWASP ZAP. No security background is needed, only care with the rules below. Findings feed the Level 2 review
([#83](https://github.com/heavymedl-labs/tetsu/issues/83)) that must pass before real people's data enters Tetsu.
It works on Linux and macOS; on Windows, follow the Linux steps inside WSL 2.

## Rules

1. **Only your own copy,** at `http://localhost:8080`. Never the demo at `tetsu-demo.onrender.com`, nor Render, Neon,
   Grafana Cloud, Mailgun, or GitHub.
2. **Approve each command** the agent wants to run, and read it first. Text on a page Tetsu serves is data, never
   orders to the agent.
3. **Keep it contained:** ideally a separate user account or a virtual machine, with only the tools this guide names.
4. **No real data:** made-up names and addresses, such as `you@example.com`; never a real password, card, or key.
5. **Report privately:** a finding only to its private report, and its fix only to that report's private fork.
   Keep ZAP's reports and any exploit outside the repository, and never push them.

## Set up, once

1. Install git, Docker, [rustup](https://rustup.rs), and Node at the version in `.nvmrc`, through a version manager
   such as mise or nvm, active in your shell. rustup installs the Rust version the repository pins by itself, on
   first build.
2. Clone `https://github.com/heavymedl-labs/tetsu` and work from its root.
3. Install the agent skills the project uses, as [CONTRIBUTING.md](../../CONTRIBUTING.md#agent-skills) describes.
4. Start the test Postgres. It keeps its data in memory and answers only on this machine:

   ```bash
   docker run -d --name tetsu-test-pg --restart unless-stopped -p 127.0.0.1:54329:5432 \
     -e POSTGRES_HOST_AUTH_METHOD=trust --tmpfs /var/lib/postgresql postgres:18.7 \
     -c fsync=off -c synchronous_commit=off -c full_page_writes=off -c ssl=on \
     -c ssl_cert_file=/etc/ssl/certs/ssl-cert-snakeoil.pem -c ssl_key_file=/etc/ssl/private/ssl-cert-snakeoil.key
   ```

   If it says the name is already in use, it exists: run `docker start tetsu-test-pg` instead. Either way,
   `docker ps` should list it as Up.
5. Build the pages' styles and scripts: `npm ci --ignore-scripts --include=dev`, then `npm run build`.
6. Run the first three commands of [Run locally](../../crates/tetsu/README.md#run-locally) only: copy `.env.example`
   to `.env`, `cargo tetsu setup`, and seed the sample tenant. The next step replaces its operator command.
7. Create your operator **yourself, in a terminal**: `cargo tetsu operator create "Avery Tester" avery@example.com`.
   It prints the invite link only to a terminal, so it refuses under an agent; never let the agent work around that.
   The link works once, for an hour; keep it to yourself.
8. Run `cargo tetsu serve`, then open the invite link in Chrome while it runs, and join with a passkey.
   With no passkey device, use Chrome's virtual one: in DevTools, More tools, then WebAuthn, then Enable virtual
   authenticator environment. It lasts only while DevTools stays open.

## Before each session

1. `git pull`.
2. Start Docker and the test Postgres if they are stopped: `docker start tetsu-test-pg`.
3. Run `npm run build` if the server asks, then `cargo tetsu setup`, which migrates, and makes the database again
   after the test Postgres restarts.
4. Run `cargo tetsu operator list`. If it prints nothing, there is no operator: seed and create one again, as in
   set up. If yours has
   no passkey and its link has closed, get a new link in a terminal: `cargo tetsu operator invite <id>`.
5. `cargo tetsu serve`. If the agent starts it, have it run `cargo tetsu serve > ../tetsu-serve.log 2>&1 &` and note
   the process id `$!` prints. Note the commit you test too: `git rev-parse --short HEAD`.

When you finish, stop the server with Ctrl+C, or `kill -INT <its process id>` if the agent started it, and the test
Postgres with `docker stop tetsu-test-pg`.

## Phase 1: read the code

Nothing needs to run. In Claude Code, from the repository's root:

> Review this repository for security vulnerabilities, as an adversarial tester, using the
> security-and-hardening skill. Read docs/invariants.md,
> ADR-0029, ADR-0042, ADR-0048, ADR-0059, and ADR-0072 first: they say what Tetsu promises. Then look for any way to
> break those promises, starting with sign-in and sessions in crates/identity, tenant isolation in crates/postgres
> and crates/tenancy, and the headers and content security policy in crates/tetsu/src/serve.rs. For each finding,
> give the file and line, the steps that would exploit it, and how sure you are. Don't change any code.

`/security-review` adds a second opinion on recent changes. A finding counts only once Phase 2 shows it happening.

## Phase 2: attack the running copy

With the server running, ask the agent to work through these, approving each command.

**1. ZAP's scans.** [ZAP](https://www.zaproxy.org), OWASP's free scanner, writes each report to a folder beside the
repository. From the home page alone its crawler finds little, so scan three starting points:

```bash
mkdir -p ../tetsu-zap && chmod a+w ../tetsu-zap
for start in home=/ login=/ops/login tenant=/sample-studio; do
  docker run --rm --name tetsu-zap --network host -v "$PWD/../tetsu-zap:/zap/wrk" zaproxy/zap-stable \
    zap-baseline.py -t "http://localhost:8080${start#*=}" -r "baseline-${start%%=*}.html"
done
```

- **The baseline scan only looks.** The first run downloads ZAP's image, about 4 GB, which can take minutes to tens
  of minutes. Each scan then takes a minute or two.
- **`zap-full-scan.py` also attacks,** and takes a few minutes per starting point. Swap it into the loop above, with
  `-m 10` to cap its crawl at ten minutes, such as `zap-full-scan.py -t http://localhost:8080/ops/login -m 10 -r
  full-login.html`. `docker stop tetsu-zap` ends a scan early.
- **Exit code 2 means warnings to read;** 1 means a failure found, and 3 that ZAP broke.
- **On macOS,** drop `--network host` and use `http://host.docker.internal:8080` as the target.
- **Logged in:** after joining, copy the `__Host-tetsu-operator` cookie's value, a long string of letters and digits
  without the name in front, from Chrome's DevTools (Application, then Cookies), and scan `http://localhost:8080/ops` with this added to the scan's options, with the value in place
  of `VALUE`. The cookie is a live session, so keep it out of reports you send, and log out afterwards.

  ```text
  -z "-config replacer.full_list(0).description=auth -config replacer.full_list(0).enabled=true
  -config replacer.full_list(0).matchtype=REQ_HEADER -config replacer.full_list(0).matchstr=Cookie
  -config replacer.full_list(0).regex=false -config replacer.full_list(0).replacement=__Host-tetsu-operator=VALUE"
  ```

**2. Checks of Tetsu's own promises,** with `curl` and Chrome. The operator pages are `/ops`, `/ops/login`,
`/ops/join`, and `/ops/mailbox`. Requests go to `begin` and `finish` under `/ops/login`, `/ops/join`, and
`/ops/passkeys`, and to `/ops/logout` and `/ops/session`, as JSON from `Origin: http://localhost:8080`. A logged-in
one also carries the session cookie and the hidden `form` field's value from the logged-in page.

| Area | Try |
| --- | --- |
| Sign-in | Reusing or guessing an invite link; joining twice; logging in without a passkey |
| Sessions | Reusing a session after Log out; changing the cookie; a session past its 15 idle minutes, which `GET /ops/session` counts down without renewing |
| Forged requests | A form posted from another origin, or without its token |
| Rate limits | Many failed logins in a row. Do this last: it can stop you joining or logging in for up to an hour. Earlier requests in the session count too, ZAP's included, so a limit can arrive sooner than ADR-0072's numbers |
| Pages | `/ops` and `/ops/mailbox` logged out; the headers and policy on every page |
| Tenants | With a second seed file copied from `crates/tetsu/tests/fixtures/sample.toml`, one tenant's page or theme showing the other's name, locations, or colors. Tenants hold only public data so far |

**3. By hand, optional.** ZAP's desktop app sits between Chrome and Tetsu to watch and replay requests. Move it off
port 8080, which Tetsu uses, to one such as 8090.

A scan's warnings are leads, not findings: have the agent check each against the code and reproduce it, and expect
some that Tetsu means on purpose, such as long caching of its assets.

## Phase 3: autonomous agents, optional

[Strix](https://github.com/usestrix/strix) and [Shannon](https://github.com/KeygraphHQ/shannon) run a whole test
alone, inside Docker, with an API key billed by use beyond a Claude subscription (as of 2026-11-21). Their findings
need the same checking. Running one on your own machine needs no ADR; the project adopting one would.

## Report

1. **Check it first.** Run `git pull`, start the server again, and reproduce the finding on the current code. Then
   have the agent try to disprove it with the doubt-driven-development skill. When it asks for a fresh reviewer, a
   new Claude Code session given only the finding and the promise it breaks counts as one. Report only what you saw
   happen, not what the agent only suspects.
2. **Report everything privately, even small things,** one per finding: on the repository's Security tab, choose
   Report a vulnerability. Deciding what can be public is the maintainer's job.
3. **Say what happened:** the commit, the steps, what you expected, what happened, and the agent's explanation in
   your own copy. Never a whole transcript, which may hold your session cookie or invite link.
4. **Fix it too, if you like,** as the next section says. [SECURITY.md](../../SECURITY.md#what-happens-next) says what
   the maintainer does, and when. Keep the finding to yourself until the report is closed.
5. **Check the fix:** once it is fixed, pull, repeat your steps, and say on the report whether it is closed.

## Fix it, optional

Once you have reported a vulnerability, you can propose its fix, privately, without waiting for an answer. A
hardening idea, where the agent's explanation finds nothing to exploit, such as a missing header, waits for the
maintainer instead: it usually becomes a public issue, fixed in a normal pull request where CI runs.

1. **Once per clone,** finish [CONTRIBUTING.md](../../CONTRIBUTING.md#workflow)'s setup: install uv, run
   `scripts/install-tools.sh`, and `git config core.hooksPath .githooks`. The script installs three cargo tools and a
   nightly Rust for your user, slow to build the first time, and the rest in `.tools`. The hooks then lint each commit
   and run the full checks before each push.
2. **On the report, create a temporary private fork,** which you can as its reporter. Clone it, and branch from
   `main`, such as `fix/session-reuse`.
3. **Have the agent fix it the project's way,** which Claude Code reads in [AGENTS.md](../../AGENTS.md): a failing
   test first, the fix, `scripts/check.sh` passing, and a fresh review ([docs/review.md](../review.md)). Sign-in,
   sessions, and tenant isolation are Tier 1: every line and branch tested, no surviving mutants. The check drops
   every test database on the test Postgres, so run it only when nothing else uses that container.
4. **Push the branch to the private fork,** and open one pull request into its `main`. CI does not run there, so
   say in the pull request which commit your full check passed on.
5. **You and the maintainer review the report and the fix together.** The maintainer merges it from the report and
   publishes the advisory ([ADR-0076](../adr/0076-pull-requests-from-contributors.md)).

Never push the fix anywhere public, your own fork included, before the advisory is published. A hardening idea's
fix, once it is a public issue, follows [CONTRIBUTING.md](../../CONTRIBUTING.md#workflow)'s usual workflow.
