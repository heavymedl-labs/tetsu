# 0049. Log export

- Status: Proposed
- Date: 2026-11-13

## Context

[ADR-0045](0045-operations-baseline.md) sends logs, traces, and metrics over OTLP to Grafana Cloud, through
settings so another backend can replace it, and alerts on an event that stops retrying and on a burst of server
errors. None of the crates it adopted carries `tracing` events as OTLP logs, and the alerts are built on logs. The
bridge that does is `opentelemetry-appender-tracing`. An accepted ADR changes only by being superseded
([ADR-0001](0001-how-we-work.md)), so adding it takes this ADR. ADR-0045's decision stands.

A request's ID, realm, route, and tenant live on its span, not on each event
([`logs.rs`](../../crates/tetsu/src/logs.rs)). The bridge copies span fields onto a log record only behind its
`experimental_span_attributes` feature. The evaluation is in [Log export research](../research/log-export.md).

## Decision

### The export

- **Adopt `opentelemetry-appender-tracing`** with `experimental_span_attributes`, copying only an allowlist of span
  fields: `request_id`, `realm`, `http.request.method`, `http.route`, and `tenant_id`. The two HTTP fields take
  OpenTelemetry's names, on standard output too, so a backend's HTTP views read them. A span field reaches the
  backend only once a reviewer adds it to the list.
- **Event fields and messages go out as standard output writes them,** so ADR-0045's rule for log lines covers
  them: never names, emails, codes, tokens, or bodies. The export also carries each event's module path, which
  standard output omits.
- **Export with the same filter as standard output,** and drop the `opentelemetry` crates' own events at every
  level, so a failing export cannot log itself into a loop.
- **Send OTLP over HTTP with protobuf,** with gzip compiled in, through `opentelemetry-otlp`'s blocking `reqwest`
  client with rustls, from the SDK's batch processor on its own thread, at its default batch settings.

### Settings

- **Tetsu reads OpenTelemetry's standard names through its settings,** and passes them to the exporter:
  `OTEL_EXPORTER_OTLP_LOGS_ENDPOINT`, or else `OTEL_EXPORTER_OTLP_ENDPOINT`; `OTEL_EXPORTER_OTLP_HEADERS`;
  `OTEL_EXPORTER_OTLP_COMPRESSION`; `OTEL_SERVICE_NAME`, default `tetsu`; and `OTEL_RESOURCE_ATTRIBUTES`.
- **With no endpoint set, there is no export layer.** Standard output still has every line.
- **A bad export setting logs a warning and serves without export,** unlike other settings, which fail at start.
  Export is for operating Tetsu, and must not take the site down.
- **Any backend that takes OTLP over HTTP with header auth replaces Grafana Cloud through these settings alone.** One
  that signs each request takes a collector in between, which is a deploy change, not a code change.

### Alerts

Each alert has a stable `alert` field on the lines that raise it, so every backend's rule is a filter and a count,
and rewording a message breaks no rule.

| `alert` | Logged on | Fires on |
| --- | --- | --- |
| `event_stopped` | An event that stops retrying, or a never-lose event still failing at its tenth attempt | 1 line in 5 minutes |
| `server_error` | A response with a 5xx status, and a panic, which a panic hook logs | 10 lines in 5 minutes |
| `start_failed` | `tetsu serve` failing to start, logged and flushed before it exits | 1 line in 5 minutes |

The table is the rules' one home, in a neutral form. Each backend's rendering, such as LogQL for Grafana, and its
email contact point are recorded in [demo-environment.md](../operations/demo-environment.md). Moving backends means
writing the three rules and the contact point again from this table.

## Alternatives considered

- **Copy every span field.** Simpler, but a field added later for debugging could carry an email off the host
  without review.
- **A homegrown layer that copies span fields.** Avoids an experimental feature, but repeats what the bridge has
  and tests. It is the fallback if the feature is removed.
- **Ship standard output through the host.** Render streams logs over syslog or HTTPS, but that ties export to the
  host, and Grafana Cloud would need a collector the free host cannot run.
- **OTLP over gRPC.** Pulls in `tonic`, and Grafana Cloud documents only HTTP for direct ingest.
- **Rules that match message text.** No change to Tetsu, but a reworded message silently breaks an alert.

## Consequences

- One new crate, and `reqwest` through `opentelemetry-otlp`; their health is in the research page.
- The work touches `logs.rs`, the settings, `main.rs` for the panic hook, the flush, and `start_failed`, the
  dispatcher's alert lines, and a test that the span-derived attributes are exactly the allowlist, each once.
  `logs.rs` marks the allowlist and the `alert` values as a contract with the backend.
- **If export stops, alerts stop silently,** for example after a revoked key or a full quota. A no-data rule cannot
  catch it, since the free host sleeps. The demo accepts this; ADR-0045's outside check covers production.
- The key goes in `OTEL_EXPORTER_OTLP_HEADERS` on the host, with its space written `%20`, as a secret with a row in
  [secrets.md](../operations/secrets.md) and its expiry there ([ADR-0027](0027-secrets-revised.md)). The
  maintainer creates the account and sets it.
- Revisit if the feature is removed, or stays experimental past 1.0, or if
  [#37](https://github.com/heavymedl-labs/tetsu/issues/37)'s load tests show export raising p95 latency on the
  free host's 0.1 CPU.

## Sources

- OpenTelemetry, [Logs bridge API](https://opentelemetry.io/docs/specs/otel/logs/bridge-api/),
  [OTLP exporter configuration](https://opentelemetry.io/docs/specs/otel/protocol/exporter/), and
  [HTTP semantic conventions](https://opentelemetry.io/docs/specs/semconv/http/http-spans/).
- [Log export research](../research/log-export.md), read 2026-11-13.
