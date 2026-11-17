# 0052. Grafana from agent sessions

- Status: Accepted
- Date: 2026-11-13

## Context

The demo's logs and alerts live in Grafana Cloud ([ADR-0049](0049-log-export.md)). Checking them, as after a deploy
or when an alert fires, meant driving Grafana's web UI through a browser: slow, fragile, and a browser session with
the maintainer's login. Grafana publishes an MCP server that queries Loki and reads alert rules directly
([#127](https://github.com/heavymedl-labs/tetsu/issues/127)).

Criteria, before looking at options: a session can query the demo's logs and see each alert's state; it cannot
change anything unless the maintainer allows it; the credential reaches only the maintainer's machine, never a
session; and nothing in the product depends on it.

## Decision

- **Use [`mcp-grafana`](https://github.com/grafana/mcp-grafana), Grafana's MCP server,** in Claude Code, run with
  `uvx` at an exact version, `mcp-grafana@2.2.0`, so an update arrives only when someone changes the pin.
- **Read-only twice over:** it runs with `--disable-write`, and logs in as service account `claude-mcp` with the
  **Viewer** role, so a write fails even if the flag is dropped.
- **The maintainer sets it up,** creating the service account's token and registering the server with
  `claude mcp add`, so the token goes from Grafana straight to Claude Code's local settings and never through a
  session. It is not in the repository's `.mcp.json`.
- **It is a tool for operating Tetsu, not part of it.** Nothing in `crates/`, the checks, or CI uses it, and a
  session without it falls back to the web UI.
- Alert rules still change in the web UI, rarely, with ADR-0049's table as their one home.

## Alternatives considered

- **`grafanactl`, Grafana's CLI.** Works with Grafana resources as files, but is young: v0.1.10, released in April
  2026, with 174 stars.
- **Grafana's Terraform provider.** Would keep the alert rules in the repository as code, but means adopting
  Terraform for three rules that rarely change. Worth revisiting if rules multiply.
- **Grafana's HTTP API through `curl`.** No new tool, but every session would rebuild queries by hand and handle
  the token itself.
- **The web UI through a browser.** No setup, and what this replaces.

## Consequences

- `mcp-grafana` was at 2.2.0 on PyPI, released 2026-11-02, and 2.3.0 on GitHub, Apache-2.0, read 2026-11-13. In the
  last 90 days, 63 people made 272 commits there, besides bots; two made half of them. It has 84 open issues and pull
  requests, and Grafana Labs maintains it.
- A new secret: the service account's token, listed in [secrets.md](../operations/secrets.md) and rotated yearly
  ([ADR-0027](0027-secrets-revised.md)).
- Sessions can read every log line Grafana holds, which ADR-0049 keeps free of personal data.
- The tool ties sessions, not Tetsu, to Grafana. Moving backends means dropping it, as ADR-0049 allows.
- Revisit if a session needs to change Grafana often, or the pin falls far behind.

## Sources

- [grafana/mcp-grafana README](https://github.com/grafana/mcp-grafana): tools, `--disable-write`, and RBAC, read
  2026-11-13.
- [mcp-grafana on PyPI](https://pypi.org/project/mcp-grafana/), read 2026-11-13.
- [grafana/grafanactl](https://github.com/grafana/grafanactl) and
  [terraform-provider-grafana](https://github.com/grafana/terraform-provider-grafana), read 2026-11-13.
