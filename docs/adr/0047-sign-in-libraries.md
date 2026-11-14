# 0047. Sign-in libraries

- Status: Proposed
- Date: 2026-11-11

## Context

[ADR-0040](0040-identity-and-sign-in.md) has Tetsu build its own sign-in from mature crates and asks for an ADR
before each is adopted. Sign-in is Tier 1 and expensive to replace once passkeys exist, so
[ADR-0010](0010-evaluating-technology.md)'s full process applies. The [identity research](../research/identity/build-vs-buy.md)
covered the options and community practice.

**Criteria.**

- Verify WebAuthn registration and authentication to Level 3, with user verification required, exact origins, and
  attestation checks for operators' hardware keys.
- Speak OpenID Connect to Google, with state, nonce, and PKCE checked.
- Pure Rust, with no system OpenSSL, as [ADR-0013](0013-postgres-tls.md) chose for TLS.
- Maintained, with a license Tetsu may ship.

## Decision

| Need | Crate | Read on crates.io, 2026-11-11 |
| --- | --- | --- |
| Passkeys | `webauthn-rs` 0.6, with `conditional-ui` alone for customers' discoverable sign-in | 0.6.2, 2026-10-31; 3.4 million downloads in 90 days; MPL-2.0; Rust 1.89 |
| Google sign-in | `openidconnect` 4 | 4.1.0, 2026-10-31; 5.7 million; MIT; Rust 1.88 |
| Random tokens and codes | `getrandom` 0.4 | 0.4.3; 795 million; MIT or Apache-2.0 |
| Hashing tokens and codes | `sha2` 0.11 | 0.11.1; 375 million; MIT or Apache-2.0 |
| Comparing secrets | `subtle` 2.6 | 2.6.1; 230 million; BSD-3-Clause |

- **Sessions, forgery protection, rate limits, and email codes are written in Tetsu,** on Postgres. Each is a few
  hundred lines that the security review reads in full, and session crates add a layer between Tetsu and the rules
  ADR-0040 sets.
- **Tokens and codes are hashed with SHA-256 before storage.** They carry at least 112 bits, the strength at which
  `v5.0.0-6.5.2` lets a standard hash protect a secret, as ADR-0041 does for recovery codes. Email codes, shorter by design, are guarded by single use, expiry,
  binding to the attempt, and rate limits, as ADR-0040 sets.
- **`webauthn-rs` stays behind the sign-in port,** so its types never reach other modules, and a replacement keeps
  every passkey, which belongs to its relying party ID, not to the library.
- **`.config/deny.toml` allows MPL-2.0.** It is file-level copyleft: Tetsu may ship the crate unmodified in a closed
  binary, and only changes to its own files would have to be shared. This is the one licence change, and the
  maintainer approves it with this ADR.
- **The failure tests ran before acceptance** ([results](../research/identity/library-failure-tests.md), step 6 of
  ADR-0010). Responses without user verification, from near-miss origins, or for another account were refused,
  as were Google tokens with a wrong audience, nonce, or issuer, expired, or forged, and no OpenSSL entered the
  tree. They also showed what the libraries leave to Tetsu:
  - **Each challenge is used once.** webauthn-rs accepts a sign-in replayed against a challenge the server still
    holds, so Tetsu stores each with its attempt, deletes it on first use, and expires it.
  - **A signature counter that goes backwards** is refused, as webauthn-rs does, and recorded in the audit log;
    Tetsu stores the counter after each sign-in. Synced passkeys, whose counter stays at zero, are unaffected.
  - **A discoverable sign-in finds its credential within the realm and business** before webauthn-rs sees it,
    since the library accepts whichever key it is given. Tetsu calls `finish_discoverable_authentication_v2`.
  - **Google's bare issuer,** `accounts.google.com`, which Google documents beside the URL form, does not parse in
    openidconnect. A code flow uses the discovery document's issuer, `https://accounts.google.com`, so Tetsu should
    not meet it, and such a token fails closed.
  - Keeping a sign-in's state in Postgres between requests needs webauthn-rs's `danger-allow-state-serialisation`;
    slice 5 decides between that and memory.

## Alternatives considered

- **`passkey-rs`.** Mostly an authenticator and client library, not a relying party server.
- **Writing WebAuthn verification in Tetsu.** CBOR, COSE keys, and attestation formats are where verification bugs
  live, and `webauthn-rs` has years of production use, including Kanidm.
- **`tower-sessions` or `axum-login`.** Useful glue, but they add a layer the review must read anyway, and
  ADR-0040's sessions carry realm, identity, and business in ways they do not model.
- **A hosted identity provider.** The identity research's fallback, which ADR-0040 declined.

## Consequences

- The weakest evidence is `webauthn-rs`'s audit: its README cites a SUSE review, but no public report was found.
  The failure tests and Tetsu's own Tier 1 tests carry more weight for that reason. Its one advisory, rated low,
  affects only relying parties that allow subdomains, which Tetsu does not, and was fixed and disclosed by its
  maintainers within a week.
- `webauthn-rs` 0.6 replaced OpenSSL with pure Rust cryptography through `crypto-glue`; the spike's dependency tree
  had no OpenSSL.
- The spike used a software authenticator, not a phone or security key; slice 9's device test covers those.
- Revisit if `webauthn-rs` stops releasing for a year, or an advisory goes unfixed for 30 days.

## Sources

- [webauthn-rs](https://crates.io/crates/webauthn-rs), [openidconnect](https://crates.io/crates/openidconnect),
  [getrandom](https://crates.io/crates/getrandom), [sha2](https://crates.io/crates/sha2), and
  [subtle](https://crates.io/crates/subtle) on crates.io, read 2026-11-11.
- W3C, [Web Authentication Level 3](https://www.w3.org/TR/webauthn-3/), Recommendation, 25 August 2026.
- Mozilla, [MPL 2.0 FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/).
- [Identity research: building in Rust](../research/identity/build-vs-buy.md).
- [Sign-in libraries: failure tests](../research/identity/library-failure-tests.md), 2026-11-14.
