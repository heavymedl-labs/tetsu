# Demo environment

The one environment of the free phase ([ADR-0012](../adr/0012-first-hosting-and-deploys.md)): fictional tenants
only, at <https://tetsu-demo.onrender.com>. Set up by hand on 2026-11-09; check it with `render services` and
`neonctl projects get square-tree-12604600`. Its secrets are listed in [secrets.md](secrets.md).

## Deploying

Run CI on `main`, then deploy the commit it passed ([ADR-0033](../adr/0033-deploys-on-demand.md)):

```bash
gh workflow run CI
gh workflow run deploy                                 # once CI has passed on main's head
gh workflow run deploy -f rollback_to=<older commit>   # deploys it without migrating
```

Every deploy that migrates then runs hosted test 8, `tetsu check`, as `tetsu_owner`. It checks the server version,
every catalog rule, `tetsu_app` included, and that each module's role sees no row with no tenant set. A problem
fails the deploy before Render serves the new image, and the log names it.

Once Render serves the image and the deploy is recorded, `scripts/smoke_check.py` fetches the home page and a missing
page, waiting out the free instance's wake, and checks each status and security header. A failure names the page and
what was wrong: roll back to the previous commit, then fix forward. Run it against `tetsu serve` locally the same way,
with the local address.

A rollback cannot cross a migration that takes rights away. Code from before
[#106](https://github.com/heavymedl-labs/tetsu/issues/106) reads Tenancy's tables as `tetsu_app` directly, which
Tenancy's second migration forbids, so every request it served would fail; fix forward instead. The same is true of
the old instance in the minute between the Migrate step and Render's switch to the new one.

Every deploy that migrates then seeds the demo businesses from the private `tetsu-demos` repository, with the image
it deploys, as `tetsu_app` ([ADR-0062](../adr/0062-seeding-from-the-deploy.md)). Seeding skips businesses that exist,
so a deploy that resets the demo leaves it complete. To add a business, add its file to `tetsu-demos` and run a
deploy.

## Neon

Project `tetsu-demo` (`square-tree-12604600`) in `aws-us-east-2`, Postgres 18, on the free plan. Database `tetsu`
on the direct endpoint, never the pooler.

Roles follow [ADR-0042](../adr/0042-tenant-isolation.md): Tetsu makes its own in SQL, and Neon's own roles, which
skip row-level security, touch no tenant data.

- **`tetsu_migrate`** was made by Neon, so it belongs to `neon_superuser` and holds `BYPASSRLS`. It only creates
  Tetsu's roles.
- **`tetsu_owner`** owns the database and every module's schema and tables and runs migrations, which make each module's
  `tetsu_<schema>` and `tetsu_<schema>_lookup` roles. It was created in SQL, with `CREATEROLE` and nothing else.
- **`tetsu_app`** is what `tetsu serve` uses. It was created in SQL, holds no rights of its own beyond `CONNECT`,
  and acts as one module's role per transaction.

Each password was generated and set with psql's `\password`, which sends only a hash, then written only into its
store in [secrets.md](secrets.md).

### Moving onto Tetsu's own roles

Done once, before deploying the release that adds Tenancy's second migration
([#106](https://github.com/heavymedl-labs/tetsu/issues/106)). Until then `tetsu_migrate` ran migrations, and
`tetsu_app` held rights on every table through default privileges. Done 2026-11-12. In one terminal, make a
password and Neon's URL as shell variables, then connect as `tetsu_migrate` with psql, passing the password as a psql
variable, so it is never typed, shown, or kept in Neon's SQL editor history:

```bash
pw="$(openssl rand -hex 32)"
base="$(neonctl connection-string --project-id square-tree-12604600 --role-name tetsu_migrate --database-name tetsu)"
psql "$base" -v pw="$pw"
```

```sql
BEGIN;
CREATE ROLE tetsu_owner LOGIN CREATEROLE;
ALTER ROLE tetsu_owner PASSWORD :'pw';
REVOKE TEMPORARY ON DATABASE tetsu FROM PUBLIC;
GRANT tetsu_owner TO tetsu_migrate WITH SET TRUE;
REASSIGN OWNED BY tetsu_migrate TO tetsu_owner;
ALTER DEFAULT PRIVILEGES FOR ROLE tetsu_migrate REVOKE USAGE ON SCHEMAS FROM tetsu_app;
ALTER DEFAULT PRIVILEGES FOR ROLE tetsu_migrate REVOKE SELECT, INSERT, UPDATE, DELETE ON TABLES FROM tetsu_app;
ALTER DEFAULT PRIVILEGES FOR ROLE tetsu_migrate REVOKE USAGE, SELECT ON SEQUENCES FROM tetsu_app;
REVOKE tetsu_owner FROM tetsu_migrate;
COMMIT;
```

Neon accepts only plaintext passwords, since its control plane manages them, so psql's `\password`, which sends a
hash, fails at `COMMIT`. `REVOKE TEMPORARY` comes before the reassign, while `tetsu_migrate` still owns the database.
`tetsu_migrate` keeps the ADMIN right on `tetsu_owner` that Postgres gives whichever role creates another, so it could
act as the owner again. It is Neon's administrative role and reaches every row regardless, so this is accepted
rather than worked around.

Then, in the same terminal, put the owner's URL in GitHub without showing it, and add a row for it to
[secrets.md](secrets.md):

```bash
printf '%s' "$base" \
  | sed -E "s#//tetsu_migrate:[^@]*@#//tetsu_owner:${pw}@#; s/([?&])sslmode=[^&]*&?/\1/; s/[?&]\$//" \
  | gh secret set MIGRATION_DATABASE_URL
```

Run CI and deploy, which migrates as `tetsu_owner`. Tenancy's second migration takes back `tetsu_app`'s rights on its
tables, sequence, and record. The deploy then runs hosted test 8, `tetsu check`, as `tetsu_owner`, as it does after
every migrate. No copy of the password is kept beyond the secret; to set it again later, set a fresh one the same way.

Connection strings drop Neon's `sslmode`, which `tetsu` refuses since TLS is always required, and keep
`channel_binding=require`. Neon's certificate is public, so no `DATABASE_CA_FILE` is needed.

## Render

Web service `tetsu-demo` (`srv-dbp542rbc2fs73962k4g`) in Ohio, on the free instance, from the private image
`ghcr.io/heavymedl-labs/tetsu` by digest, pulled with the `ghcr-tetsu` registry credential. Auto-deploy is off.

- **Command:** `/usr/local/bin/tetsu serve`. Render's command replaces the image's entrypoint too, so `serve`
  alone exits with status 128.
- **Environment:** `TETSU_DEMO=true`, `TETSU_ORIGIN=https://tetsu-demo.onrender.com`, which the operator
  pages' passkeys belong to, `TETSU_TRUSTED_PROXIES=3`, the proxies measured in front of the service
  ([ADR-0072](../adr/0072-rate-limits-by-network-address.md)), and `DATABASE_URL`, and the export settings under
  [Logs and alerts](#logs-and-alerts). Render sets `PORT`.
- **Health check:** `/`.
- **Free instance:** sleeps after 15 minutes without traffic. Waking takes about 12 seconds, Neon's included, and
  once in 50 tries took 8 minutes ([test 9](../research/hosting.md#first-deploy-2026-11-09)).

The GitHub repository variable `RENDER_SERVICE_ID` names the service for `deploy.yml`.

## Logs and alerts

Render keeps standard output, and Grafana Cloud's free stack `regalchicken947` (`prod-us-west-0`, instance
`1922014`, set up 2026-11-13) receives the same lines over OTLP ([ADR-0049](../adr/0049-log-export.md)). Its trial
runs to 2026-11-27; with no card on file it then stays on the free plan. Export is set on the Render service, with these
[settings](../../crates/tetsu/README.md#settings):

- `OTEL_EXPORTER_OTLP_ENDPOINT`: `https://otlp-gateway-prod-us-west-0.grafana.net/otlp`, from the stack's
  OpenTelemetry tile.
- `OTEL_EXPORTER_OTLP_HEADERS`: `Authorization=Basic%20<base64 of 1922014:token>`, as a secret
  ([secrets.md](secrets.md)). The token is `render-tetsu-demo`, on access policy `tetsu-demo-logs`, which holds
  only `logs:write` on this stack, so a leaked key can write logs and nothing else. Build the value without the
  token touching history or chat:

  ```bash
   export GC_TOKEN='<token>'
  printf 'Authorization=Basic%%20%s\n' "$(printf '1922014:%s' "$GC_TOKEN" | base64 -w0)"; unset GC_TOKEN
  ```

- `OTEL_RESOURCE_ATTRIBUTES`: `deployment.environment.name=demo`.

**Render applies a changed setting only with a deploy,** not on save or restart, so run the Deploy workflow after
changing one. A bad value logs `logs are not exported` as a warning on Render, and the site serves without export.
A refused key or an unreachable stack logs `BatchLogProcessor.ExportError` on Render. Check a `served` line arrives
in Grafana's Explore view with `{service_name="tetsu"}`. Grafana stores dotted field names with underscores, so
`http.route` reads `http_route` there.

The alerts are ADR-0049's table, written as Grafana alert rules on the Loki data source, in folder `tetsu`, group
`tetsu-demo`. Each evaluates every minute with no pending period, sets **no data** to Normal, since the free
instance sleeps, sets **error or timeout** to Error, so a broken query emails too, and sends to contact point
`maintainer-email`. Each rule's summary says what to do.

| Rule | Query | Fires when |
| --- | --- | --- |
| Event stopped | `sum(count_over_time({service_name="tetsu"} \| alert="event_stopped" [5m]))` | above 0 |
| Server errors | `sum(count_over_time({service_name="tetsu"} \| alert="server_error" [5m]))` | above 9 |
| Start failed | `sum(count_over_time({service_name="tetsu"} \| alert="start_failed" [5m]))` | above 0 |

Another backend takes the same three rules, in its own query language, from ADR-0049's table.

Sessions read the stack through Grafana's MCP server, read-only ([ADR-0052](../adr/0052-grafana-from-agent-sessions.md)).
To set it up once, make service account `claude-mcp` with the **Viewer** role under **Administration → Users and
access → Service accounts**, add it a token, and register the server yourself, so the token stays out of sessions:

```bash
 claude mcp add grafana -e GRAFANA_URL=https://regalchicken947.grafana.net \
  -e GRAFANA_SERVICE_ACCOUNT_TOKEN='<token>' -- uvx mcp-grafana@2.2.0 --disable-write
```

The leading space keeps the command out of shell history.

## Resetting the demo database

Before the first real business, a change to a module's first migrations may be folded into those files rather than
added as new ones, as for [ADR-0060](../adr/0060-database-hardening.md). The runner then refuses the demo's database,
since its record holds the old text, so the changed modules start over. Deploy with their schemas named:

```bash
gh workflow run deploy -f reset=tenancy,audit
```

The Reset step checks the server against the floor, then drops the named schemas, records of applied files
included, and migrates; the deploy's own migrate then finds nothing new, and it checks and seeds as usual. It drops
nothing if any name is not a module's schema. The demo
serves errors from the drop until the deploy finishes.

## When `serve` refuses to start

`tetsu serve` will not start while a stored tenant holds an address a release has since reserved for Tetsu's own
pages ([#90](https://github.com/heavymedl-labs/tetsu/issues/90)), since a site route would hide that tenant. A
forward deploy then fails at Render's start and the previous instance keeps serving; a rollback is never blocked,
since older releases reserve fewer names. Its log names each address. Give each tenant a new slug, which moves its
address too, then deploy again:

```bash
gh workflow run deploy -f chore=rename -f target=/login -f value=new-slug
```

It also will not start on a database behind its code: a migration not applied, or one changed since it was
applied, which its log names ([#164](https://github.com/heavymedl-labs/tetsu/issues/164)). The deploy migrates before
it serves, so this means the migrate step was skipped or failed; run the deploy again. A database ahead of the code,
as after a rollback, still serves. A changed migration needs the change undone in code, since an applied file must
stay as it was.

## When an event stops retrying

An event that fails 10 times logs an `ERROR` line, `event stopped retrying`, with its ID, type, each failing
consumer, and `alert` set to `event_stopped`, which raises the [alert](#logs-and-alerts) ([ADR-0043](../adr/0043-domain-event-delivery.md)). A kept event logs `event keeps failing`
instead, with the same `alert`, and goes on retrying hourly. Once the consumer is fixed and deployed, replay a stopped
event within 30 days of its last attempt, after which it is removed ([ADR-0080](../adr/0080-event-rows-past-their-use.md)),
with the schema of the module that recorded it:

```bash
gh workflow run deploy -f chore=replay -f target=identity -f value=<event ID>
```

It makes the event due again with its attempts cleared, from the business's outbox or the global one, as the
delivery role, and says which. Not found means the schema is wrong, or the event was removed: delivered over 7
days ago, or stopped over 30.
Consumers skip what they already applied, so a replay repeats nothing.

The dispatcher removes rows past their use each hour ([ADR-0080](../adr/0080-event-rows-past-their-use.md)), and
logs how many when it removes any. Its first cleanup after ADR-0080 shipped removes every stopped event over 30
days old at once, and works through a backlog of old inbox IDs twenty thousand an hour. The demo holds no real
business's data, so nothing that matters is lost; a database that did would replay or keep its stopped events
first. An older build that runs again, through `rollback_to` or the minute two instances overlap in a deploy, does
not mark an event kept when a consumer that must never lose it went unreached, so replay any event it stops within
30 days. A cleanup that logs `removing event rows past their use failed` keeps running hourly, and removes nothing
from the inboxes until its cause, such as an outbox that lost its delivery policy, is fixed.

## Hosted chores

A chore runs instead of a deploy, in the deploy workflow's chore job, the only workflow that may read secrets
(ADR-0027). It runs from the image live on the demo, as `tetsu_app`, whose URL it reads from the Render
service through `RENDER_API_KEY` and masks in the log. It runs only from `main`, never alongside a deploy, and
refuses inputs that are not the shape the chore takes.

## Operators on the demo

An operator is made from your own terminal, since an invite link prints only to one, with the demo's settings in
`.env.demo` at the repository's root ([ADR-0073](../adr/0073-settings-files-per-environment.md)). Git ignores it.
Make it once, readable only by you (`chmod 600 .env.demo`), with these lines:

```text
DATABASE_URL=<the Render service's DATABASE_URL, which is tetsu_app's>
TETSU_ORIGIN=https://tetsu-demo.onrender.com
```

The demo sends no email until Tetsu has a domain ([ADR-0079](../adr/0079-no-mail-provider-before-a-domain.md),
[#151](https://github.com/heavymedl-labs/tetsu/issues/151)): notices are kept for the mailbox page, and invites,
which are never kept, are only printed. Mail settings, when they come, go in this file and Render's environment
alike, all of them in each ([ADR-0078](../adr/0078-mail-settings-from-one-place.md)).

Then, from the commit the demo runs:

```bash
cargo tetsu --env demo operator create "Your Name" you@example.com   # prints the invite link, and emails it
cargo tetsu --env demo operator list
cargo tetsu --env demo operator email 1 you@example.com   # gives an operator made before addresses one
```

Open the link to join with a passkey; it logs you in at `/ops`. Every other `operator` command works the same way.
