# Postgres TLS research

Research for [#29](https://github.com/heavymedl-labs/tetsu/issues/29), following
[ADR-0010](../adr/0010-evaluating-technology.md). This is research, not a decision. Where it
disagrees with an ADR, the ADR wins.

## Criteria

Written 2026-11-04, before looking at results.

| # | Criterion | Why |
| --- | --- | --- |
| M1 | Works with `tokio-postgres` and `deadpool-postgres` | [ADR-0007](../adr/0007-server-stack.md) |
| M2 | Fails closed on a wrong certificate authority, wrong hostname, or a server without TLS | Tenant data crosses this connection |
| M3 | Sends the server name (SNI), which Neon uses for routing | [ADR-0012](../adr/0012-first-hosting-and-deploys.md) |
| M4 | Can trust a specific certificate authority | Providers use different roots, and Render's internal certificates are self-signed |
| M5 | Vetted and maintained, with no unpatched advisory | [ADR-0004](../adr/0004-security.md) |
| W1 | Builds simply in our image, without system libraries where possible | One binary in an OCI image ([ADR-0003](../adr/0003-architecture.md)) |
| W2 | Connection setup and query cost | Cached statements mean connections are long-lived, so this matters less than safety |
| W3 | Cost to replace | Should touch only pool setup |

It is hard to reverse only in a small way and is on the connection path, so this follows the
short process plus a failure test, since M2 is about how it fails.
