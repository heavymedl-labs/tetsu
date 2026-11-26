# Deploy configuration without a provider's or a stage's name

Researched 2026-11-25 for [#169](https://github.com/heavymedl-labs/tetsu/issues/169), after the maintainer asked that
deploy configuration name neither the host provider nor a temporary stage: Render is treated as a viable production
host, though it serves only demo businesses for now. What is settled is not repeated here: Render as the first host
([ADR-0012](../adr/0012-first-hosting-and-deploys.md)), a service per realm
([ADR-0092](../adr/0092-a-service-per-realm-before-domains.md)), and whether a business is a demo, which
is data, not infrastructure ([ADR-0018](../adr/0018-demo-businesses-as-data.md)). Sources were read through search
summaries on 2026-11-25.

## Summary

Twelve-factor config names each setting for what it configures and sets it per deploy, never grouped under an
environment's name, so a second environment reuses the names with its own values. GitHub environments hold exactly
that: variables and secrets scoped to one environment, reachable only by jobs that name it, with branch rules; a
public repository has them on the free plan, and this one already records deploys under an environment, `demo`.
Render groups services into projects and environments, and shares values across services through environment groups,
so settings every realm's service needs are set once. A Render service's `onrender.com` name is fixed when it is
made, which stays the host's matter.

## Name settings for what they configure, and set them per deploy

**Takeaway:**

A setting's name says what it configures, such as a realm's service or origin; which deploy it belongs to is where
it is set, not part of its name.

**Cited Findings:**

- Twelve-factor config keeps each setting "fully orthogonal", and settings "are never grouped together as
  'environments', but instead are independently managed for each deploy", which scales as an app gains deploys
  ([12factor.net: config](https://12factor.net/config)).
- Practitioners name a setting for its resource, such as `USERS_DB_CONNECTION_URI`, rather than for a stage, such as
  `PROD_DB_URL` ([DEV Community](https://dev.to/sebm/application-configuration-best-practices-26ag)).

## GitHub environments scope a deploy's values and secrets

**Takeaway:**

Values and secrets set on a GitHub environment reach only jobs that name it, and its branch rules decide which refs
may deploy there, so the workflow uses plain names and the environment supplies them.

**Cited Findings:**

- An environment can require approval, restrict which branches deploy to it, and limit secrets; its secrets are
  "only available to workflow jobs that reference the environment"
  ([GitHub Docs: deployments and environments](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments)).
- Branch policies can allow only chosen branches, administrators bypass protection rules unless that is turned off,
  and deleting an environment deletes its secrets and rules
  ([GitHub Docs: control deployments](https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/control-deployments)).
- On GitHub Free, environment secrets reach only public repositories (same source); this repository is public, and
  its deploys are recorded under the environment `demo`.

## Render shares settings across services, and fixes each service's name

**Takeaway:**

Render's environment groups hold settings once for several services, and its projects separate environments; a
service's `onrender.com` name cannot change, so neutral names need new services.

**Cited Findings:**

- A project holds environments, such as staging and production, and an environment group bundles variables and
  secret files for several services, optionally scoped to one environment; a service's own variable overrides a
  group's ([Render: projects](https://render.com/docs/projects);
  [Render: environment variables](https://render.com/docs/configure-environment-variables)). A Hobby workspace has
  one project with up to two environments (same source).
- Blueprints, a `render.yaml` file, declare services, environments, and groups as code
  ([Render: Blueprints](https://render.com/docs/infrastructure-as-code)).
- A service's subdomain is unique across Render and "cannot currently be changed once the service is created";
  renaming a service changes only its display name, and a custom domain is the way to another address
  ([Render Community](https://community.render.com/t/can-i-change-my-existing-subdomain-onrender-com-to-a-new-one/9999)).
  On 2026-11-25, `tetsu-web`, `tetsu-admin`, and `tetsu-app` answered as unclaimed, and `tetsu` did not answer.

## Recommendations for #169

The repository names neither the host provider nor a stage; how the host itself is set up is the host's own matter,
recorded in the runbook as current values.

- **Settings named for what they configure:** `SITE_SERVICE`, `ADMIN_SERVICE`, `STAFF_SERVICE`, `SITE_ORIGIN`,
  `ADMIN_ORIGIN`, `STAFF_ORIGIN`, and the secret `HOST_TOKEN`, with no stage or provider in any name.
- **Set on a GitHub environment named `production`,** which the deploy job names and which allows only `main`; its
  deploy records move there from `demo`, so a rollback reads what is live in production. Whether a business is a
  demo stays data, marked by `TETSU_DEMO`, not a stage of the infrastructure.
- **One adapter knows the provider:** `scripts/host.py` holds every call to it, so moving hosts changes that file and
  the environment's values, never the workflow.
- **Names in the repository follow:** the deploy job and runbook speak of production, and the local settings file
  for production is `.env.production` (ADR-0073).
- **The host's own setup stays the host's:** services keep the names they were made with, and Render's projects,
  environment groups, and Blueprints are choices for the host, not the repository.

## Gaps

- Pages were read through search summaries, not opened. Blueprint field names for image services and environment
  groups were not confirmed.
- Whether `tetsu.onrender.com` is taken or only slow to answer was not settled.
