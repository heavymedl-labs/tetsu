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

A rollback cannot cross a migration that takes rights away. Code from before
[#106](https://github.com/heavymedl-labs/tetsu/issues/106) reads Tenancy's tables as `tetsu_app` directly, which
Tenancy's second migration forbids, so every request it served would fail; fix forward instead. The same is true of
the old instance in the minute between the Migrate step and Render's switch to the new one.

Seeding is still by hand until [#73](https://github.com/heavymedl-labs/tetsu/issues/73), as `tetsu_app`, like
`tetsu serve`. Copy `DATABASE_URL` from the service's environment in Render and paste it at the prompt, so it is never
shown or saved; `seed` skips tenants that exist:

```bash
read -rs DATABASE_URL && DATABASE_URL="$DATABASE_URL" tetsu seed ../tetsu-demos/demos
```

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
tables, sequence, and record. Last, run hosted test 8 as `tetsu_owner`. It checks the server version and every
catalog rule, `tetsu_app` included, and that each module's role sees no row with no tenant set, and prints any
problem. No copy of the password is kept; to run it again later, set a fresh one the same way.

```bash
DATABASE_URL="$(printf '%s' "$base" \
  | sed -E "s#//tetsu_migrate:[^@]*@#//tetsu_owner:${pw}@#; s/([?&])sslmode=[^&]*&?/\1/; s/[?&]\$//")" \
  cargo run -p tetsu-postgres --example check_isolation
```

Connection strings drop Neon's `sslmode`, which `tetsu` refuses since TLS is always required, and keep
`channel_binding=require`. Neon's certificate is public, so no `DATABASE_CA_FILE` is needed.

## Render

Web service `tetsu-demo` (`srv-dbp542rbc2fs73962k4g`) in Ohio, on the free instance, from the private image
`ghcr.io/heavymedl-labs/tetsu` by digest, pulled with the `ghcr-tetsu` registry credential. Auto-deploy is off.

- **Command:** `/usr/local/bin/tetsu serve`. Render's command replaces the image's entrypoint too, so `serve`
  alone exits with status 128.
- **Environment:** `TETSU_DEMO=true` and `DATABASE_URL`, and the export settings under
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

## When `serve` refuses to start

`tetsu serve` will not start while a stored tenant holds an address a release has since reserved for Tetsu's own
pages ([#90](https://github.com/heavymedl-labs/tetsu/issues/90)), since a site route would hide that tenant. A
forward deploy then fails at Render's start and the previous instance keeps serving; a rollback is never blocked,
since older releases reserve fewer names. Its log names each address. Until a command for it exists, give the
tenant a new slug and address as `tetsu_owner`, then deploy again:

```sql
UPDATE tenancy.tenants SET slug = 'new-slug' WHERE slug = 'login';
UPDATE tenancy.addresses SET address = '/new-slug' WHERE address = '/login';
```

## When an event stops retrying

An event that fails 10 times logs an `ERROR` line, `event stopped retrying`, with its ID, type, each failing
consumer, and `alert` set to `event_stopped`, which raises the [alert](#logs-and-alerts) ([ADR-0043](../adr/0043-domain-event-delivery.md)). A kept event logs `event keeps failing`
instead, with the same `alert`, and goes on retrying hourly. Once the consumer is fixed and deployed, replay a stopped event by making it
due again, with the schema of the module that recorded it. A business's events are in `outbox`, which row-level
security hides from `tetsu_owner`, so act as the delivery role, which may settle every tenant's events, through
the service's own connection. Paste `DATABASE_URL` at the prompt, as for seeding, so it is never shown or saved:

```bash
read -rs DATABASE_URL && psql "$DATABASE_URL"
```

```sql
BEGIN;
SET LOCAL ROLE tetsu_delivery;
UPDATE identity.outbox SET attempts = 0, due_at = now() WHERE id = '<event ID>';
COMMIT;
```

It reports `UPDATE 1`. `UPDATE 0` means the event is not in that table: check the schema, use `outbox_global` for
an event that belongs to no business, or it was delivered and removed. Consumers skip what they already applied,
so a replay repeats nothing.
