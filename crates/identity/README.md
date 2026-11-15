# Identity

Who signs in to Tetsu, their passkeys, and their sessions ([ADR-0040](../../docs/adr/0040-identity-and-sign-in.md),
[ADR-0048](../../docs/adr/0048-sign-in-design.md)). Tier 1, since it is identity and access. Operators come first,
in slice 5 ([#110](https://github.com/heavymedl-labs/tetsu/issues/110)); staff and customers follow.

So far, `Operators` creates operators and starts, finds, and ends their sessions. `postgres_store(pool)` returns it
over Postgres.

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

## Not yet

Passkeys, enrollment, the sign-in pages, the `Sec-Fetch-Site` check, the headers that apply these checks, the
commands and pages that end sessions ([ADR-0059](../../docs/adr/0059-session-controls-and-request-checks.md)), rate
limits, and the `identity.signed_in` event arrive in the later steps of #110.
