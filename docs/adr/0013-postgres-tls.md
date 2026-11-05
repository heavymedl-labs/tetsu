# 0013. Postgres TLS

- Status: Accepted
- Date: 2026-11-04

## Context

Neon rejects unencrypted connections ([ADR-0012](0012-first-hosting-and-deploys.md)), and
`tokio-postgres` needs a separate TLS crate. [ADR-0004](0004-security.md) requires an ADR for each
cryptography crate. The evidence is in [Postgres TLS research](../research/postgres-tls.md).

## Decision

Use `tokio-postgres-rustls` with `rustls` and its default `aws-lc-rs` provider.

- **Every connection sets `SslMode::Require`.** The driver defaults to `Prefer`, which connected without
  encryption in our test when the server offered no TLS. Only a local test configuration, set
  explicitly, may turn it off.
- **Certificates are verified.** Trust comes from the public roots in `webpki-roots`, with an optional
  certificate authority file from the environment. No code disables verification.
- **Channel binding is required where the provider supports it.** This is on for Neon, once tested.
- **`rustls` stays current.** Five of its advisories appeared in 2026, so we run at least 0.23.46 and take
  updates promptly. [#31](https://github.com/heavymedl-labs/tetsu/issues/31) adds the checks that enforce this.
- **We have a fallback for `tokio-postgres-rustls`.** We copy it into this repository, which its MIT license
  allows, if it blocks a `rustls` security update or new major version for 30 days, or if a trigger in
  [ADR-0011](0011-server-stack-guardrails.md) fires.
- **CI runs the connection tests.** The failure cases in the research page become tests once code exists, so
  an update or a copied version is checked the same way.

## Alternatives considered

- **`postgres-native-tls` with OpenSSL.** Equally strict in our test and from the driver's own project,
  with a smaller binary. It links system OpenSSL, so the image must carry and patch those libraries,
  and it behaves differently on a Mac and on Linux.
- **`rustls` with the `ring` provider.** Avoids AWS-LC's C build, but `ring`'s last release was in
  March 2025, and it is not the default.
- **`tokio-postgres-rustls-improved`.** A similar crate with far fewer downloads, not evaluated further.
- **Our own adapter on `tokio-rustls`.** Removes the dependency on one maintainer, but we would own
  security-sensitive glue from day one, including channel binding. The fallback keeps this option open.

## Consequences

The image needs no system TLS libraries, at a cost of about 2.6 MB in our test binary. Pool setup is the
only code that names these crates, so replacing them is cheap.

**Weakest evidence.** One local run on a private certificate authority, not Neon. Expired certificates and
sending the server name are untested, so the first deploy runs the connection tests. `rustls` has had
several recent advisories, which shows active scrutiny and also a need to update.

`tokio-postgres-rustls` relies largely on one maintainer, and its last release gap was about 19 months.
The risk is narrow: it holds no cryptography, and `rustls` patch releases reach us without a new release of
it. **Revisit** if the fallback is used, or if `tokio-postgres` gains built-in TLS. The
ADR that chooses a paid setup must settle Render's self-signed internal certificates.

## Sources

- [Postgres TLS research](../research/postgres-tls.md): failure tests, build notes, and project health,
  2026-11-04.
- [Neon: connect securely](https://neon.com/docs/connect/connect-securely) and
  [connection errors](https://neon.com/docs/connect/connection-errors).
- `tokio-postgres` documentation, docs.rs/tokio-postgres: `SslMode`.
- RustSec advisory database, rustsec.org.
