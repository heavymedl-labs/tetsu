# Identity

Who logs in to Tetsu, their passkeys, and their sessions ([ADR-0040](../../docs/adr/0040-identity-and-sign-in.md),
[ADR-0048](../../docs/adr/0048-sign-in-design.md)). Tier 1, since it is identity and access. Admins come first,
in slice 5 ([#110](https://github.com/heavymedl-labs/tetsu/issues/110)); staff and customers follow.

So far, `Admins` creates, lists, disables, and enables admins, issues their invite links, begins and
finishes joining with their first passkey, logs them in with it, adds their second, removes their passkeys, and
starts, finds, and ends their sessions. `postgres_store(pool)` returns it over Postgres.

An admin may hold an email address ([ADR-0074](../../docs/adr/0074-operator-email-in-development.md)).
`invite_message` renders their invite from templates, never kept for the mailbox, and `notices(mailer)` is an outbox
consumer of Identity's own events that emails an admin when a login, an added passkey, or a join goes through, keyed on the event, with no link in it
(`v5.0.0-6.3.5`, `v5.0.0-6.3.7`). A notice that cannot go just now is retried; one the mail server refuses is
dropped. `/admin/mailbox` shows a logged-in admin the email kept rather than sent, newest first, as text, through
the kernel's `Mailbox` port, which Mail implements; a message's HTML is never shown.

- **Session tokens are 256 random bits** from the operating system's source, written as 64 lowercase hexadecimal
  digits. Only their SHA-256 hash is stored ([ADR-0047](../../docs/adr/0047-sign-in-libraries.md)), and a token
  is never printed.
- **Admin sessions end** 8 hours after they start or 15 minutes after their last request, whichever comes first,
  by Postgres's clock alone, with each realm's limits kept in the SQL beside it, so no caller can stretch one. A
  session past either limit is removed when it is next presented, so it never comes back, and starting a session
  removes the admin's spent ones, so they never pile up.
- **Checking a session and recording the request are one step,** in `use_session`: an update that waits for any
  request holding the row and checks it again as that request left it, then a removal only if the session is spent.
  Requests at the same moment never end a live session or revive a spent one, and the last request never moves back.
- **Logging out removes the session** on the server. It changes state, so it passes the origin and form-token
  checks first.
- **An admin sees and ends their own sessions** at `/admin/settings/security` (ADR-0059, `v5.0.0-7.5.2`), as Linear lists them:
  each live one by its public id, with the coarse device it started on, such as Firefox on Linux, when it started,
  and when it was last used, this one first. `sessions_of` finds them from the presented session alone and renews
  none. `POST /admin/sessions/end` ends one other session, or every other one, passing logging out's checks, and only
  from a login within ten minutes, as adding a passkey needs (ADR-0048, `v5.0.0-7.5.1`); this session is never ended
  there. Each attempt that reaches storage is recorded as `identity.other_sessions_ended`, with the session it
  named, if any, the admin and the asking session's public id where storage found them, and how many ended, or
  refused with its reason: `stale` for a session no passkey proved within ten minutes, `ended` or `disabled` when
  the session or admin went since the request was checked. One refused before, from elsewhere, without its form
  token, or naming something that is not a session id, is recorded as a refused request, below.
  The device is named once, as login and joining start the session, from the request's own `User-Agent`, and goes
  with it.
- **A state-changing request gets one of three answers:** it acts for its live session, its session has ended, or
  it is refused. A refusal is told apart, with the first check it failed, so the page adapter can answer it as
  forbidden and record it (`ForChange`, `Logout`, `FailedCheck`).
- **A refused request is recorded** as `identity.request_refused` ([ADR-0081](../../docs/adr/0081-recording-refused-requests.md),
  `v5.0.0-16.3.2`, `v5.0.0-16.3.3`): logging out, ending sessions, adding a passkey, and staying logged in, refused at their
  origin, cookie, form-token, or field check, in that order, and the steps before login refused at their origin.
  The record names the route, the check, and the device, never the token, the address, or the cookie's admin,
  since the request proved nothing. Each network has room for ten records in fifteen minutes, under the `refusal`
  rate limit; past that its requests are still refused, and the first each window records
  `identity.refusals_suppressed` instead. The bound is per network, as login's limits are. A refusal that cannot
  be recorded is unavailable. Malformed input from the realm's own origin, such as a body that is not JSON, is
  refused unrecorded.
- **Session tokens compare in constant time,** as form tokens do.
- **Every table is global** ([ADR-0042](../../docs/adr/0042-tenant-isolation.md)): `identity.admins`, which
  Identity's role adds to and reads, and `identity.sessions`, `invites`, `passkeys`, and `ceremonies`, which
  it reaches only through lookup functions that take one hash a call, so nothing lists links, a session lists only
  its own admin's sessions, and no realm uses or ends another's.
- **Each admin has a random WebAuthn user handle,** which says nothing about them, and each session a random
  public id, for a list of sessions to show and end them by without the token's hash.
- **Each realm's session cookie** is `__Host-` prefixed, `Secure`, `HttpOnly`, and `SameSite=Lax`, with path `/`
  and a `Max-Age` no longer than the session (`Realm::session_cookie`, `Realm::cleared_cookie`). Reading it
  (`Realm::token_in`) takes every `Cookie` header, as HTTP/2 may split them, refuses a request that carries it
  twice, and trims only ASCII whitespace, so a name led by Unicode whitespace is never taken for it.
- **Every state-changing request for a session** must come from one of the realm's `Origins`, exactly, and carry
  the session's form token (`Admins::session_for_change`). Logging in and joining have no session yet, so
  their requests pass `Origins::admit` alone, the only guard against logging someone in to an attacker's account:
  an exact `Origin`, and a `Sec-Fetch-Site` of `same-origin` when the browser sends one (ADR-0059). A `Change`
  checks both too, and is built from every `Cookie` header, so the session token is always read through
  `Realm::token_in`. A form token is derived from the session token with SHA-256, so
  nothing more is stored. Each page masks it with a fresh random pad, so a compressed page never repeats it (BREACH),
  and it is unmasked and compared in constant time. These are plain text in and out; the pages' adapter reads and writes
  the headers.

## Hot paths

- **Finding a session** runs on every logged-in request: one call to `use_session`, an update by primary key that
  returns the row. ADR-0040 asks for a benchmark of it, which comes with the session middleware.
- **Reading a session's time left** runs once a minute for each visible logged-in page, and not at all for a hidden
  one: one call to `session_left`, a read by primary key that writes nothing.

## The admin command

`tetsu admin` ([ADR-0040](../../docs/adr/0040-identity-and-sign-in.md),
[ADR-0059](../../docs/adr/0059-session-controls-and-request-checks.md)) works through these use cases:

- **An invite link** carries 256 random bits, stored only as their hash, good once and for an hour, by one
  reading of the clock. Each admin has one open link at most; a new one replaces the last. An admin who
  holds a passkey is issued none, since another passkey is added only after logging in (`v5.0.0-6.4.1`). The
  command prints
  it only to a terminal, a guard against it landing in a log by accident; the trust boundary is access to the
  database. The token sits in the link's fragment, which a browser never sends to a server, though its history
  keeps it. The page a link opens, `/admin/join`, reads the token from the fragment and clears it from the address
  bar at once.
- **Names are unique.** A create retried after its output was lost is refused, naming the admin who holds the
  name, rather than making a second.
- **Joining with a first passkey** takes two requests (`v5.0.0-6.4.1`). Beginning reads the link, which must be open
  and name an enabled admin with no passkey, and keeps a ceremony under a token only the browser holds, for ten
  minutes but never past the link. Beginning takes the admin's row, so a disable at the same moment ends what
  it stores, and a new link waits for it and then ends it. A link holds one ceremony, by a unique index; a newer
  one replaces it. Nothing is used up, so a scanner that opens the link, or an abandoned attempt, costs the link
  nothing.
  Finishing takes the ceremony once, verifies the registration through `webauthn-rs` against the realm's origin,
  its scheme, host, and port alike, with user verification, and then, in one transaction holding the admin's
  row, stores the passkey and uses up the link. It refuses an admin disabled, given a new link, or joined by
  another tab meanwhile; a credential another passkey has; and one larger than the table keeps, measured as the
  table measures it. Each of those is recorded as `identity.invite_redeemed` with the outcome, the reason,
  and the credential ID. Before that, it refuses a registration that is malformed, does not verify, or whose
  browser says the passkey is not discoverable, and records it the same way, with that reason. Transports are
  taken as listed, as `WebAuthn` asks, and the ones `webauthn-rs` names are kept, once each and sorted, as a hint
  for later ceremonies. Every refusal reads the same to the browser, whichever part of Identity
  refuses it. Joining logs the admin in, in the same transaction, ending any session the browser presented; that
  session rests on the link, so it is never fresh, and anything that needs a fresh login waits for a passkey login
  ([ADR-0069](../../docs/adr/0069-logged-in-on-joining.md)). Passkeys are discoverable, so
  logging in needs no name, and the account picker shows each as "Tetsu admin · Name", the label first, where
  an authenticator's 64-byte cut never reaches it.
  The AAGUID, which names a passkey's provider but is unverified without attestation, is not kept: `webauthn-rs`
  does not expose it, and reading it needs a CBOR decoder, a new dependency.
- **Logging in with a passkey** takes two requests. Beginning names no passkey, so the browser offers the person's
  own, and asks for the prompt a button opens; it keeps a ceremony for ten minutes, like joining's. Anyone may
  begin one, so until step 5's rate limits each begin sweeps up to a hundred ended ceremonies without waiting on
  any, and past ten thousand open logins ends the oldest rather than refusing a begin (`v5.0.0-6.3.1`). A flood of
  begins can still end a real ceremony before its person answers, until those rate limits. Finishing takes the
  ceremony once, finds the passkey the answer names, refuses a credential ID past `WebAuthn`'s 1023 bytes, refuses
  it unless its enabled admin's user handle is the one the answer gives (`WebAuthn` L3 §7.2), and verifies it
  through `webauthn-rs`, user verification included, which refuses a counter that does not move forward or a backup
  eligibility that changed. A synced passkey's counter stays at 0, so nothing can tell a copy of one from the
  original. Then, in one transaction holding the admin's row, it stores the passkey's new state only if no other
  login changed it meanwhile, and starts the session. Every answer to an open ceremony is recorded as
  `identity.logged_in`, naming the admin where known; one that reaches storage carries the passkey's flags and,
  once one starts, the session's public ID. A refusal names its reason (`malformed`, `unknown_credential`,
  `handle_mismatch`, `counter_regressed`, `not_verified`, `disabled`, `raced`, or `unknown`), and every refusal
  reads the same to the browser. `webauthn-rs` checks user verification and backup eligibility before the signature,
  so anyone could claim either: they are recorded as `not_verified`. A token that finds no open ceremony, and a
  storage or randomness failure, record nothing. The [login
  review](../../docs/research/identity/passkey-sign-in-review.md) lists what later steps add.
- **Adding a second passkey** (ADR-0057) needs a fresh login: a session begun with a passkey within ten minutes,
  by Postgres's clock (ADR-0048, `v5.0.0-7.5.1`), on a request that passed the origin and form-token checks, which
  only `session_for_change` makes. The device is told which passkey the admin holds, so it makes no second where
  it has one. Beginning stores an addition for ten minutes, one per admin, re-checking the session, that the
  admin is enabled, and that they hold fewer than two, under their row; it ends with the session that began it.
  Finishing takes the addition for that session alone, once, and verifies the registration as joining does.
  Storing the passkey uses the addition up, under the admin's row, while the session is still live, refusing a
  third, a taken credential, or one too large, so the database itself ties the passkey to an addition a fresh
  session began. It can finish up to twenty minutes after the login: ten to begin, and the addition's ten. Storing
  it ends the admin's other sessions and keeps this one (ADR-0059, `v5.0.0-7.4.3`). Each attempt that reaches
  storing the passkey is recorded as `identity.passkey_added`, with the sessions it ended, the adding session's
  public ID, and the passkey's flags; one refused before, such as a registration that does not verify, records
  nothing yet. Admins have no email yet, so the audit record is the notice ADR-0040 asks for.
- **Removing an admin's passkeys** lets them be invited and join again, as when their only passkey is
  lost or unusable. It ends their sessions and any ceremony too, since a session begun with a removed passkey must
  not outlive it (`v5.0.0-7.4.3`), takes their row for update as a disable does, and records
  `identity.passkeys_removed` with the counts. The admin keeps their account and its audit record, where
  ADR-0057 has a new account made instead.
- **What one database login allows.** Identity's role, which the web app shares with the admin command
  (ADR-0061), can do what an admin can, for any admin: the database cannot check a passkey's signature.
  Code that runs SQL as that role can replace an admin's passkeys outright, by removing them, issuing a link,
  and joining with one of its own; or, for one who holds a passkey, read its credential ID from the audit outbox,
  start a session through the login function, and add a passkey of its own. What the database refuses is
  narrower, and keeps stored data sound rather than keeping that role out: calling `start_session` directly, a
  login that changes a stored passkey beyond its counter and backup flags, and a passkey added outside an
  addition its session took. Database access is the trust boundary, as for every admin command.
- **A device can keep a passkey Tetsu does not.** Every attempt registers under the admin's one user handle,
  and an authenticator replaces a discoverable passkey with a newer one for the same handle. If two tabs begin,
  the newer finishes, and the older then creates its passkey anyway, the device holds a passkey whose finish was
  refused. Nothing on the server can prevent it; recovering such an admin is the open question in the
  [enrollment review](../../docs/research/identity/passkey-enrollment-review.md#a-decision-for-the-maintainer).
- **What `webauthn-rs` owns.** Its challenge comes from its own random source, which panics rather than failing
  cleanly if the operating system's cannot be read; and serialising a passkey can panic inside the library on a
  malformed attestation certificate. A stored ceremony that reads but holds the wrong state fails as the browser's
  registration would, since only Identity writes it. Without attestation, a registration's flags and origin are
  the client's word: the guarantees that bind a person to a passkey come at login, where the authenticator signs.
- **Read committed** is required, not assumed, by every function that waits on an admin's row and then checks
  what changed: issuing a link, beginning and finishing a join, logging in, beginning and storing an
  addition, enabling, disabling, removing passkeys, and ending sessions refuse any other level (TD002), and so does
  beginning a login, whose sweep would fail there when begins overlap. Such a refusal is a fault of the calling
  code, so it surfaces as a storage error.
- **Disabling an admin** ends every session and the open link they hold (`v5.0.0-7.4.2`), in separate statements
  after the update, so they see a login or link committed while the update waited on the admin's row. That
  holds only under read committed, which every scoped transaction asks for, and which disabling and ending
  sessions refuse to run without. A disabled admin starts no session, uses none, and is issued no link.
  Enabling them again leaves their sessions ended.
- **A request already past its session check** when a disable commits finishes that one request. Stopping it too
  would mean holding the admin's row through every request, ASVS level 3's "applied immediately" (8.3.2),
  beyond the level 2 Tetsu targets.
- **Ending sessions,** one admin's or every admin's, is the administrator's control (`v5.0.0-7.4.5`). It
  takes the admins' rows first, so a login that holds one finishes and is ended too. A login still waiting
  for a row, or one that starts after, goes through, since ending sessions changes nothing about the admin.
  For an admin who must stay out, disable them. An admin added during `--all` is outside it.
- **Counts** are of live sessions and links. Spent ones go too, but ending them changes nothing, so ending only
  spent sessions is recorded as `unchanged`.
- **An admin who does not exist** is refused by every command, never treated as one with nothing to do.
- **Each change records one audit event** in Identity's outbox, in the same transaction, with IDs and counts, never
  names or tokens, plus who ran it and its outcome, `changed`, `unchanged`, or `refused` (`v5.0.0-16.2.1`,
  `v5.0.0-16.3.3`): `identity.admin_created`, `identity.invite_issued`, `identity.admin_disabled` (with
  the sessions and links it ended), `identity.admin_enabled`, `identity.sessions_ended`, and
  `identity.passkeys_removed`; joining, logging in, and adding a passkey record `identity.invite_redeemed`,
  `identity.logged_in`, and `identity.passkey_added`, as described above. Listing changes nothing and records
  nothing. A command refused before it reaches the database, such as one printing a link to a pipe, records no
  event. Logging out records no event of its own; a refused one is recorded as above.
- **Who ran a command** is `user@host`: the first of `SUDO_USER`, `USER`, and `LOGNAME` that is set, and the
  first real line of `/etc/hostname`, with `unknown` for a part that is missing, not UTF-8, or would not show
  safely. It is asserted, not proven: every command
  shares one database login (ADR-0061), so database access is the trust boundary, and only Identity's code, not
  the database, makes a change record its event.
- **Names and actors** hold no control, invisible, direction-changing, or blank-looking characters, so neither can
  hide or reorder what the audit record shows. The database refuses control characters in names too, and
  Identity's role adds an admin by name alone.

## The admin pages

`adapters/http.rs` serves the admins' realm at `/admin` ([ADR-0084](../../docs/adr/0084-realm-routes.md)):
`/admin/login`, `/admin/join`, their account's `/admin/settings/profile` and `/admin/settings/security`, and
`/admin/mailbox`, with the JSON
endpoints the passkey islands in `web/src/passkey/` call: `<tetsu-passkey>` runs a page's one passkey action, and
`<tetsu-passkeys>` wraps the admin's list of passkeys. Pages take Linear's look, as ADR-0035 gives Tetsu's own
pages, in a layout of Tetsu's own, load only Tetsu's own script, under a policy of their own, and are never cached.
Logging in is by passkey alone, as ADR-0040 sets for admins. Endpoints read only a JSON object sent as
`application/json`, answer `no-store`, and give each ceremony's one refusal; a failure of the server is `503` to
the browser and reported to the admin. Logging in ends the session the browser presented, in the transaction
that starts the new one. Logging out is a plain form that clears the cookie and the browser's cache of the pages. Until
admins have a home (#157), `/admin` sends them to Profile, where logging in and joining land too. Every admin
page takes Linear's settings mode (#156): the shared settings shell, `web/templates/settings.html`, with a sidebar of
Tetsu's wordmark, the Personal pages, a Platform group of pages only admins have, now the mailbox, and the
admin with Log out at its foot, which below a tablet's width opens from "Menu" as a native popover, beside the
initials menu (#148, `v5.0.0-7.4.4`). The initials open a menu, a native popover that works without script, with the
admin's name, "Admin", Account, and the Log out form; it and the header it came with are shared components in
`web/templates/components.html`, which the public site's header uses too. An address under `/admin` that no page
holds gets the site's own not-found page, as any wrong address does. Pages before login and the refused and
unavailable pages have no menu. Profile shows the
admin's name, email address, and role; Security lists their sessions, then their passkeys. Security
offers a second passkey only within ten minutes of logging in, as the database allows, and its
`<tetsu-passkeys>` tells the admin's devices every passkey Tetsu holds, through the Signal API.

The edges of a login follow [the admin lifecycles research](../../docs/research/identity/operator-lifecycles.md).
A logged-in admin who opens the login page is sent on. A login returns to the page it was asked for, through
`browser::return_path`, which accepts only `/admin` or a plain path under it, checked on the page and again when the
login finishes. Every admin page extends `account_shell.html`, which loads the session island in
`web/src/session/`. `<tetsu-session>` reads `GET /admin/session`, the seconds to each limit by the database's clock,
which renews nothing, each minute while its page is visible, so an ending no limit foretold is seen within a minute,
and two minutes before a limit opens the page's dialog: before the idle limit to stay logged
in, a `POST` with the form token that renews the session, and before the 8 hour limit to log in again, a logout that returns to the page after the next login. It keeps the
site's tabs in step over a `BroadcastChannel`, asks again when the page is shown, and sends a page whose session has
ended to `/admin/login?ended=1`, which says the admin was logged out and how long a session may sit idle.

## Not yet

Rate limits, with the request's address in each record; revoking a link on its own (today a new link or a
disable ends it); browser tests ([#129](https://github.com/heavymedl-labs/tetsu/issues/129)); and running the dispatcher that
carries Identity's events to the audit record arrive in later steps of
[#110](https://github.com/heavymedl-labs/tetsu/issues/110).
