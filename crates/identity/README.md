# Identity

Who signs in to Tetsu, their passkeys, and their sessions ([ADR-0040](../../docs/adr/0040-identity-and-sign-in.md),
[ADR-0048](../../docs/adr/0048-sign-in-design.md)). Tier 1, since it is identity and access. Operators come first,
in slice 5 ([#110](https://github.com/heavymedl-labs/tetsu/issues/110)); staff and customers follow.

So far, `Operators` creates, lists, and disables operators, issues their enrollment links, and starts, finds, and
ends their sessions. `postgres_store(pool)` returns it over Postgres.

- **Session tokens are 256 random bits** from the operating system's source, written as 64 lowercase hexadecimal
  digits. Only their SHA-256 hash is stored ([ADR-0047](../../docs/adr/0047-sign-in-libraries.md)), and a token
  is never printed.
- **Operator sessions end** 8 hours after they start or 15 minutes after their last request, whichever comes first,
  by Postgres's clock alone, with each realm's limits kept in the SQL beside it, so no caller can stretch one. A
  session past either limit is removed when it is next presented, so it never comes back, and starting a session
  removes the operator's spent ones, so they never pile up.
- **Checking a session and recording the request are one step,** in `use_session`: an update that waits for any
  request holding the row and checks it again as that request left it, then a removal only if the session is spent.
  Requests at the same moment never end a live session or revive a spent one, and the last request never moves back.
- **Signing out removes the session** on the server. It changes state, so it passes the origin and form-token
  checks first.
- **A state-changing request gets one of three answers:** it acts for its live session, its session has ended, or
  it is refused. A refusal is told apart, so the page adapter can answer it as forbidden and record it
  (`ForChange`, `SignOut`).
- **Session tokens compare in constant time,** as form tokens do.
- **Both tables are global** ([ADR-0042](../../docs/adr/0042-tenant-isolation.md)): `identity.operators`, which
  Identity's role adds to and reads, and `identity.sessions`, which it reaches only through lookup functions that
  take one token's hash and realm a call, so nothing can list sessions, and no realm uses or ends another's.
- **Each operator has a random WebAuthn user handle,** which says nothing about them, and each session a random
  public id, for a list of sessions to show and end them by without the token's hash.
- **Each realm's session cookie** is `__Host-` prefixed, `Secure`, `HttpOnly`, and `SameSite=Lax`, with path `/`
  and a `Max-Age` no longer than the session (`Realm::session_cookie`, `Realm::cleared_cookie`). Reading it
  (`Realm::token_in`) takes every `Cookie` header, as HTTP/2 may split them, refuses a request that carries it
  twice, and trims only ASCII whitespace, so a name led by Unicode whitespace is never taken for it.
- **Every state-changing request** must come from one of the realm's `Origins`, exactly, and carry the session's
  form token (`Operators::session_for_change`). A `Change` is built from every `Cookie` header, so the session
  token is always read through `Realm::token_in`. A form token is derived from the session token with SHA-256, so
  nothing more is stored. Each page masks it with a fresh random pad, so a compressed page never repeats it (BREACH),
  and it is unmasked and compared in constant time. These are plain text in and out; the page adapter in step 4 reads
  and writes the headers.

## Hot paths

- **Finding a session** runs on every signed-in request: one call to `use_session`, an update by primary key that
  returns the row. ADR-0040 asks for a benchmark of it, which comes with the session middleware.

## The operator command

`tetsu operator` ([ADR-0040](../../docs/adr/0040-identity-and-sign-in.md),
[ADR-0059](../../docs/adr/0059-session-controls-and-request-checks.md)) works through these use cases:

- **An enrollment link** carries 256 random bits, stored only as their hash, good once and for an hour, by one
  reading of the clock. Each operator has one open link at most; a new one replaces the last. An operator who
  holds a passkey is issued none, since another passkey is added only after signing in (`v5.0.0-6.4.1`). The
  command prints
  it only to a terminal, a guard against it landing in a log by accident; the trust boundary is access to the
  database. The token sits in the link's fragment, which a browser never sends to a server, though its history
  keeps it. The page a link opens, `/ops/enroll`, arrives with passkeys, so until then a link opens nothing.
- **Names are unique.** A create retried after its output was lost is refused, naming the operator who holds the
  name, rather than making a second.
- **Enrolling a first passkey** takes two requests (`v5.0.0-6.4.1`). Beginning reads the link, which must be open
  and name an enabled operator with no passkey, and keeps a ceremony for five minutes under a token only the
  browser holds; nothing is used up, so a scanner that opens the link, or an abandoned attempt, costs nothing.
  Finishing takes the ceremony once, verifies the registration through `webauthn-rs` against the realm's exact
  origin, with user verification, and then, in one transaction holding the operator's row, stores the passkey and
  uses up the link. It refuses an operator disabled, given a new link, or enrolled by another tab meanwhile, and
  records `identity.enrollment_redeemed` with the outcome, the reason, and the credential ID. Every refusal
  reads the same to the browser. The operator is not signed in: signing in with the new passkey proves it works.
  Passkeys are discoverable, so signing in needs no name. Their AAGUID is not kept: with no attestation it says
  nothing, and every operator enrolls again with attestation on Tetsu's domains (ADR-0057).
- **Disabling an operator** ends every session and the open link they hold (`v5.0.0-7.4.2`), in separate statements
  after the update, so they see a sign-in or link committed while the update waited on the operator's row. That
  holds only under read committed, which every scoped transaction asks for, and which disabling and ending
  sessions refuse to run without. A disabled operator starts no session, uses none, and is issued no link.
  Enabling them again leaves their sessions ended.
- **A request already past its session check** when a disable commits finishes that one request. Stopping it too
  would mean holding the operator's row through every request, ASVS level 3's "applied immediately" (8.3.2),
  beyond the level 2 Tetsu targets.
- **Ending sessions,** one operator's or every operator's, is the administrator's control (`v5.0.0-7.4.5`). It
  takes the operators' rows first, so a sign-in that holds one finishes and is ended too. A sign-in still waiting
  for a row, or one that starts after, goes through, since ending sessions changes nothing about the operator.
  For an operator who must stay out, disable them. An operator added during `--all` is outside it.
- **Counts** are of live sessions and links. Spent ones go too, but ending them changes nothing, so ending only
  spent sessions is recorded as `unchanged`.
- **An operator who does not exist** is refused by every command, never treated as one with nothing to do.
- **Each change records one audit event** in Identity's outbox, in the same transaction, with IDs and counts,
  never names or tokens, plus who ran it and its outcome, `changed`, `unchanged`, or `refused`
  (`v5.0.0-16.2.1`, `v5.0.0-16.3.3`): `identity.operator_created`, `identity.enrollment_issued`,
  `identity.operator_disabled` (with the sessions and links it ended), `identity.operator_enabled`, and
  `identity.sessions_ended`. Listing changes nothing and records nothing. A command refused before it reaches the
  database, such as one printing a link to a pipe, records no event. Signing in and out records
  nothing yet; `identity.signed_in` comes with the sign-in pages.
- **Who ran a command** is `user@host`: the first of `SUDO_USER`, `USER`, and `LOGNAME` that is set, and the
  first real line of `/etc/hostname`, with `unknown` for a part that is missing, not UTF-8, or would not show
  safely. It is asserted, not proven: every command
  shares one database login (ADR-0061), so database access is the trust boundary, and only Identity's code, not
  the database, makes a change record its event.
- **Names and actors** hold no control, invisible, direction-changing, or blank-looking characters, so neither can
  hide or reorder what the audit record shows. The database refuses control characters in names too, and
  Identity's role adds an operator by name alone.

## Not yet

Signing in with a passkey, the pages and script that run enrollment in a browser, recording a registration
that fails to verify (with the sign-in rate limits), revoking a link on its own
(today a new link or a disable ends it), the sign-in
pages, the `Sec-Fetch-Site` check, the headers that apply these checks, the session list
([ADR-0059](../../docs/adr/0059-session-controls-and-request-checks.md)), rate limits, the `identity.signed_in`
event, and running the dispatcher that carries Identity's events to the audit record arrive in later steps of
[#110](https://github.com/heavymedl-labs/tetsu/issues/110).
