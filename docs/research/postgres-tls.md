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

## Findings

**Use `tokio-postgres-rustls`, and always set `SslMode::Require`.** Both crates failed closed on bad
certificates. The bigger risk is the driver's default, not the choice of crate.

### Failure tests

Run 2026-11-04 on Linux in containers, once, with `tokio-postgres` 0.7.18, PostgreSQL 18.6, and a private
certificate authority. This did not run against Neon.

| Case | rustls | native-tls (OpenSSL) |
| --- | --- | --- |
| Right authority, `Require` | Connected with TLS | Connected with TLS |
| Wrong authority, `Require` | Refused | Refused |
| Wrong hostname, `Require` | Refused | Refused |
| Channel binding required | Connected with TLS | Connected with TLS |
| Through `deadpool-postgres` | Connected with TLS | Connected with TLS |
| Server without TLS, `Require` | Refused | Refused |
| **Server without TLS, `Prefer`** | **Connected, unencrypted** | **Connected, unencrypted** |
| Wrong authority, `Prefer` | Refused | Refused |

`Prefer` is the driver's default, as the test printed. So a connection that does not set a mode silently
runs without encryption if something strips TLS or a server lacks it. Certificates are checked by the TLS
crate, so `Require` here does verify them, unlike libpq's `sslmode=require`.

Expired certificates were not tested. Neither was sending the server name to Neon, which
[Neon's docs](https://neon.com/docs/connect/connection-errors) say it needs for routing. The connection
test in the [hosting research](hosting.md#hosted-tests-still-to-run) covers that at first deploy.

### Build and cost

| | rustls | native-tls |
| --- | --- | --- |
| Links system OpenSSL | No | Yes, `libssl` and `libcrypto`, so the image must carry them |
| Stripped test binary | 4.4 MB | 1.8 MB, plus those libraries |
| Crypto underneath | AWS-LC, through `aws-lc-rs` | OpenSSL on Linux, and the system's library on a Mac |
| Query over TLS | 110 µs | 95 µs |

Plain queries took 82 µs on the same loopback. One run is too noisy to separate the two crates, and a real
network will dominate. Connection timing is omitted, because the test rebuilt each connector per connection.

### Project health

Read 2026-11-04 from crates.io, GitHub, and RustSec.

| Crate | Version | License | Recent downloads | Advisories |
| --- | --- | --- | --- | --- |
| `tokio-postgres-rustls` | 0.14.0, May 2026 | MIT | 7.2M | None |
| `rustls` | 0.23.46, Oct 2026 | Apache-2.0, ISC, or MIT | 267M | Five in 2026, all patched by 0.23.46 |
| `aws-lc-rs` | 1.19.0, Oct 2026 | Not read | 114M | None under this name |
| `aws-lc-sys` | 0.47.0 tested | Not read | Not read | Five in 2026, all patched by 0.39.0 |
| `postgres-native-tls` | 0.5.3, Mar 2026 | Not read | 5.1M | None |
| `native-tls` | 0.2.18, Feb 2026 | Not read | 68M | None |
| `openssl` | 0.10.82, Nov 2026 | Not read | 83M | Several over the years |

`tokio-postgres-rustls` depends heavily on one maintainer: 54 of its commits, and all 12 in the last year.
The risk is narrow. Its `src/lib.rs` is 416 lines including docs, forbids `unsafe`, and holds no cryptography
beyond computing the channel binding value. It depends on `rustls` 0.23, so patch releases reach us without a
new release of it, but a new `rustls` major version would have to wait for one. 156 crates depend on it, and
the gap before 0.14.0 was about 19 months. `rustls` needs prompt updates.

### Community practice

Not reviewed beyond download counts, which the short process allows. `rustls` is far more widely
downloaded than `native-tls`.

### What this does not settle

Render's internal certificates are self-signed and do not support `verify-full`, so strict checking there
would need its certificate pinned or the external URL. That belongs to the ADR that chooses a paid setup.
