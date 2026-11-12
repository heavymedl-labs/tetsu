# Secrets and tokens

Every secret Tetsu uses, by name, never by value. Add a row when you create a secret, and update its date when you
rotate it. When to rotate, and what to do after a leak, is in [ADR-0027](../adr/0027-secrets-revised.md).

`scripts/check.sh` fails when a date here is more than 13 months old, when a workflow uses a secret with no row,
or when a workflow sets a `GITHUB_TOKEN` permission its row does not list.

## Secrets

The demo environment's setup is in [demo-environment.md](demo-environment.md). Each value went straight from the
tool that made it into its store, never shown.

| Secret | Store | Scope | Use | Last rotated |
| --- | --- | --- | --- | --- |
| `GITHUB_TOKEN` | GitHub, issued per workflow run | That run, with the permissions below | Each workflow's own API calls | automatic |
| `MIGRATION_DATABASE_URL` | GitHub Actions, repository secret | Role `tetsu_owner` on Neon project `tetsu-demo`, direct endpoint, which owns Tetsu's schemas and creates its roles; set from a password no one kept ([runbook](demo-environment.md#moving-onto-tetsus-own-roles)) | `deploy.yml`'s Migrate step | 2026-11-12 |
| `RENDER_API_KEY` | GitHub Actions, repository secret | The maintainer's Render account; Render offers no narrower key | `deploy.yml`'s Deploy step | 2026-11-09 |
| `DATABASE_URL` | Render, environment of service `tetsu-demo` | Role `tetsu_app`, which acts only as module roles, one transaction at a time | `tetsu serve` | 2026-11-09 |
| `ghcr-tetsu` | Render, registry credential | A GitHub classic token with only `read:packages`, expiring 2027-11-09 | Render pulling the private image | 2026-11-09 |
| Render CLI token | `~/.render/cli.yaml` on the maintainer's machine, mode 600 | The maintainer's Render account, refreshed by `render login` | Setting up and inspecting the service | 2026-11-09 |
| Neon CLI token | `~/.config/neon/credentials.json` on the maintainer's machine, mode 600 | The maintainer's Neon account, refreshed by `neonctl auth` | Setting up and inspecting the database | 2026-11-09 |

## Workflow tokens

| Workflow | `GITHUB_TOKEN` permissions |
| --- | --- |
| `ci.yml` | `contents: read`, and `actions: read` to find the commit the last successful run checked; its publish job, which runs only GitHub's own actions after every check passes, has `packages: write` alone, to publish the image a manual run on main checked |
| `deploy.yml` | `contents: read`, `actions: read` to find the CI run that checked the commit and download its digest, `packages: write` to pull the image and prune old ones, and `deployments: write` to record each deploy. Render's service ID is a repository variable, not a secret |
| `benchmarks.yml` | `contents: read`; its image job adds `packages: write` to push the benchmark image, and its run job `packages: read` to pull it |
