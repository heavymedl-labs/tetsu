# Secrets and tokens

Every secret Tetsu uses, by name, never by value. Add a row when you create a secret, and update its date when you
rotate it. When to rotate, and what to do after a leak, is in [ADR-0027](../adr/0027-secrets-revised.md).

`scripts/check.sh` fails when a date here is more than 13 months old, when a workflow uses a secret with no row,
or when a workflow sets a `GITHUB_TOKEN` permission its row does not list.

## Secrets

None of the deploy secrets exist yet. The deploy key and migration URL, the app's database URL, the registry read
token, and the maintainer's local Render and Neon CLI keys join this table when they are created, with the first
deploy ([#77](https://github.com/heavymedl-labs/tetsu/issues/77)).

| Secret | Store | Scope | Use | Last rotated |
| --- | --- | --- | --- | --- |
| `GITHUB_TOKEN` | GitHub, issued per workflow run | That run, with the permissions below | Each workflow's own API calls | automatic |

## Workflow tokens

| Workflow | `GITHUB_TOKEN` permissions |
| --- | --- |
| `ci.yml` | `contents: read`, and `actions: read` to find the commit the last successful run checked |
| `benchmarks.yml` | `contents: read`; its image job adds `packages: write` to push the benchmark image, and its run job `packages: read` to pull it |
