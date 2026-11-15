# Identity

Who signs in to Tetsu, their passkeys, and their sessions ([ADR-0040](../../docs/adr/0040-identity-and-sign-in.md),
[ADR-0048](../../docs/adr/0048-sign-in-design.md)). Tier 1, since it is identity and access. Operators come first,
in slice 5 ([#110](https://github.com/heavymedl-labs/tetsu/issues/110)); staff and customers follow.

So far, `Operators` creates operators and starts, finds, and ends their sessions. `postgres_store(pool)` returns it
over Postgres.

- **Session tokens are 256 random bits** from the operating system's source, written as 64 lowercase hexadecimal
  digits. Only their SHA-256 hash is stored ([ADR-0047](../../docs/adr/0047-sign-in-libraries.md)), and a token
  is never printed.
- **Operator sessions end** 8 hours after they start or 15 minutes after their last request, whichever comes first.
  A session past either limit is removed when it is next presented, so it never comes back.
- **Signing out removes the session** on the server.
- **Both tables are global** ([ADR-0042](../../docs/adr/0042-tenant-isolation.md)): `identity.operators`, which
  Identity's role adds to and reads, and `identity.sessions`, which it reaches only through lookup functions that
  take one token's hash a call, so nothing can list sessions.
- **Each operator has a random WebAuthn user handle,** which says nothing about them.

## Hot paths

- **Finding a session** runs on every signed-in request: one lookup by primary key through `session_for`, then one
  update through `touch_session`. ADR-0040 asks for a benchmark of it, which comes with the session middleware.

## Not yet

Passkeys, enrollment, the sign-in pages, cookies and form checks, rate limits, and the `identity.signed_in` event
arrive in the later steps of #110.
