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

Seeding is still by hand until [#73](https://github.com/heavymedl-labs/tetsu/issues/73). The URL comes from Neon
inside the command, so it is never shown or saved; `seed` skips tenants that exist:

```bash
DATABASE_URL="$(neonctl connection-string --project-id square-tree-12604600 --role-name tetsu_migrate \
  --database-name tetsu | sed -E 's/([?&])sslmode=[^&]*&?/\1/; s/[?&]$//')" tetsu seed ../tetsu-demos/demos
```

## Neon

Project `tetsu-demo` (`square-tree-12604600`) in `aws-us-east-2`, Postgres 18, on the free plan. Database `tetsu`
on the direct endpoint, never the pooler.

- **`tetsu_migrate`** owns the database and runs migrations. Neon made it, so it belongs to `neon_superuser`.
- **`tetsu_app`** is what `tetsu serve` uses. It was created in SQL, so it belongs to no role and has no special
  attribute, and it gets its rights from what the migration role creates:

  ```sql
  CREATE ROLE tetsu_app LOGIN;
  GRANT CONNECT ON DATABASE tetsu TO tetsu_app;
  ALTER DEFAULT PRIVILEGES FOR ROLE tetsu_migrate GRANT USAGE ON SCHEMAS TO tetsu_app;
  ALTER DEFAULT PRIVILEGES FOR ROLE tetsu_migrate GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO tetsu_app;
  ALTER DEFAULT PRIVILEGES FOR ROLE tetsu_migrate GRANT USAGE, SELECT ON SEQUENCES TO tetsu_app;
  REVOKE CREATE ON SCHEMA public FROM PUBLIC;
  ```

  Each module's `migrations` table stays read-only to it, since the migration runner takes back every other right
  on it.

  Its password was generated and set with `ALTER ROLE`, then written only into Render's environment.

Connection strings drop Neon's `sslmode`, which `tetsu` refuses since TLS is always required, and keep
`channel_binding=require`. Neon's certificate is public, so no `DATABASE_CA_FILE` is needed.

## Render

Web service `tetsu-demo` (`srv-dbp542rbc2fs73962k4g`) in Ohio, on the free instance, from the private image
`ghcr.io/heavymedl-labs/tetsu` by digest, pulled with the `ghcr-tetsu` registry credential. Auto-deploy is off.

- **Command:** `/usr/local/bin/tetsu serve`. Render's command replaces the image's entrypoint too, so `serve`
  alone exits with status 128.
- **Environment:** `TETSU_DEMO=true` and `DATABASE_URL`. Render sets `PORT`.
- **Health check:** `/`.
- **Free instance:** sleeps after 15 minutes without traffic and takes about a minute to wake.

The GitHub repository variable `RENDER_SERVICE_ID` names the service for `deploy.yml`.
