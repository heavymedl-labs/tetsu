# 0092. A service per realm before domains

- Status: Accepted
- Date: 2026-11-25
- Supersedes [ADR-0061](0061-one-service-until-domains.md)'s one service for every realm and its shared passkey
  domain; its two database logins stand

## Context

[ADR-0040](0040-identity-and-sign-in.md) gives each realm a host of its own, so its passkeys answer only there
(`v5.0.0-3.5.4`). ADR-0061 put that off until Tetsu has domains
([#102](https://github.com/heavymedl-labs/tetsu/issues/102)): one service would serve every realm under its own path,
sharing one passkey domain, since three services would share Render's 750 free hours. It made splitting later a
matter of configuration.

Building staff pages for [#111](https://github.com/heavymedl-labs/tetsu/issues/111) showed what sharing costs. Each
login's passkey picker offers every realm's passkeys, a WebAuthn signal that removes an unknown passkey would delete
another realm's, and the code that guards against both would be thrown away at the split. A free service that sleeps
when idle spends free hours only while used, and nothing keeps the demo awake, so a service per realm fits the hours
ADR-0061 worried about ([staff pages and their host](../research/identity/staff-pages.md)). On 2026-11-25 the
maintainer chose to split now ([#169](https://github.com/heavymedl-labs/tetsu/issues/169)).

## Decision

- **Each realm is a service of its own, on its own host,** before domains: the site and customers on one, admins on
  another, staff on a third, all one image. `TETSU_SERVES` names what a service serves, and this machine serves every
  realm on one origin ([realm hosts](../research/identity/realm-hosts.md)).
- **Each realm's passkeys belong to its own host,** from its own origin setting, `TETSU_ADMIN_ORIGIN` or
  `TETSU_STAFF_ORIGIN`, and its invite links point there. `TETSU_ORIGIN` goes, since admins were its only reader; the
  approved plan kept it for the site, which reads no origin yet.
- **Realms keep their paths on their own hosts,** `/admin` and `/app` (ADR-0084), and a host without the site sends
  its root to its realm's home.
- **The repository's code, workflow, and settings name neither the host provider nor a stage.** Each realm's origin
  and service are values of the deploy's environment, named for what they configure
  ([deploy configuration](../research/deploy-configuration.md)); one script holds the provider's calls, and the
  runbook records the current values, so moving hosts changes that script and those values.
- **A deploy confirms each realm's service with the host before it changes anything:** named, its own, and served
  at that realm's origin. A service that then fails to deploy leaves those before it on the new image, which the run
  reports and the runbook recovers. A rollback to an image from before this split is refused, since such an image
  serves admins on the site's host.
- **ADR-0061's two database logins stand,** with its rules for when operators and the dispatcher get their own.

## Alternatives considered

- **Keep ADR-0061 until domains.** No new services, but shared-host guards built for staff pages would be thrown
  away, and each login would offer every realm's passkeys meanwhile.
- **One service answering several hosts.** One deploy, but on `onrender.com` a second host needs a custom domain,
  which waits for #102.

## Consequences

- A script injected into one realm's page no longer reaches another realm's pages, since each host is its own site
  on a public suffix; ADR-0061's exposure for someone logged in to two realms ends on the demo.
- [ADR-0087](0087-one-log-in-button.md)'s remembered-browser hint, set at admin login, cannot be read by the site's
  host. Its "Log in" routing is decided again on #111 before that sub-slice is built.
- The demo runs three free services; the runbook says how each is set, and a deploy checks each host.
- Admin passkeys on the demo are enrolled again on the admin host, in the reset #111 plans.

## Sources

- [Staff pages and their host](../research/identity/staff-pages.md) and [a host for each realm](../research/identity/realm-hosts.md),
  2026-11-25.
- [Render: deploy for free](https://render.com/docs/free), on pooled free hours and sleeping services.
- [Public Suffix List](https://publicsuffix.org/list/), which makes each `onrender.com` service its own site.
