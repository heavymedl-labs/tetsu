# Passkey enrollment: a practice review

Research on 2026-11-15 for [#110](https://github.com/heavymedl-labs/tetsu/issues/110), steps 4a and 4b (commits
`2fddaea` and `52750ca`): the `passkeys` and `ceremonies` tables, and enrolling an operator's first passkey from
their link through `webauthn-rs`. Two reviews ran in parallel, one on WebAuthn practice and one on the database.
The changes marked "now" land in the commit that adds this page; the rest feed the steps named.

## Summary

The design follows the W3C specification and `webauthn-rs`'s own guidance. The link is used up only when a
passkey is stored, atomically, under the operator's row lock. Beginning has no side effect on the link. State
stays on the server, keyed by a hashed random token. The relying party is the origin's scheme, host, and port,
and user verification
is required. Pinned races under read committed broke none of the three invariants: one first passkey per link, no
passkey for a disabled operator, and no link for an operator who has one.

The reviews found no high-severity defect reachable through the application. They found one reachable from any
other isolation level, one unbounded table, and several gaps that would cost an operator their account or the
audit record its evidence.

## Ranked changes at a glance

| Rank | Severity | Change | When |
| --- | --- | --- | --- |
| 1 | High | `issue_invite` and `redeem_invite` refuse any isolation but read committed (TD002), as disable does | Now |
| 2 | Medium | One open ceremony per link; begin re-checks the link as it stores; sweep only the operator's ended ceremonies | Now |
| 3 | Medium | A credential ID stored a moment before is a recorded `taken`, not a storage error | Now |
| 4 | Medium | Refuse a registration whose client says it is not discoverable | Now |
| 5 | Medium | Refuse credential IDs and passkeys outside the store's bounds as invalid, not storage | Now |
| 6 | Medium | Ceremonies last 10 minutes, past the browser's 5, but never past the link | Now |
| 7 | Medium | Grant Identity's role an explicit list of functions; revoke every routine from PUBLIC | Now |
| 8 | Low | Label the passkey as an operator's in the account picker | Now |
| 9 | Low | Disable ends the operator's ceremonies; read the clock once per statement; correct the AAGUID note | Now |
| 10 | Medium | Record begin and every refusal, with source, IP, and user agent; store AAGUID, transports, and a label | Step 4d |
| 11 | Low | Tell "start again" from "this link has ended" in the browser | Step 4d |
| 12 | Medium | Login ceremonies: nullable operator, a bounded sweep, an index on `ends_at`, a global cap | Step 4c |
| 13 | Medium | Login: row-locked counter updates, user handle checked against the credential, two passkeys at most | Step 4c |
| 14 | Medium | Rate limits for enrollment begin and finish, in ADR-0048's table | Step 5 |
| 15 | Decided | How an operator with an unusable passkey gets back in: `operator remove-passkeys` | Done |

## Isolation the functions trust but do not check

`issue_invite` refuses an operator who holds a passkey by reading `passkeys` after taking the operator's row.
Under repeatable read its snapshot predates the wait, so a passkey committed meanwhile is invisible. The database
review reproduced it: a redeem held the row, `issue_invite` under repeatable read waited, then issued a link to
an operator who now had a passkey. Serializable did no better, since the redeem was read committed and only locked
the row, which PostgreSQL does not count as a conflict
([PostgreSQL 18, transaction isolation](https://www.postgresql.org/docs/18/transaction-iso.html)). The application
always runs read committed (`scope.rs`), so only a script or a future pool setting reaches it. `disable_operator`
and `end_sessions` already refuse other levels with TD002; the two enrollment functions now do too.

## A link could grow the ceremonies table without bound

Each begin stored a ceremony of up to 16 KiB, with no limit per link, and swept every ended ceremony in one
sequential scan whose row locks queued concurrent begins behind it. At 100,000 rows a begin cost about 14 ms, and
begins for different operators waited on each other. ASVS 5.0 2.1.3, 2.3.2, and 2.4.1 ask for enforced limits and
anti-automation
([ASVS 5.0 V2](https://raw.githubusercontent.com/OWASP/ASVS/v5.0.0/5.0/en/0x11-V2-Validation-and-Business-Logic.md)),
and WebAuthn practice keeps one outstanding challenge per user. A link now holds one ceremony; a newer one replaces
it, as a new link replaces the last. Begin re-checks the link, the operator, and "no passkey yet" in the statement
that stores it, rather than trusting its caller, and sweeps only that operator's ended ceremonies through their
index. Login ceremonies in step 4c need no link, so anyone can start one; they will need a sweep that never
waits (`FOR UPDATE SKIP LOCKED` with a limit), an index on `ends_at`, and a global cap.

## Edges that turned refusals into storage errors

- **A raced credential ID.** Two operators' finishes with one credential ID hit the primary key; the second
  became a unique violation, a storage error, and its refused event rolled back with it. WebAuthn L3 §7.1 asks the
  relying party to fail such a registration, because it could replace a victim's
  ([W3C WebAuthn L3](https://www.w3.org/TR/webauthn-3/)). The link's use and the insert now sit in a subtransaction
  that turns the violation into `taken`, which leaves the link unused and records the attempt.
- **Oversized credentials.** `webauthn-rs` accepts credential IDs up to 4096 bytes, while WebAuthn and the table
  allow 16 to 1023, and the table caps a passkey's JSON at 8 KiB. A software authenticator in a link holder's
  hands could make a registration that verifies and then fails the table's check. Both bounds are now checked
  where the passkey is stored, as a recorded `oversized` refusal. A first attempt checked them in Rust, on the
  compact JSON, which jsonb writes back longer, with a space after each colon and comma, so a padded passkey
  still slipped past it; the doubt-driven review that followed caught it. A check on transports added then was
  removed in the third cycle: it refused `smart-card`, which WebAuthn names, accepted `test`, which it does not,
  and protected nothing, since with no attestation `webauthn-rs` stores no transports.

## A passkey logging in could never find

Step 4c logs operators in without a name, so their passkeys must be discoverable. `webauthn-rs` deliberately does
not require it, since it can exhaust a security key's slots
([webauthn-rs discussion 442](https://github.com/kanidm/webauthn-rs/discussions/442)), so Tetsu sets
`residentKey: required` on the options itself, as web.dev recommends for passkeys
([web.dev, passkey registration](https://web.dev/articles/passkey-registration)). The library ignores that setting
when it verifies, and nothing checked the result. A conforming browser fails rather than make a non-discoverable
credential (WebAuthn L3 §5.4.6), but a client that reports `credProps.rk: false` would have left an operator with a
passkey they could never use, and no way to get another link. Such a registration is now refused, and the link
stays open. One that reports nothing is accepted, since Safari and some clients omit `credProps`, and only the
link holder could forge it.

## Smaller changes now

- **Ceremony lifetime.** The browser waits up to 300 seconds for its authenticator, and WebAuthn L3 §13.4.3 says a
  challenge should stay valid about that long, citing WCAG's "Enough Time". The ceremony now lasts ten minutes,
  never past its link.
- **Grants.** One schema-wide grant replaced the list of functions Identity's role may run. It was safe, since
  the outbox template adds no functions, but a helper added later to the first migration would have become callable
  without anyone deciding so. The list is back, and every routine is revoked from PUBLIC first
  ([PostgreSQL 18, CREATE FUNCTION](https://www.postgresql.org/docs/18/sql-createfunction.html)).
- **The account picker.** Every realm shares one passkey domain until Tetsu has domains (ADR-0061), so an
  operator's passkey appears beside demo staff and customers' passkeys. Its name now reads like
  "Tetsu operator · Kurt Medley", the label first, since authenticators may keep only the first 64 bytes of a name,
  and the plain name as its display name. Renaming a passkey later works only in
  Chrome and Edge ([Chrome, Signal API](https://developer.chrome.com/docs/identity/webauthn-signal-api)).
- **Tidiness.** A disable ends the operator's ceremonies as it ends their sessions and link. Two lookups read the
  clock once per statement, as their comments promised.
- **The AAGUID.** Identity's README said it "says nothing" without attestation. WebAuthn L3 §18.1.1 records that
  browsers no longer zero it under `attestation: none`; it is unverified, but it names the passkey's provider
  ([web.dev, AAGUID](https://web.dev/articles/webauthn-aaguid)). `webauthn-rs` discards it for that format, so
  keeping it means reading the authenticator data the browser sends, in step 4d.

## For later steps

- **Step 4d, the audit record.** ASVS 5.0 16.3.1 asks for every authentication operation, successful or not, and
  NIST SP 800-63B-4 §4.1 for the time and source of each authenticator event
  ([ASVS 5.0 V16](https://raw.githubusercontent.com/OWASP/ASVS/v5.0.0/5.0/en/0x25-V16-Security-Logging-and-Error-Handling.md),
  [NIST 800-63B-4 events](https://pages.nist.gov/800-63-4/sp800-63b/events/)). Begin and every refusal, including
  a registration that fails to verify, should be recorded with the address, user agent, and the operator as the
  actor, once the web layer that knows them exists. The passkey's AAGUID, transports, and a label belong there
  too.
- **Step 4d, the browser.** Only someone holding a ceremony token reaches finish's refusals, so the page may tell
  "start again; your link is still open" from "this link has ended; ask for a new one" without revealing anything.
  NIST asks for clear instructions after a binding mishap. Begin's refusals stay one answer. Begin runs from a
  button's POST, never on page load.
- **Step 4c, login.** Check that `response.userHandle` names the credential's operator (WebAuthn L3 §7.2); update
  the counter and backup state under the passkey's row lock; enforce two passkeys at most, under the operator's
  lock, with the TD002 guard; and pass `excludeCredentials` when adding the second.
- **Step 5.** Enrollment needs rows in ADR-0048's rate-limit table, per network address, for begin and finish.

## A decision for the maintainer

An operator whose only passkey becomes unusable, for example one lost before a second is added, could not get a new
link, since `invite` refuses an operator who holds a passkey. ADR-0057 has such an operator given a new account and
the old one removed, but operator names are unique, and nothing removes an operator. Two tabs racing on one link
can also leave a device holding a passkey Tetsu refused, which the server cannot prevent.

The maintainer chose `tetsu operator remove-passkeys <id>`: it removes the operator's passkeys, ends their sessions
and any ceremony, and is recorded, after which `invite` issues a link as for a new operator. The operator keeps
their account and its audit record. ADR-0057's recovery wording should be amended to match.

## What is already right

- The link is used up only when the passkey is stored, in one transaction that re-checks disabled, enrolled,
  replaced or ended, and taken.
- Beginning changes nothing about the link, so a mail scanner or an abandoned attempt costs nothing.
- The ceremony is single use, deleted whatever happens, and its state never reaches the browser, the safe use of
  `danger-allow-state-serialisation` that `webauthn-rs` documents.
- The relying party is the origin's scheme, host, and port, refusing other ports, subdomains, and addresses, and
  requires user
  verification. The user handle is random and holds nothing personal.
- Every refusal reads the same to the browser, while storage faults are told apart.
- No one is logged in after enrolling, as NIST and OWASP advise.
- Definer functions pin `search_path`, belong to a role that cannot log in, and are callable only by Identity's
  role.
