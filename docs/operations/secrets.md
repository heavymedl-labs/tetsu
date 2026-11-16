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
| `MIGRATION_DATABASE_URL` | GitHub Actions, repository secret | Role `tetsu_owner` on Neon project `tetsu-demo`, direct endpoint, which owns Tetsu's schemas and creates its roles; set from a password no one kept ([runbook](demo-environment.md#moving-onto-tetsus-own-roles)) | `deploy.yml`'s Reset, Migrate, and Check isolation steps | 2026-11-12 |
| `RENDER_API_KEY` | GitHub Actions, repository secret | The maintainer's Render account; Render offers no narrower key | `deploy.yml`'s Deploy and Seed steps, and its chore job, which read the service's `DATABASE_URL` through it and mask it | 2026-11-09 |
| `DEMOS_READ_TOKEN` | GitHub Actions, repository secret | A fine-grained GitHub token that reaches only `heavymedl-labs/tetsu-demos`, with read access to its contents (ADR-0062), owned by the organization, expiring 2027-11-16 | `deploy.yml`'s Fetch the demo businesses step | 2026-11-15 |
| `DATABASE_URL` | Render, environment of service `tetsu-demo` | Role `tetsu_app`, which acts only as module roles, one transaction at a time | `tetsu serve` | 2026-11-09 |
| `OTEL_EXPORTER_OTLP_HEADERS` | Render, environment of service `tetsu-demo` | Grafana Cloud token `render-tetsu-demo` on access policy `tetsu-demo-logs`: only `logs:write` on stack `regalchicken947`; it never expires, so it rotates yearly ([runbook](demo-environment.md#logs-and-alerts)) | `tetsu serve`'s log export | 2026-11-13 |
| `ghcr-tetsu` | Render, registry credential | A GitHub classic token with only `read:packages`, expiring 2027-11-09 | Render pulling the private image | 2026-11-09 |
| Grafana service account token | Claude Code's local MCP settings on the maintainer's machine | Service account `claude-mcp` on stack `regalchicken947`, **Viewer** role, used by `mcp-grafana --disable-write` ([ADR-0052](../adr/0052-grafana-from-agent-sessions.md)) | Sessions reading logs and alert state | 2026-11-13 |
| Render CLI token | `~/.render/cli.yaml` on the maintainer's machine, mode 600 | The maintainer's Render account, refreshed by `render login` | Setting up and inspecting the service | 2026-11-09 |
| Neon CLI token | `~/.config/neon/credentials.json` on the maintainer's machine, mode 600 | The maintainer's Neon account, refreshed by `neonctl auth` | Setting up and inspecting the database | 2026-11-09 |

## Renewals

What lapses on a date of its own, beside each secret's yearly rotation above. A weekly workflow, `renewals.yml`,
opens an issue for each date here or rotation deadline within 30 days, once; `scripts/check.sh` fails on a date
here that has passed.

| What | Renew by | Why |
| --- | --- | --- |
| `DEMOS_READ_TOKEN` | 2027-11-16 | The GitHub token expires; make a new one with `heavymedl-labs` as resource owner, only `tetsu-demos`, and Contents read-only, then set the secret |
| `ghcr-tetsu` | 2027-11-09 | The GitHub token expires; make a new one with only `read:packages` and set it on Render |
| Grafana Cloud trial | 2026-11-27 | The stack's trial ends; with no card on file it stays on the free plan, so check a `served` line still arrives, then remove this row |

## Workflow tokens

| Workflow | `GITHUB_TOKEN` permissions |
| --- | --- |
| `ci.yml` | `permissions: {}` for the workflow, so each job asks for its own: the checks job `contents: read`, and `actions: read` to find the commit the last successful run checked; the image job the same, so a run whose base is its own commit stops at once in both (ADR-0064); and the publish job, which runs only GitHub's own actions after every check passes, `packages: write` alone, to publish the image a manual run on main checked |
| `deploy.yml` | `contents: read`, `actions: read` to find the CI run that checked the commit and download its digest, `packages: write` to pull the image and prune old ones, and `deployments: write` to record each deploy. Its chore job has `contents: read`, `deployments: read` to find the image live on the demo, and `packages: read` to pull it. Render's service ID is a repository variable, not a secret |
| `renewals.yml` | `contents: read`, and `issues: write` to open a reminder for each renewal due within 30 days |
| `benchmarks.yml` | `contents: read`; its image job adds `packages: write` to push the benchmark image, and its run job `packages: read` to pull it |
