# Login libraries: failure tests

Step 6 of [ADR-0010](../../adr/0010-evaluating-technology.md) for [ADR-0047](../../adr/0047-sign-in-libraries.md),
run on 2026-11-14 for [#109](https://github.com/heavymedl-labs/tetsu/issues/109): webauthn-rs 0.6.2 and openidconnect
4.1.0 against hostile logins, in a throwaway crate on Rust 1.99.1. The spike's code is gone; slice 5 rewrites the
tests that matter in Tetsu.

## Method

- **Passkeys:** 1Password's software authenticator (passkey-rs 0.6.0, with its pure-Rust crypto) made real
  registrations and logins against a webauthn-rs relying party for `tetsu.test`, with user verification required.
  A hostile client was played by editing the options the server sent before the device saw them, and by writing the
  signed client data by hand so the device signed for any origin. passkey-rs writes binary fields as number arrays
  unless its `serialize_bytes_as_base64_string` feature is on; with it, its JSON matched webauthn-rs's as a
  browser's does.
- **Google:** ID tokens signed with a local RSA key standing in for Google's, checked by openidconnect's verifier
  for a confidential client, with Google's issuer and Tetsu's client ID.

## Results

| Attack | Result |
| --- | --- |
| Registration where the device skipped user verification | Refused: `UserNotVerified` |
| Login where the device skipped user verification | Refused: `UserNotVerified` |
| Signed for another port (`:8443`), a subdomain, plain `http`, a longer domain, or a lookalike punycode domain | Refused: `InvalidRPOrigin` |
| A login replayed against the challenge it answered | **Accepted** |
| The same login against a fresh challenge | Refused: `MismatchedChallenge` |
| A cloned device whose signature counter is behind the stored one | Refused: `CredentialPossibleCompromise` |
| A synced passkey whose counter is always 0, logging in three times | Accepted each time |
| One account's passkey offered for another account | Refused: `CredentialNotFound` |
| The same person's passkey from another business, offered in a discoverable login and finished with this business's keys | Refused: `CredentialNotFound` |
| The same, finished with the keys of the account its user handle names, wherever that account is | Accepted |
| A Google token for another client | Refused: `InvalidAudience` |
| A Google token for another login attempt | Refused: `InvalidNonce` |
| A Google token from a lookalike or `http` issuer | Refused: `InvalidIssuer` |
| An expired Google token | Refused: `Expired` |
| A Google token signed by another key, with altered claims, or with `alg: none` | Refused: signature failure |
| A genuine token whose `iss` is the bare `accounts.google.com`, a form Google documents | Refused: the issuer does not parse as a URL |

The dependency tree has no OpenSSL with webauthn-rs's defaults plus `conditional-ui`, and openidconnect with no
features, its defaults, or `reqwest` with `rustls-tls`.

## What Tetsu must do itself

- **Each challenge is used once.** webauthn-rs keeps no record of which challenges it has seen, so a login replayed
  against a challenge the server still holds succeeds. The server stores each challenge with its attempt, deletes it
  on first use, and expires it, so a replay meets a challenge that no longer exists.
- **A backwards counter is a refusal and a security event.** webauthn-rs refuses it; Tetsu records it in the audit
  log, and stores the counter from each successful login.
- **A discoverable login looks its credential up within the business.** The library names the account a passkey
  belongs to and accepts whatever keys it is handed, so the lookup is scoped to the realm and business before the
  key reaches it. `finish_discoverable_authentication` is deprecated for `_v2`, which takes the one credential.
- **Customer login needs webauthn-rs's `conditional-ui` feature,** where discoverable login lives, against
  ADR-0047's "no optional features".
- **Storing a login's state between requests** in Postgres needs webauthn-rs's
  `danger-allow-state-serialisation`; keeping it in memory does not, but loses logins in progress on restart.
  Slice 5 decides. (Note: ADR-0047, accepted on 2026-11-14, chose Postgres.)

## Follow-up checks

- **Google's issuer, for a code flow:** Google's discovery document declares `https://accounts.google.com`, signed
  with RS256 (read 2026-11-14), and a code flow verifies against the discovery document's issuer. The bare form
  belongs to Google's older Sign-In flows, so Tetsu should not meet it; if it did, the login fails closed.
- **Advisories:** openidconnect has none in OSV. webauthn-rs has one, GHSA-22w3-693w-x895, rated low: with subdomains
  allowed, an origin was matched to the relying party ID by suffix without a dot, so `hermit-crab.example` could
  pass for `crab.example`. Tetsu never allows subdomains (ADR-0040), and 0.5.5 and the 0.6 line carry the fix. The
  maintainers published it themselves, a week after the report, and openSUSE shipped it in its Kanidm packages.
- **The SUSE audit** the README cites is still unpublished; Kanidm's maintainers describe it as covering the
  security-critical and cryptographic parts.

## Weakest evidence

- The software authenticator is not a phone or a security key. Platform quirks wait for the device test in slice 9.
- The Google tokens were signed locally; slice 10 logs in against Google itself.

## Sources

- [webauthn-rs 0.6.2](https://docs.rs/webauthn-rs/0.6.2), its source for the discoverable flow and features, read
  2026-11-14.
- [passkey-rs 0.6.0](https://github.com/1Password/passkey-rs), README and source, read 2026-11-14.
- [openidconnect 4.1.0](https://docs.rs/openidconnect/4.1.0), read 2026-11-14.
- Google, [OpenID configuration](https://accounts.google.com/.well-known/openid-configuration), read 2026-11-14.
- [GHSA-22w3-693w-x895](https://github.com/kanidm/webauthn-rs/security/advisories/GHSA-22w3-693w-x895), and
  [OSV](https://osv.dev) for webauthn-rs, webauthn-rs-core, webauthn-rs-proto, and openidconnect, read 2026-11-14.
- [Is Kanidm audited and used in prod?](https://github.com/kanidm/kanidm/discussions/2584), Kanidm discussion #2584.
- W3C, [Web Authentication Level 3](https://www.w3.org/TR/webauthn-3/), sections 7.2 (assertions) and 6.1.1
  (signature counter).
- Google, [Verify the Google ID token on your server side](https://developers.google.com/identity/gsi/web/guides/verify-google-id-token):
  `iss` is `accounts.google.com` or `https://accounts.google.com`, read 2026-11-14.
