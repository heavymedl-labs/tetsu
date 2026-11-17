# 0061. One service and two logins until Tetsu has domains

- Status: Accepted
- Date: 2026-11-15
- Amends [ADR-0040](0040-identity-and-sign-in.md) (passkey domains), [ADR-0042](0042-tenant-isolation.md) (one
  service per demo realm, the operator login), and [ADR-0048](0048-sign-in-design.md) (realm services)

## Context

ADR-0042 runs each demo realm as its own Render service, `tetsu-demo`, `tetsu-demo-staff`, and `tetsu-demo-ops`, so
that each is its own site with its own cookies and passkey domain, as ADR-0040 asks. It also gives operators, and
in ADR-0060's first draft the dispatcher, their own database logins. Each environment would need three services and
four database credentials, sharing Render's 750 free hours.

Each layer is defense in depth: sessions are bound to their realm on the server, each realm's cookie has its own
name, and bound parameters guard against injected SQL. The maintainer wants the simplest setup while the first
features are built, as long as splitting later stays easy.

## Decision

- **One service serves every realm, each under its own path,** such as `/ops` for operators. Each realm keeps its
  own `__Host-` cookie name, its own session rows, and its own origin check, all set per realm in configuration.
  Realm paths are reserved names, so no business can take one ([#90](https://github.com/heavymedl-labs/tetsu/issues/90)).
- **The realms share the service's passkey domain** until Tetsu has domains
  ([#102](https://github.com/heavymedl-labs/tetsu/issues/102)). A browser may offer an operator's passkey on another
  realm's login; the server refuses any passkey whose account is not in the realm logging in, as it refuses one
  from another business. Demo passkeys are thrown away when the domains arrive in any case (ADR-0057).
- **Two database logins:** the owner, which runs migrations, and the application. Operators' pages and the
  dispatcher use the application login. Module roles, row security, and the catalog check stay as ADR-0042 sets
  them, since the runner sets them up and nobody configures them.
- **Splitting later is configuration.** Which realms a process serves, their hosts, and their paths are settings
  ([ADR-0050](0050-local-settings-file.md)), so moving a realm to its own subdomain or service deploys the same
  image with different settings. A separate login for operators, or for the dispatcher, is a role the runner
  already creates, plus one more database URL.

## When to split

- **Realms move to their own subdomains** when Tetsu's domains arrive, before the Level 2 review, which ADR-0040's
  device test needs on the real domains anyway.
- **Operators get their own login** when [#24](https://github.com/heavymedl-labs/tetsu/issues/24) lets them read
  inside businesses.
- **The dispatcher gets its own login** before the first real business's data.

## Alternatives considered

- **Keep three services and four logins now** (ADR-0042, ADR-0060's draft). Keeps the separation, at the cost of
  more setup in every environment and free hours shared three ways, for a demo holding fictional data.
- **Operators by command only, with no operator pages.** Simpler still, but it changes the order ADR-0048 sets for
  the slices, and operator login is how login gets tested first.

## Consequences

- A script injected into one realm's page runs on the same origin as the others' pages. It cannot read another
  realm's `HttpOnly` cookie, but it can load that realm's pages, read their form token, and act for whoever is signed
  in there in the same browser. Only someone logged in to two realms in one browser is exposed, which on the demo is
  the maintainer. The content security policy (#110) is the guard until the split before the Level 2 review.
- #110 builds operator pages under `/ops` on `tetsu-demo`, and no `tetsu-demo-ops` service is created.

## Sources

- [Postgres database design: a practice review](../research/database-design-review.md), 2026-11-14, on the
  dispatcher's and operators' logins.
- [Public Suffix List](https://publicsuffix.org/list/), which makes each `onrender.com` service its own site.
