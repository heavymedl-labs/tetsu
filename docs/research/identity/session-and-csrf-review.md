# Operator sessions and CSRF: a practice review

Research on 2026-11-14 for [#110](https://github.com/heavymedl-labs/tetsu/issues/110), steps 1 and 2 (commit
`78424fb`), against current standards, frameworks, and browser behavior. It feeds step 3 (the enrollment command),
step 4 (passkey login and the operator pages), and an addendum to
[ADR-0048](../../adr/0048-sign-in-design.md) for ASVS 5.0.0 items 7.1.2, 7.4.2, 7.4.3, 7.4.5, and 7.5.2.

## Summary

Tetsu's design matches or beats current practice almost everywhere. One real bug needs fixing now:
`Realm::token_in` trims each cookie pair with `str::trim`, which strips Unicode whitespace, so a cookie named
`\u{2000}__Host-tetsu-operator` reads as the real one and reopens the prefix bypass PortSwigger published in
September 2025. Beyond that, three changes would bring the design in line with where practice moved in 2025 and 2026.
Add a `Sec-Fetch-Site` check beside the Origin check. Mask the form token on each response, or retire it. Drop
ADR-0048's pre-session record, since the Origin check already guards login forms. Storage, expiry, and cookie
attributes need no change. The ASVS session controls are missing, but the migration has not been applied anywhere,
so they are cheap to prepare now.

## CSRF defense

**The exact Origin check is stricter than every framework surveyed.** OWASP's cheat sheet now says missing headers
and `Origin: null` "are not evidence of a same-origin request" and should be refused unless another defense
succeeds ([OWASP CSRF](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html)).
Tetsu refuses both, with no Referer fallback. Go 1.25's `CrossOriginProtection` allows a request with neither
header, and its fallback compares Origin's host with `Host` but ignores the scheme
([Valsorda](https://words.filippo.io/csrf/), [Go docs](https://pkg.go.dev/net/http#CrossOriginProtection)).
Tetsu's list includes the scheme, so plain `http` never passes. Tetsu also compares against configured origins, not
`Host` or forwarded headers, so OWASP's October 2026 rule about trusting proxy headers does not bite
([commit history](https://github.com/OWASP/CheatSheetSeries/commits/master/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.md)).
Every current browser sends Origin on a same-origin POST; it has been Baseline since July 2020
([MDN Origin](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Origin)).

**Fetch Metadata is now a primary defense, and Tetsu lacks it.** OWASP added a Fetch Metadata section in September
2025 and moved it to the primary controls in December 2025, with an Origin fallback required. `Sec-Fetch-Site` has
been Baseline since March 2023, and scripts cannot set it
([MDN](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Sec-Fetch-Site)). Rails 8.2 makes a
header-only mode the default for new apps and plans to remove tokens
([rails/rails#56350](https://github.com/rails/rails/pull/56350)). The case for adding it is a browser bug. In
Firefox CVE-2026-8950, a form POST that a cross-origin host redirected with 307 carried the first origin instead of
`null`; it was fixed in Firefox 151 ([Bugzilla 1965430](https://bugzilla.mozilla.org/show_bug.cgi?id=1965430)). Two
independent signals cover each other's bugs. The policies differ on two values. Rails allows `same-site`; Go and
OWASP's conservative reading do not. Go allows `none` on POST; Rails refuses it. Tetsu should allow only
`same-origin` on unsafe methods. That is what its exact origin list already implies.

**The form token is sent unmasked into compressed HTML, which meets BREACH's preconditions.** BREACH needs
compression, reflected input, and a secret in the body, and it takes "a few thousand requests"
([breachattack.com](https://breachattack.com/)). Rails and Django mask the token afresh on each response, and Valsorda
says any token in HTML should be masked ([Django CSRF](https://docs.djangoproject.com/en/stable/ref/csrf/),
[Rails source](https://raw.githubusercontent.com/rails/rails/main/actionpack/lib/action_controller/metal/request_forgery_protection.rb),
[Valsorda](https://words.filippo.io/csrf/)). OWASP's cheat sheet does not mention BREACH. In Tetsu the real risk is
low. `SameSite=Lax` keeps the cookie off cross-site subresource requests, so the victim's requests would have to be
thousands of visible top-level navigations. A recovered token is still useless without passing the Origin check. And
SHA-256 is one-way, so it never reveals the session token. Masking costs a few lines: send `pad || (pad XOR token)`
with a fresh 32-byte pad, unmask, and compare in constant time. The other honest choice is to retire the token, as
Go and Rails 8.2 do, once `Sec-Fetch-Site` lands. This review recommends masking. It keeps ADR-0048's two-control
design and costs less than an ADR change.

**Login forms need no pre-session record.** OWASP still describes a pre-session token for login forms
([OWASP CSRF](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html)).
But the Origin check needs no session, so it already guards every unauthenticated POST. So does a `Sec-Fetch-Site`
check, and Go and Rails rely on this ([rails/rails#56350](https://github.com/rails/rails/pull/56350)). A passkey
login also answers a single-use challenge that Tetsu stores (ADR-0047). ADR-0048's "short-lived pre-session record
holds the token for forms before login" adds state without adding a signal.

## Session storage and expiry

**Hashed 256-bit tokens are the strongest storage pattern found.** Rails, Django, and Phoenix store session
identifiers in plain form. Only Lucia's successor code and OWASP's current cheat sheet store a SHA-256 verifier, as
Tetsu does ([Lucia](https://raw.githubusercontent.com/lucia-auth/lucia/refs/heads/main/code/auth_session.ts),
[OWASP sessions](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html),
[Phoenix](https://github.com/phoenixframework/phoenix/blob/main/priv/templates/phx.gen.auth/schema_token.ex.eex)).
ASVS 7.2.3 asks for 128 bits and NIST for 64
([ASVS V7](https://github.com/OWASP/ASVS/blob/master/5.0/en/0x16-V7-Session-Management.md)).

**The limits follow the standards, not framework defaults.** The operator realm's 8 hours absolute and 15 minutes
idle meet NIST SP 800-63B-4's AAL3 idle "SHOULD" of 15 minutes and its AAL2 overall limit of 24 hours
([NIST session](https://pages.nist.gov/800-63-4/sp800-63b/session/)). They sit inside OWASP's 4 to 8 hour absolute
range, at the floor of its 15 to 30 minute "low risk" idle band. OWASP's "high-value" band is 2 to 5 minutes. Every
framework surveyed uses limits measured in days, and most have one limit, sliding or absolute, not both. ADR-0048
already says why operators get the shortest limits. The addendum should add one sentence on why 15 minutes, not 5, is
enough for a realm that reaches every business.

**The per-request write is the outlier, and acceptable at operator scale.** Lucia writes at most hourly, Better Auth
once per `updateAge`, Phoenix weekly by reissue, and Django only when asked
([Better Auth](https://www.better-auth.com/docs/concepts/session-management),
[Django sessions](https://docs.djangoproject.com/en/dev/topics/http/sessions/)). A throttle of T seconds can only end
a session early, never late. A 60-second throttle on a 15-minute idle limit costs at most 6.7%. The update stays HOT,
with no index writes, while `last_seen_at` is in no index
([PostgreSQL HOT](https://www.postgresql.org/docs/current/storage-hot.html)). The migration indexes only
`operator_id`, so it qualifies today. With a handful of operators, a write per request costs nothing that matters.
It matters for the customer realm and for read replicas. Record it as a choice to revisit at slice 7.

**One clock decides, which no source requires but which is sound.** Phoenix compares against the database clock;
Django and Lucia use the app's. I found no guidance or incident either way. `use_session` reads `clock_timestamp()`
once, before any lock wait. As its comment says, a request is judged as of its own start, up to the scope's
`lock_timeout` late. The remaining risk is the database host's own time keeping.

**Cleanup is better than most and leaves one gap.** Django and Kratos ship no automatic purge and tell you to
schedule one ([Django sessions](https://docs.djangoproject.com/en/dev/topics/http/sessions/),
[Kratos cleanup](https://www.ory.com/docs/kratos/cli/kratos-cleanup-sql)). Tetsu deletes a spent session when it is
next presented, and starting a session deletes that operator's spent ones. An operator who never logs in again
leaves rows behind. They hold only a hash and timestamps, so this is a retention question, not a security one. It
matters more once rows carry a device label.

## Cookies

**`SameSite=Lax` with an explicit attribute is right.** OWASP prefers Strict but treats SameSite as defense in depth
only ([OWASP sessions](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html)). Strict
makes every emailed link land logged out. RFC 6265bis's remedy is a second Strict cookie for writes
([6265bis-19](https://www.ietf.org/archive/id/draft-ietf-httpbis-rfc6265bis-19.txt)). Chrome's two-minute
"Lax+POST" exception applies only to cookies with no SameSite attribute, so the existing test that pins the
attribute is worth keeping. `onrender.com` is on the Public Suffix List, so no other Render customer can toss cookies
at the operator host or count as same-site ([PSL](https://publicsuffix.org/list/public_suffix_list.dat)). On a
dedicated domain, a tenant site under the same registrable domain would be same-site. Lax would then stop nothing,
which is one more reason to give operators their own registrable domain.

**`__Host-` is enforced everywhere and matched without regard to case.** RFC 6265bis-19 matches the prefix
case-insensitively. Firefox fixed its case-sensitive check as CVE-2024-5699
([Bugzilla 1891349](https://bugzilla.mozilla.org/show_bug.cgi?id=1891349)), and the draft sits in the RFC Editor
queue ([datatracker](https://datatracker.ietf.org/doc/draft-ietf-httpbis-rfc6265bis/)). Tetsu compares names
exactly, so it ignores `__host-` variants, and it has a test for this.

**Nameless cookies are a browser problem; Unicode whitespace is Tetsu's.** A nameless cookie `=__Host-x=v`
serializes as `__Host-x=v`. Firefox fixed this in 105 and again in 138, and Chromium has rejected every `=a=b`
cookie since M148 in June 2026 ([Bugzilla 1779993](https://bugzilla.mozilla.org/show_bug.cgi?id=1779993),
[Blink-dev](https://groups.google.com/a/chromium.org/g/Blink-dev/c/SQUdQBMJqDk)). The newer attack works on the
server. A browser stores `\u{2000}__Host-name` as an ordinary cookie with no prefix rules, and a server that trims
Unicode whitespace reads it as the protected name. Django and ASP.NET are affected, and Safari still accepts U+0085
and U+00A0 in names ([PortSwigger Cookie Chaos](https://portswigger.net/research/cookie-chaos-how-to-bypass-host-and-secure-cookie-prefixes)).
Rust's `str::trim` strips Unicode White_Space, and `token_in` calls `pair.trim()`. If the page adapter decodes the
header as UTF-8, a planted cookie reaches `token_in` as the real one. The duplicate rule turns this into a logout
when a real cookie is present. With none present, it enables session fixation: the operator would act inside the
attacker's session. HTTP's cookie grammar trims only space and tab, and `http::HeaderValue::to_str` refuses any byte
outside visible ASCII, so fixing the trim and the adapter together closes it.

**Split headers and duplicates are handled correctly.** HTTP/2 lets a browser split `Cookie` into several fields
that the server must treat as one ([RFC 9113 §8.2.3](https://www.rfc-editor.org/rfc/rfc9113.txt)). `token_in` takes
them all. axum-extra reads them with `get_all(COOKIE)`, so `HeaderMap::get` would miss a split cookie
([axum-extra](https://raw.githubusercontent.com/tokio-rs/axum/main/axum-extra/src/extract/cookie/mod.rs)). RFC
6265bis says servers SHOULD NOT rely on the order of same-name cookies. Go takes the first, and Django and
axum-extra take the last. Treating a duplicate as no cookie is stricter and fails safe. It works only if the parser
never folds two distinct names into one, which is the trim bug again.

## Session controls

**ASVS 5.0.0 asks for four things Tetsu does not have yet.** The text, all Level 2
([ASVS V7](https://raw.githubusercontent.com/OWASP/ASVS/master/5.0/en/0x16-V7-Session-Management.md)):

| Item | Asks | Tetsu today |
| --- | --- | --- |
| 7.1.2 | Document the concurrent sessions allowed per account and what happens at the limit | Undocumented; unlimited |
| 7.4.2 (L1) | End all sessions when an account is disabled or deleted | No disable or delete; the foreign key blocks deleting an operator who has sessions |
| 7.4.3 | Offer to end all other sessions after any change to an authentication factor | Nothing |
| 7.4.5 | Administrators can end one user's sessions, or everyone's | Nothing |
| 7.5.2 | Users view their sessions and, after authenticating again, end any or all | Nothing; the hash-only lookup cannot list |

**Session caps are rare and cut both ways.** Keycloak is the only product found that offers one: 3 per realm when
turned on, denying new sessions by default
([Keycloak flows](https://raw.githubusercontent.com/keycloak/keycloak/main/docs/documentation/server_admin/topics/authentication/flows.adoc)).
"Deny new" locks out an operator whose old device is lost. "End oldest" lets an attacker evict the owner. Since 7.1.2
asks only for documentation, "no cap, with each session listed and endable" is compliant.

**"End all others, keep this one" is the common pattern.** On a credential change, Devise, Django, and Google force
it, while Keycloak, Kratos, and Better Auth offer it
([Ory hooks](https://www.ory.com/docs/kratos/hooks/configure-hooks),
[Devise](https://raw.githubusercontent.com/heartcombo/devise/main/lib/devise/models/database_authenticatable.rb),
[Google](https://support.google.com/accounts/answer/41078)). Forcing it meets "gives the option" and is simpler.
Kanidm goes further: deleting a credential ends every session it started
([Kanidm](https://raw.githubusercontent.com/kanidm/kanidm/master/book/src/accounts/authentication_and_credentials.md)).
NIST requires a notice when an authenticator is added but no session termination; that duty is ASVS's
([NIST events](https://pages.nist.gov/800-63-4/sp800-63b/events/)).

**Session lists show a device label, place, and times.** Keycloak, Linear, Kratos, and Better Auth show the browser
and OS, IP or coarse location, last active, login date, and a "current" badge, with per-row revoke and "end all
others" ([Linear](https://linear.app/docs/security-and-access.md),
[Ory sessions](https://www.ory.com/docs/kratos/session-management/list-revoke-get-sessions)). IP addresses are
personal data, so store only what the list shows, coarsened, and delete it with the session. Kratos keeps users'
endpoints scoped to their own sessions and gives administrators separate ones. That fits Tetsu's `SECURITY DEFINER`
pattern: functions that derive the operator from the presented hash and take no account id.

## Weakest evidence

- That `SameSite=Lax` blunts BREACH rests on a vendor page and reasoning. No 2023 to 2026 study covers it, and none
  compares brotli with gzip.
- Sec-Fetch-Site's handling of a cross-site redirect hop, which would catch CVE-2026-8950, comes from memory of the
  Fetch Metadata spec and was not checked.
- That hyper passes split HTTP/2 cookie fields through unjoined is inferred from axum-extra needing `get_all`.
- Chromium's case-insensitive prefix match and the status of Lax+POST come from search summaries and a
  self-published post.
- No source sets a write throttle for a short idle limit, or settles database clock against app clock.
- ASVS was read from the `master` copy of 5.0, not the `v5.0.0` tag. Keycloak 26.3's default change came from a
  search summary.

## Recommendations

### Fixes to code already pushed

1. **`Realm::token_in` trims only space and tab:** `pair.trim_matches([' ', '\t'])`. Add tests that names prefixed
   with U+2000, U+00A0, and U+0085 yield `None`.
2. **Mask the form token per response.** `form_token` takes a random pad and returns `pad || (pad XOR token)` as
   hex. `form_token_matches` unmasks and compares in constant time. Test that two renders differ and both pass.
3. **Add a `sessions.id` public identifier,** random and unique, never the hash, for the session list in step 4, item 7. The
   migration is unapplied, so this is an edit, not a new migration.

### Step 3: the enrollment command

1. **Add commands to end sessions:** one operator's, and every session in the operator realm (7.4.5). Back them with
   `end_operator_sessions(operator, realm)` and `end_realm_sessions(realm)`, granted only to the command's role, each
   recording an audit event.
2. **Removing or disabling an operator ends its sessions in the same transaction** (7.4.2). The foreign key without
   `ON DELETE` already refuses a removal that forgets; keep it, and test it.
3. **Re-enrollment after lost passkeys ends every session of that operator.** It is recovery, and recovery implies
   possible compromise.

### Step 4: passkey login and the operator pages

1. **Read cookies with `HeaderMap::get_all(COOKIE)` and `HeaderValue::to_str`,** skipping any header that fails.
   Never use `get`, and never decode lossily.
2. **Add `Sec-Fetch-Site` to `Change`.** Present and not `same-origin` on an unsafe method is refused. Absent defers
   to the Origin check, which already refuses a missing Origin.
3. **Apply the Origin and Fetch Metadata checks to every unsafe request,** including passkey start and finish, which
   have no session. Use no pre-session token.
4. **On login, end any session the browser presented,** then start a new one (7.2.4). Logout stays a POST, and
    every logged-in page shows it (7.4.4).
5. **Serve pages carrying a form token with `Cache-Control: no-store`,** and keep every GET free of state changes,
    since Lax sends the cookie on cross-site navigations.
6. **After adding or removing a passkey, end the operator's other sessions** and keep the current one (7.4.3).
    #110 allows two passkeys, so adding the second one triggers this.
7. **A "your sessions" page** lists live sessions with start, last seen, a coarse device label, and a current badge.
    It ends one or all others after a fresh passkey login, using ADR-0048's 10-minute freshness (7.5.2). If it does
    not fit #110, open it as a follow-up before slice 9's Level 2 review.

### The ADR-0048 addendum

1. **7.1.2:** operators have no session cap. Each session is listed and endable, and the limits are short. A cap
    would lock out an operator whose old device is lost, or let an attacker evict the owner.
2. **7.4.2:** disabling or removing an operator, staff member, or customer ends all its sessions in the same
    transaction.
3. **7.4.3:** adding or removing a passkey, or completing recovery, ends the account's other sessions without asking.
    Recovery ends all of them.
4. **7.4.5:** for operators, the operator command is the administrator. It ends one operator's sessions or the whole
    realm's. Who ends staff sessions is decided in slice 6.
5. **7.5.2:** list fields as in step 4, item 7, stored coarsely and deleted with the session. The list is reached only
    through functions that derive the account from the presented token.
6. **CSRF:** replace the pre-session record with the Origin and `Sec-Fetch-Site` checks on unauthenticated forms.
    State that the form token is masked per response.
7. **Sessions:** record the per-request write as a choice to revisit at slice 7, the database clock as the one
    clock, and that spent rows of operators who never return stay until a sweep, which slice 7 adds. Add a sentence
    on why 15 minutes idle suffices, given OWASP's 2 to 5 minute band for high-value apps.

## Sources

All read 2026-11-14.

- OWASP, [Cross-Site Request Forgery Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html),
  and its [commit history](https://github.com/OWASP/CheatSheetSeries/commits/master/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.md).
- OWASP, [Session Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html).
- OWASP, [ASVS 5.0 V7 Session Management](https://github.com/OWASP/ASVS/blob/master/5.0/en/0x16-V7-Session-Management.md),
  `master` copy.
- NIST SP 800-63B-4, [Session management](https://pages.nist.gov/800-63-4/sp800-63b/session/),
  [Authenticator events](https://pages.nist.gov/800-63-4/sp800-63b/events/).
- F. Valsorda, [Cross-Site Request Forgery](https://words.filippo.io/csrf/), 2025-08-13; Go,
  [`CrossOriginProtection`](https://pkg.go.dev/net/http#CrossOriginProtection).
- Rails, [PR #56350](https://github.com/rails/rails/pull/56350) and
  [request_forgery_protection.rb](https://raw.githubusercontent.com/rails/rails/main/actionpack/lib/action_controller/metal/request_forgery_protection.rb).
- Django, [CSRF](https://docs.djangoproject.com/en/stable/ref/csrf/) and
  [sessions](https://docs.djangoproject.com/en/dev/topics/http/sessions/).
- MDN, [Origin](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Origin) and
  [Sec-Fetch-Site](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Sec-Fetch-Site).
- Mozilla bugs [1965430](https://bugzilla.mozilla.org/show_bug.cgi?id=1965430) (CVE-2026-8950),
  [1891349](https://bugzilla.mozilla.org/show_bug.cgi?id=1891349) (CVE-2024-5699), and
  [1779993](https://bugzilla.mozilla.org/show_bug.cgi?id=1779993) (CVE-2022-40958).
- [BREACH](https://breachattack.com/), 2013.
- IETF, [RFC 6265bis-19](https://www.ietf.org/archive/id/draft-ietf-httpbis-rfc6265bis-19.txt) and its
  [datatracker page](https://datatracker.ietf.org/doc/draft-ietf-httpbis-rfc6265bis/);
  [RFC 9113](https://www.rfc-editor.org/rfc/rfc9113.txt).
- Chromium, [Blink-dev PSA on nameless cookies](https://groups.google.com/a/chromium.org/g/Blink-dev/c/SQUdQBMJqDk),
  2026-06-04.
- PortSwigger, [Cookie Chaos](https://portswigger.net/research/cookie-chaos-how-to-bypass-host-and-secure-cookie-prefixes),
  2025-09-03.
- [Public Suffix List](https://publicsuffix.org/list/public_suffix_list.dat).
- axum-extra, [cookie extractor](https://raw.githubusercontent.com/tokio-rs/axum/main/axum-extra/src/extract/cookie/mod.rs).
- Lucia, [auth_session.ts](https://raw.githubusercontent.com/lucia-auth/lucia/refs/heads/main/code/auth_session.ts);
  Phoenix, [phx.gen.auth token schema](https://github.com/phoenixframework/phoenix/blob/main/priv/templates/phx.gen.auth/schema_token.ex.eex);
  [Better Auth sessions](https://www.better-auth.com/docs/concepts/session-management).
- PostgreSQL 18, [Heap-only tuples](https://www.postgresql.org/docs/current/storage-hot.html).
- Ory Kratos, [cleanup](https://www.ory.com/docs/kratos/cli/kratos-cleanup-sql),
  [hooks](https://www.ory.com/docs/kratos/hooks/configure-hooks), and
  [session listing](https://www.ory.com/docs/kratos/session-management/list-revoke-get-sessions).
- Keycloak, [authentication flows](https://raw.githubusercontent.com/keycloak/keycloak/main/docs/documentation/server_admin/topics/authentication/flows.adoc);
  Kanidm, [credentials](https://raw.githubusercontent.com/kanidm/kanidm/master/book/src/accounts/authentication_and_credentials.md);
  Devise, [database_authenticatable.rb](https://raw.githubusercontent.com/heartcombo/devise/main/lib/devise/models/database_authenticatable.rb);
  Google, [Account Help 41078](https://support.google.com/accounts/answer/41078);
  Linear, [Security and access](https://linear.app/docs/security-and-access.md).
- Tetsu, `crates/identity` at `78424fb`: `src/browser.rs`, `src/domain.rs`, and `migrations/0001_operators.sql`.
