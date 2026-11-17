# 0058. A virtual authenticator for login tests

- Status: Accepted
- Date: 2026-11-14

## Context

[ADR-0040](0040-identity-and-sign-in.md) has tests log in with a virtual authenticator, in test code only, so no
code bypasses login. Tier 1 tests for passkey enrollment and login need one that makes real WebAuthn
registrations and assertions, and lets a test play a hostile client, as the
[failure tests](../research/identity/library-failure-tests.md) did. A test-only dependency is cheap to replace and
on no hot path, so [ADR-0010](0010-evaluating-technology.md)'s steps 1, 4, and 7 apply.

**Criteria.** Makes registrations and assertions that [ADR-0047](0047-sign-in-libraries.md)'s `webauthn-rs`
verifies, with user verification; lets a test edit the options and the signed client data; pure Rust, with no
OpenSSL; maintained, under a license `.config/deny.toml` allows; and independent of `webauthn-rs`, so a bug the two
share cannot hide.

## Decision

**Use [passkey-rs](https://github.com/1Password/passkey-rs)** (the `passkey` crate and its parts), as a
dev-dependency of the crates that test login, never a dependency of the binary.

| Read on crates.io and GitHub, 2026-11-14 | |
| --- | --- |
| Latest | 0.6.0, 2026-10-01; MIT or Apache-2.0; Rust 1.87 |
| Use | 1.3 million downloads in 90 days for `passkey-authenticator` |
| Activity | 37 commits by 5 authors in 90 days; last push 2026-11-03 |
| Advisories | None in OSV for `passkey`, `passkey-authenticator`, `passkey-client`, or `passkey-types` |
| Maintainer | 1Password, which ships it in its own passkey support |

- **It passed the failure tests' use already:** real registrations and logins against `webauthn-rs`, with user
  verification required, and a hostile client played by editing the options and writing the client data by hand.
  Its pure-Rust crypto added no OpenSSL.
- **Its JSON matches a browser's** with the `serialize_bytes_as_base64_string` feature, which the tests turn on.
- **It stays in test code.** The binary never links it, so it cannot become a way around login in any environment
  (`v5.0.0-6.3.4`).

## Alternatives considered

- **`webauthn-authenticator-rs`'s soft passkey,** from the `webauthn-rs` project (0.6.2, MPL-2.0). Maintained and
  pure Rust, but written alongside the verifier it would test, so a misreading the two share would pass.
- **A browser's virtual authenticator** through Playwright. Right for the browser tests
  [ADR-0054](0054-browser-tests.md) proposes, but too slow and too far from the code for Tier 1 unit and
  integration tests, and it cannot easily forge client data.
- **Writing one in Tetsu.** CBOR, COSE keys, and signing are where such code goes wrong, the same reason ADR-0047
  declined writing the verifier.

## Consequences

- Login tests enroll and log in with real passkeys, and play hostile clients, without a browser.
- `cargo deny` sees the new test dependencies, and their licenses are already allowed.
- Revisit if passkey-rs stops releasing for a year, or an advisory goes unfixed for 30 days.

## Sources

- [passkey-rs](https://github.com/1Password/passkey-rs), its README and releases, and
  [passkey-authenticator on crates.io](https://crates.io/crates/passkey-authenticator), read 2026-11-14.
- [OSV](https://osv.dev) for the four `passkey` crates, read 2026-11-14.
- [webauthn-authenticator-rs on crates.io](https://crates.io/crates/webauthn-authenticator-rs), read 2026-11-14.
- [Login libraries: failure tests](../research/identity/library-failure-tests.md), 2026-11-14.
