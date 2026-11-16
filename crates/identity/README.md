# Identity

Who signs in to Tetsu, their passkeys, and their sessions ([ADR-0040](../../docs/adr/0040-identity-and-sign-in.md),
[ADR-0048](../../docs/adr/0048-sign-in-design.md)). Tier 1, since it is identity and access. Operators come first,
in slice 5 ([#110](https://github.com/heavymedl-labs/tetsu/issues/110)); staff and customers follow.

So far, `Operators` creates, lists, disables, and enables operators, issues their enrollment links, begins and
finishes enrolling their first passkey, signs them in with it, adds their second, removes their passkeys, and
starts, finds, and ends their sessions. `postgres_store(pool)` returns it over Postgres.

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
- **Every table is global** ([ADR-0042](../../docs/adr/0042-tenant-isolation.md)): `identity.operators`, which
  Identity's role adds to and reads, and `identity.sessions`, `enrollments`, `passkeys`, and `ceremonies`, which
  it reaches only through lookup functions that take one hash a call, so nothing can list sessions or links, and
  no realm uses or ends another's.
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
  and name an enabled operator with no passkey, and keeps a ceremony under a token only the browser holds, for ten
  minutes but never past the link. Beginning takes the operator's row, so a disable at the same moment ends what
  it stores, and a new link waits for it and then ends it. A link holds one ceremony, by a unique index; a newer
  one replaces it. Nothing is used up, so a scanner that opens the link, or an abandoned attempt, costs the link
  nothing.
  Finishing takes the ceremony once, verifies the registration through `webauthn-rs` against the realm's origin,
  its scheme, host, and port alike, with user verification, and then, in one transaction holding the operator's
  row, stores the passkey and uses up the link. It refuses an operator disabled, given a new link, or enrolled by
  another tab meanwhile; a credential another passkey has; and one larger than the table keeps, measured as the
  table measures it. Each of those is recorded as `identity.enrollment_redeemed` with the outcome, the reason,
  and the credential ID. Before that, it refuses a registration that does not verify, or whose browser says the
  passkey is not discoverable; recording those, with the request's address, waits for step 4d. Transports are
  taken as listed, as `WebAuthn` asks. Every refusal reads the same to the browser, whichever part of Identity
  refuses it. The operator is not signed in: signing in with the
  new passkey proves it works. Passkeys are discoverable, so signing in needs no name, and the account picker
  shows each as "Tetsu operator · Name", the label first, where an authenticator's 64-byte cut never reaches it.
  The AAGUID, which names a passkey's provider but is unverified without attestation, is kept from step 4d, when
  the browser's authenticator data arrives.
- **Signing in with a passkey** takes two requests. Beginning names no passkey, so the browser offers the
  person's own, and asks for the prompt a button opens; it keeps a ceremony for ten minutes, like enrollment's.
  Anyone may begin one, so until step 5's rate limits each begin sweeps up to a hundred ended ceremonies without
  waiting on any, and past ten thousand open sign-ins ends the oldest rather than refusing a begin
  (`v5.0.0-6.3.1`). A flood of begins can still end a real ceremony before its person answers, until those rate
  limits. Finishing takes the ceremony once, finds the passkey the answer names, refuses a credential ID past
  `WebAuthn`'s 1023 bytes, refuses it unless its enabled operator's user handle is the one the answer gives
  (`WebAuthn` L3 §7.2), and verifies it through `webauthn-rs`, user verification included, which refuses a counter
  that does not move forward or a backup eligibility that changed. A synced passkey's counter stays at 0, so
  nothing can tell a copy of one from the original. Then, in one transaction holding the
  operator's row, it stores the passkey's new state only if no other sign-in changed it meanwhile, and starts the
  session. Every answer to an open ceremony is recorded as `identity.signed_in`, naming the operator where known;
  one that reaches storage carries the passkey's flags and, once one starts, the session's public ID. A refusal
  names its reason (`malformed`, `unknown_credential`,
  `handle_mismatch`, `counter_regressed`, `not_verified`, `disabled`, `raced`, or `unknown`), and every refusal
  reads the same to the browser. `webauthn-rs` checks user verification and backup eligibility before the
  signature, so anyone could claim either: they are recorded as `not_verified`. A token that finds no open ceremony, and a storage or randomness failure, record nothing. The
  [sign-in review](../../docs/research/identity/passkey-sign-in-review.md) lists what later steps add.
- **Adding a second passkey** (ADR-0057) needs a fresh sign-in: a session begun with a passkey within ten minutes,
  by Postgres's clock (ADR-0048, `v5.0.0-7.5.1`), on a request that passed the origin and form-token checks, which
  only `session_for_change` makes. The device is told which passkey the operator holds, so it makes no second
  where it has one. Beginning stores an addition for ten minutes, one per operator, re-checking the session, that
  the operator is enabled, and that they hold fewer than two, under their row; it ends with the session that
  began it. Finishing takes the addition for that session alone, once, and verifies the registration as
  enrollment does. Storing the passkey uses the addition up, under the operator's row, while the session is still
  live, refusing a third, a taken credential, or one too large, so the database itself ties the passkey to an
  addition a fresh session began. It can finish up to twenty minutes after the sign-in: ten to begin, and the
  addition's ten. Storing it ends the operator's other sessions and keeps this one (ADR-0059, `v5.0.0-7.4.3`).
  Each attempt that reaches storing the passkey is recorded as `identity.passkey_added`, with the sessions it
  ended, the adding session's public ID, and the passkey's flags; one refused before, such as a registration that
  does not verify, records nothing yet. Operators have no email yet, so the audit record is the notice ADR-0040 asks for.
- **Removing an operator's passkeys** lets them be issued a link and enroll again, as when their only passkey is
  lost or unusable. It ends their sessions and any ceremony too, since a session begun with a removed passkey must
  not outlive it (`v5.0.0-7.4.3`), takes their row for update as a disable does, and records
  `identity.passkeys_removed` with the counts. The operator keeps their account and its audit record, where
  ADR-0057 has a new account made instead.
- **What one database login allows.** Identity's role, which the web app shares with the operator command
  (ADR-0061), can do what an operator can, for any operator: the database cannot check a passkey's signature.
  Code that runs SQL as that role can issue a link for an operator with no passkey and enroll one of its own; and
  for one who holds a passkey, read its credential ID from the audit outbox, start a session through the sign-in
  function, and add a passkey of its own. What the database does refuse is narrower: calling `start_session`
  directly, changing a stored passkey beyond its counter and backup flags, and adding a passkey outside an
  addition its session took. Database access is the trust boundary, as for every operator command.
- **A device can keep a passkey Tetsu does not.** Every attempt registers under the operator's one user handle,
  and an authenticator replaces a discoverable passkey with a newer one for the same handle. If two tabs begin,
  the newer finishes, and the older then creates its passkey anyway, the device holds a passkey whose finish was
  refused. Nothing on the server can prevent it; recovering such an operator is the open question in the
  [enrollment review](../../docs/research/identity/passkey-enrollment-review.md#a-decision-for-the-maintainer).
- **What `webauthn-rs` owns.** Its challenge comes from its own random source, which panics rather than failing
  cleanly if the operating system's cannot be read; and serialising a passkey can panic inside the library on a
  malformed attestation certificate. A stored ceremony that reads but holds the wrong state fails as the browser's
  registration would, since only Identity writes it. Without attestation, a registration's flags and origin are
  the client's word: the guarantees that bind a person to a passkey come at sign-in, where the authenticator signs.
- **Read committed** is required, not assumed, by every function that waits on an operator's row and then checks
  what changed: issuing a link, beginning and finishing an enrollment, disabling, and ending sessions refuse any
  other level (TD002). Such a refusal is a fault of the calling code, so it surfaces as a storage error.
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
  database, such as one printing a link to a pipe, records no event. Signing out records nothing yet.
- **Who ran a command** is `user@host`: the first of `SUDO_USER`, `USER`, and `LOGNAME` that is set, and the
  first real line of `/etc/hostname`, with `unknown` for a part that is missing, not UTF-8, or would not show
  safely. It is asserted, not proven: every command
  shares one database login (ADR-0061), so database access is the trust boundary, and only Identity's code, not
  the database, makes a change record its event.
- **Names and actors** hold no control, invisible, direction-changing, or blank-looking characters, so neither can
  hide or reorder what the audit record shows. The database refuses control characters in names too, and
  Identity's role adds an operator by name alone.

## Not yet

The pages and script that run enrollment and sign-in in a browser, with the request's address in each record;
ending the session a browser presents when it signs in again; recording registrations that fail to verify; rate
limits; revoking a link
on its own (today a new link or a disable ends it); the `Sec-Fetch-Site` check and the headers that apply these
checks; the session list ([ADR-0059](../../docs/adr/0059-session-controls-and-request-checks.md)); and running the
dispatcher that carries Identity's events to the audit record arrive in later steps of
[#110](https://github.com/heavymedl-labs/tetsu/issues/110).
