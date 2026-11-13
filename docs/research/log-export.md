# Log export research

Research for [#107](https://github.com/heavymedl-labs/tetsu/issues/107), following
[ADR-0010](../adr/0010-evaluating-technology.md) steps 1, 4, and 7. This is research, not a decision. Where it
disagrees with an ADR, the ADR wins.

## Criteria

Written 2026-11-13, before reading the crates.

| # | Criterion | Why |
| --- | --- | --- |
| M1 | Any backend that takes OTLP replaces Grafana Cloud through settings alone | [ADR-0003](../adr/0003-architecture.md), [ADR-0045](../adr/0045-operations-baseline.md) |
| M2 | Exported lines carry a request's ID, realm, route, and tenant, and nothing a reviewer has not listed | ADR-0045 |
| M3 | A failing or misconfigured export never fails, slows, or loops requests or the dispatcher | Logs may be lost; requests may not |
| M4 | Runs on the free host, 0.1 CPU and 512 MB, without a collector beside it | [ADR-0012](../adr/0012-first-hosting-and-deploys.md) |
| M5 | Maintained, permissive license, no unpatched advisory | [ADR-0028](../adr/0028-dependency-checks-revised.md) |
| W1 | Alert rules move between backends with little work | M1 |
| W2 | Cost to replace | Should touch only the logging setup |

## Options

- **`opentelemetry-appender-tracing`**, the OpenTelemetry project's bridge from `tracing` events to OTLP logs.
- **A homegrown layer** over the `opentelemetry` logs API. Feasible, about the size of the bridge's `layer.rs`.
- **Shipping standard output through the host.** Render streams logs over syslog or HTTPS to a provider's
  endpoint ([Render, Log streams](https://render.com/docs/log-streams), read 2026-11-13). That ties export to the
  host, and Grafana Cloud documents only OTLP over HTTP for direct ingest
  ([Grafana, Send data using OTLP](https://grafana.com/docs/grafana-cloud/send-data/otlp/send-data-otlp/), read
  2026-11-13), so it needs a collector the free host cannot run.

## Project health

Read on crates.io and GitHub on 2026-11-13. RustSec lists no advisory for any of them.

| Crate | Latest | Downloads, 90 days | License | Rust |
| --- | --- | --- | --- | --- |
| `opentelemetry-appender-tracing` | 0.33.1, 2026-10-09 | 12 million | Apache-2.0 | 1.75 |
| `reqwest`, through `opentelemetry-otlp` | 0.13.5, 2026-09-08 | 264 million | MIT or Apache-2.0 | 1.85 |

- The bridge ships from [opentelemetry-rust](https://github.com/open-telemetry/opentelemetry-rust) with the crates
  ADR-0045 adopted, on the same release train. In the last 90 days, 30 people made 107 commits there, besides
  bots, and two made 58 of them. It has 221 open issues, 14 of them about logs.
- `experimental_span_attributes` first shipped in 0.32.0, on 2026-05-08, per the bridge's `CHANGELOG.md`, and
  carries no stability promise.
- `opentelemetry-otlp` reads `OTEL_EXPORTER_OTLP_HEADERS` from the process environment always, and lets an endpoint
  passed in code override the environment's (`src/exporter/http/mod.rs`). Unset, the endpoint falls back to
  `http://localhost:4318`.
- The bridge exports every event field and the message, plus the event's target and name. Its allowlist covers
  span fields only (`src/layer.rs`).

## Findings

- **The bridge meets M2 to M5,** with an allowlist of span fields and a filter on the export layer.
- **M1 holds for backends that take OTLP over HTTP with header auth.** One that signs each request, such as AWS
  CloudWatch, takes a collector in between, which is a deploy change, not a code change.
- **W1 needs a stable field.** Rules that match message text break when a message is reworded, and a query language
  is per backend. A field naming the alert makes each rule a filter and a count.

## Weakest evidence

`experimental_span_attributes` is six months old with no stability promise. The fallback is the homegrown layer,
which a test of the exported fields would catch the need for.
