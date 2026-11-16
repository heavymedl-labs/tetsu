# Passkey sign-in: a practice review

Research on 2026-11-15 for [#110](https://github.com/heavymedl-labs/tetsu/issues/110), step 4c (commits `94a1a66`
and `654f45d`): signing an operator in with a discoverable passkey, and adding their second passkey from a fresh
sign-in. Two reviews ran in parallel, one on WebAuthn practice and one on the database, each pinning races in a
scratch database. The changes marked "now" land in the commit that adds this page; the rest feed the steps named.

## Summary

The cryptographic core follows the specification. The modal prompt names no passkey, user verification is
required, the user handle must be the credential's operator's, and `webauthn-rs` checks the counter and backup
flags. The new counter is stored only if no other sign-in changed it, in the transaction that starts the session.
No pinned race gave an operator a third passkey or let a disabled operator in.

The reviews found two ways to defeat the slice's own rules. Identity's role could start a session without a
passkey, which counted as a fresh sign-in and so could add one. Anyone could fill the cap on open sign-ins and
lock every operator out. They also found rules from ADR-0047 and ADR-0059 the code did not yet keep.

## Ranked changes at a glance

| Rank | Severity | Change | When |
| --- | --- | --- | --- |
| 1 | High | Past the limit of open sign-ins, end the oldest rather than refuse; raise the limit to 10,000 | Now |
| 2 | High | Adding a passkey ends the operator's other sessions and keeps this one (ADR-0059) | Now |
| 3 | High | Record every refused sign-in after its ceremony is taken, with its reason | Now |
| 4 | Medium | Identity's role may not call `start_session`; only `sign_in` starts a session | Now |
| 5 | Medium | One add ceremony per operator by a unique index; only the session that began it finishes it | Now |
| 6 | Medium | Test the signature counter and backup eligibility | Now |
| 7 | Low | A passkey removed while a sign-in waited is `unknown`, not `raced` | Now |
| 8 | Low | TD001 can no longer reach Rust; it surfaces as a storage fault | Now |
| 9 | Medium | End the session a browser presents when it signs in (ADR-0059, `v5.0.0-7.2.4`) | Step 4d |
| 10 | Medium | Show recent security events at sign-in, in place of a notice independent of the binding | Step 4d |
| 11 | Medium | Add the session's public ID, address, user agent, BE, BS, and UV to events; store transports | Step 4d |
| 12 | Low | Signal API after sign-in and adding; no `signalUnknownCredential` while realms share an RP ID | Step 4d |
| 13 | Medium | Rate limits on sign-in begins as well as failures, per address | Step 5 |
| 14 | Low | A step-up assertion in place of session age for adding a passkey | Level 2 review |

## Anyone could lock operators out

Beginning a sign-in needs no account, and each stored a ten-minute ceremony. At a thousand open, begins were
refused. The database review filled the table in a fraction of a second, after which a real begin failed. About
two requests a second kept it full, and no sign-in ever failed, so ADR-0048's limit on failed sign-ins would never
fire. ADR-0048 says Tetsu "slows down rather than locks out" (`v5.0.0-6.3.1`), and OWASP warns that lockouts
become denial of service
([Authentication Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html)).

Begin now ends the oldest open sign-ins past ten thousand, skipping rows other transactions hold, and never
refuses. An attacker must now outpace a person touching their authenticator, not merely fill a table. Concurrent
begins may overshoot the bound by the number of connections. A fixed pool of slots would give a hard bound; this
is enough until step 5. Step 5 should limit begins per address too, which ADR-0048's table does not yet list.
Sealing the state into the ceremony cookie would remove the unauthenticated write entirely, but ADR-0047 keeps
state on the server.

## Adding a passkey left other sessions running

ADR-0059 ends an account's other sessions, keeping the current one, when a passkey is added (`v5.0.0-7.4.3`).
`add_passkey` stored the passkey and stopped there. It now deletes the operator's other sessions under the same
row lock, and `identity.passkey_added` records how many live ones ended.

## Refusals vanished from the audit record

Only outcomes that reached the final SQL step were recorded. A counter that went back, a changed backup
eligibility, failed user verification, an unknown credential, and a mismatched user handle all failed earlier,
unrecorded, and their reasons were thrown away. ADR-0047 says a counter that goes back is "recorded in the audit
log". ASVS 5.0 16.3.1 asks for every authentication attempt
([V16](https://raw.githubusercontent.com/OWASP/ASVS/v5.0.0/5.0/en/0x25-V16-Security-Logging-and-Error-Handling.md)),
and WebAuthn L3 §6.1.1 treats a counter that does not increase as a sign of a cloned authenticator
([WebAuthn L3](https://www.w3.org/TR/webauthn-3/)).

The answer is now read once, and each refusal after the ceremony is taken records `identity.signed_in` with its
reason, plus the operator and credential when known. The browser still sees one answer. A token that finds no
ceremony records nothing, since it costs an attacker nothing to send; step 4d adds the address, and step 5 the
limits.

## A session without a passkey counted as fresh

Adding a passkey trusts any session begun within ten minutes. Identity's role could call `start_session` directly,
and the web app can take that role. The database review started a session, then added an attacker's passkey to
the operator, all through the role. Only `sign_in`, which runs as the function's owner, needs it, so the grant is
gone, along with the use case that only tests called. Tests now start sessions as the test server's administrator.
A session begun within ten minutes is a fresh passkey sign-in by construction (ADR-0048, `v5.0.0-7.5.1`).

Sources disagree on whether session age is enough. NIST SP 800-63B-4 §4.1.2.1 asks for authentication at the
account's level before binding, without saying whether an existing session counts
([events](https://pages.nist.gov/800-63-4/sp800-63b/events/)). ASVS 7.5.1 asks for "full re-authentication"
without defining it. The OWASP cheat sheet suggests asking for a bound passkey. A step-up assertion inside the
session would also defeat a stolen cookie within the window; it belongs with the Level 2 review.

## Two begins could store two add ceremonies

Two concurrent `begin_addition` calls both found nothing to replace and both inserted, because `holding` takes a
share lock. A unique index on the operator's add ceremony makes the second wait and replace the first. The
ceremony also records the session that began it, so another of the operator's sessions cannot finish it, and
ending that session leaves nothing it could use.

## Counters and backup eligibility were untested

passkey-rs's test authenticator keeps no counter by default, like a synced passkey, so no test covered the counter
moving forward or going back. Tests now sign in with a counter ahead, then with the original passkey, which is
refused as `counter_regressed`; and with the stored backup eligibility flipped, refused as
`backup_eligibility_changed`. `webauthn-rs` refuses both for good. The remedy is `operator remove-passkeys` and a
new link, now diagnosable from the record.

The compare-and-swap on the whole passkey stays. A synced passkey's counter stays at 0, so its new state equals its
old and two sign-ins at once both succeed. Only a real change underneath refuses, which WebAuthn names as a race or
a clone. jsonb equality ignores key order and number spelling, and the old value is the exact text `signer` read,
never re-serialised.

## Smaller fixes

- When `remove_passkeys` held the operator's row, a waiting sign-in found its passkey gone at the update and said
  `raced`. It now says `unknown`. A pinned test holds the removal open and checks.
- TD001 is raised only by `start_session`, which only `sign_in` calls while holding an enabled operator's row, so
  it can no longer reach Rust. Its mapping to "invalid" is gone; were it raised, it would be a storage fault.

## For later steps

- **Step 4d:**
  - On sign-in, end the session the browser presents in the same transaction (ADR-0059, `v5.0.0-7.2.4`). A new
    token is already issued each time, so fixation is prevented.
  - Show recent security events at the next sign-in. NIST 800-63B-4 asks for a notice independent of the binding
    transaction, which ADR-0057 defers with email; record this as a gap for the Level 2 review.
  - Add the session's public ID, never its hash, to `signed_in` and `passkey_added`, with the address, a coarse
    user agent, BE, BS, UV, and the AAGUID. Store BE and BS per passkey, which also inventories synced operator
    passkeys for ADR-0057's gate. Store transports and pass them in `excludeCredentials`.
  - Call `signalAllAcceptedCredentials` with the full list, never a partial one, and `signalCurrentUserDetails`
    after sign-in and adding, feature-detected. Do not call `signalUnknownCredential` while realms share an RP ID
    (ADR-0061), since a passkey unknown here may be valid in another realm.
  - Apply ADR-0059's Origin and `Sec-Fetch-Site` checks to sign-in begin and finish, the only defence against
    login CSRF. Begin from a button, carry the ceremony token in a `__Host-` cookie or the body, and cancel a
    pending prompt before starting another.
- **Step 5:** rate limits per address on sign-in begins and failures.
- **Level 2 review:** a step-up assertion for adding a passkey; a fixed slot pool if begins need a hard bound.

## Note added after review

A doubt cycle on 2026-11-16 found that this page's "now" changes overclaimed. Revoking `start_session` did not
make a fresh session a passkey sign-in by construction: Identity's role could still start one through
`sign_in`, and swap an operator's key with it, and `add_passkey` trusted its caller for freshness. `sign_in` now
refuses a new state that changes more than the counter and backup flags, and checks the operator Rust checked;
`add_passkey` uses up an addition its session took; and user verification and backup eligibility, which
`webauthn-rs` checks before the signature, are recorded as `not_verified`. The role can still start a session,
which stays a matter of the shared login (ADR-0061). Identity's README has the current rules.
