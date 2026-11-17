# 0071. Readable logs in a terminal

- Status: Proposed
- Date: 2026-11-17
- Supersedes the log format rule in [ADR-0045](0045-operations-baseline.md)

## Context

[ADR-0045](0045-operations-baseline.md) writes logs as one JSON object a line on standard output, which every host
collects and the OTLP export reads. A person running `cargo tetsu serve` reads the same lines in a terminal, where
JSON is hard to scan. `tracing-subscriber`, which Tetsu already uses, has layouts for both built in: JSON for
machines to read, and text layouts for people
([tracing-subscriber](https://docs.rs/tracing-subscriber/0.3.23/tracing_subscriber/fmt/index.html)).

## Decision

- **Logs are JSON, unless standard output is a terminal,** where they are the same lines in
  `tracing-subscriber`'s compact text layout. A host, a container, or a pipe gets JSON without setting anything.
- **`TETSU_LOG_FORMAT` chooses,** `json` or `text`, for when the default is wrong, such as checking locally what a
  host collects. Any other value fails at start, as other settings do.
- **Only the layout differs.** Both layouts carry the same fields through the same filters, and the OTLP export is
  unchanged, so text cannot show what JSON would not.
- **The filters stay fixed.** No `RUST_LOG` or similar setting turns on other crates' lines, since no one has
  checked what they write for personal details.
- **Text has no color,** which would add a crate for little gain.

## Alternatives considered

- **JSON everywhere, read through a tool such as `jq`.** Every person running Tetsu needs the tool and the habit.
- **Text whenever `TETSU_DEMO` is unset or a debug build runs.** Neither says where the lines go; a terminal does.
- **The pretty layout,** several lines an event. It is easier to read alone but harder to scan in a busy log.

## Consequences

- A person running Tetsu reads plain lines, such as `INFO request: served status=200 realm="operator"`.
- A test checks that text lines carry the span's and the event's fields, and that the setting is read as above.

## Sources

- tokio-rs, `tracing-subscriber` 0.3.23, module `fmt`,
  <https://docs.rs/tracing-subscriber/0.3.23/tracing_subscriber/fmt/index.html>.
