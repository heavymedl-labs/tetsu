# 0045. Operations baseline

- Status: Proposed
- Date: 2026-11-11

## Context

Tetsu must be observed, backed up, and recovered without tying itself to one provider
([ADR-0003](0003-architecture.md)). Bookings and money cannot be lost, Anvil Row's morning rush across two time
zones sets when an alert matters, and every log line needs its tenant without leaking tenant data.

Until a client showcase is close, Tetsu runs only the demo, on free hosting, with fictional businesses rebuilt
from their seed files ([ADR-0018](0018-demo-businesses-as-data.md)). No real business can sign up before the
Level 2 review ([#83](https://github.com/heavymedl-labs/tetsu/issues/83)). So the decision has two parts: what the
demo needs now, which sign-in builds on, and the targets production must meet before any real business arrives.

## Decision

### Now, on the demo

- **Logs are structured JSON on standard output,** written through the `tracing` and `tracing-subscriber` crates.
  Every host collects standard output, so the host's log viewer is enough for now, and moving host loses nothing.
- **Each request's log lines carry a request ID, the tenant's ID, and the realm,** never names, emails, codes,
  tokens, or request bodies. Identifiers that could find a person, such as an account ID, appear only where a log
  line needs them to be useful.
- **Security events go to an audit record, not the log.** Sign-ins, passkey changes, recoveries, and relinks are
  domain events ([ADR-0043](0043-domain-event-delivery.md)) that the audit consumer stores per tenant. Logs are
  for operating Tetsu and may be lost; the audit record may not.
- **An event that stops retrying is logged at error level** with its ID, type, and consumer, so it can be found
  and replayed.
- **The demo has no backups.** Its data is fictional and rebuilt from seed files. The host's point-in-time restore
  covers mistakes within its window.

### Before a real business, in production

These are targets. Each is met, and the restore tested, before the Level 2 review.

- **Recovery point: at most 5 minutes of data lost.** Continuous point-in-time recovery from the database host
  meets it, kept for at least 7 days.
- **A daily logical backup to object storage with another provider,** encrypted, kept 30 days, so a host failure
  or account loss is survivable. It runs as a dedicated backup role with `BYPASSRLS`, which no request ever uses,
  since a dump of forced row-level security needs it ([ADR-0042](0042-tenant-isolation.md)).
- **Recovery time: back in service within 4 hours** of a total loss of the database.
- **A restore is tested every month,** into a scratch database, with the app started against it and checked.
- **Availability: 99.9% a month** for booking and payment paths, and 99.5% for everything else, measured by an
  outside check and by the share of requests that succeed. Alerts fire on fast burn of that error budget, and on
  events that stop retrying, rather than on single errors.
- **Metrics and traces go out through OpenTelemetry,** to a backend chosen with production hosting, so the backend
  can change without code changes.
- **Alerts reach the maintainer's phone,** through a service chosen with production hosting. With one maintainer
  there is no rota; the targets above assume that.

## Alternatives considered

- **The host's own metrics and alerting.** Free and easy, but tied to the host, and Render and Neon each see only
  their half.
- **OpenTelemetry from day one.** Portable, but a collector and backend cost money or setup before there is
  anything to measure. `tracing` exports to OpenTelemetry later without rewriting call sites.
- **Self-run backups with pgBackRest.** Full control, but it needs a server and storage to run. A managed host's
  point-in-time recovery plus a daily dump elsewhere covers the same risks for less.
- **Backups of the demo now.** Its data rebuilds from seed files, so backups would protect nothing.

## Consequences

- `tracing` and `tracing-subscriber` join the workspace. Both are MIT-licensed, maintained by the Tokio project,
  and each had over 190 million downloads in the 90 days before 2026-11-11. `tracing` 0.1.44 was released on
  2025-12-18 and `tracing-subscriber` 0.3.23 on 2026-03-13, both needing Rust 1.65 or later. Advisory checks in
  `scripts/check.sh` cover them like every other dependency.
- Sign-in ([#20](https://github.com/heavymedl-labs/tetsu/issues/20)) can ship on the demo once logging and the
  audit consumer exist; nothing else here blocks it.
- [#27](https://github.com/heavymedl-labs/tetsu/issues/27)'s production half gates the Level 2 review rather than
  sign-in on the demo.
- Choosing an object storage provider, an OpenTelemetry backend, an outside uptime check, and an alerting service
  each needs its own ADR, made with production hosting
  ([#104](https://github.com/heavymedl-labs/tetsu/issues/104)).
- Revisit the targets once real businesses give real traffic and real cost.

## Sources

- Google, [*Site Reliability Engineering*](https://sre.google/sre-book/table-of-contents/), chapters 4 and 6:
  service level objectives and monitoring.
- Google, [*The Site Reliability Workbook*](https://sre.google/workbook/alerting-on-slos/), "Alerting on SLOs":
  burn-rate alerts.
- [OpenTelemetry documentation](https://opentelemetry.io/docs/).
- PostgreSQL 18 documentation, [Continuous archiving and point-in-time recovery](https://www.postgresql.org/docs/18/continuous-archiving.html).
- [tracing on crates.io](https://crates.io/crates/tracing) and
  [tracing-subscriber on crates.io](https://crates.io/crates/tracing-subscriber), read 2026-11-11.
