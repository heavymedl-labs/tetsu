# 0059. Session controls and cross-site request checks

- Status: Proposed
- Date: 2026-11-14
- Amends [ADR-0048](0048-sign-in-design.md): its "Sessions" and "Forms and pages" sections, and replaces its
  pre-session record

## Context

ADR-0048 sets sessions, timeouts, and the checks on state-changing requests, and leaves out four ASVS 5.0.0 Level 2
requirements for sessions: 7.1.2 (a stated limit on concurrent sessions), 7.4.3 (end other sessions after a factor
changes), 7.4.5 (administrators end sessions), and 7.5.2 (people see and end their own sessions), with 7.4.2 (end all
sessions when an account is disabled or removed) at Level 1. The fresh reviews of
[#110](https://github.com/heavymedl-labs/tetsu/issues/110)'s first steps raised them.

The [session and CSRF review](../research/identity/session-and-csrf-review.md) of 2026-11-14 found the design in
line with current practice, and practice moved in 2025 and 2026: OWASP made Fetch Metadata (`Sec-Fetch-Site`) a
primary CSRF defense, Rails 8.2 and Go 1.25 check it without tokens, and a page that renders an unmasked token beside
reflected input meets BREACH's preconditions. It also found that almost no product caps concurrent sessions, and that
ASVS 7.1.2 asks only that the policy be stated.

## Decision

### Session controls

- **No cap on concurrent sessions,** in any realm (`v5.0.0-7.1.2`). Each session is listed and can be ended, and
  the limits are short. A cap would lock out a person whose old device is lost, or let an attacker who signs in evict
  the owner.
- **Disabling or removing an account ends all its sessions in the same transaction** (`v5.0.0-7.4.2`), for
  operators, staff, and customers.
- **Adding or removing a passkey, or completing recovery, ends the account's other sessions,** keeping the one that
  made the change, without asking (`v5.0.0-7.4.3`). Recovery ends every session, the current one too, since it may
  follow a compromise. ASVS asks only for the option; ending them always is the stricter choice.
- **Administrators end sessions** (`v5.0.0-7.4.5`). For operators, ADR-0040's operator command is the
  administrator: it ends one operator's sessions or the whole realm's, and each use is audited. Who ends staff and
  customer sessions is decided with them, in slices 6 and 7.
- **People see and end their own sessions** (`v5.0.0-7.5.2`): each session's start, last request, a coarse device
  label, and which one is current, shown by each session's random public id, never by its token's hash. Ending one,
  or all the others, needs a fresh passkey sign-in, within ADR-0048's 10 minutes. The list is reached only through
  functions that find the account from the presented session's token, so no request can list another account's
  sessions. The device label is stored only as coarsely as the list shows it, and deleted with the session.

### Checks on state-changing requests

- **Every state-changing request passes two checks,** whether or not it has a session: its `Origin` must exactly
  match one of the realm's origins, and its `Sec-Fetch-Site`, when present, must be `same-origin`. A missing or
  `null` Origin is refused. Two independent signals keep a single browser bug, such as Firefox's CVE-2026-8950, from
  opening a gap.
- **A signed-in request also carries the session's form token,** masked with a fresh random pad on every page, so a
  compressed page never repeats it.
- **Forms before sign-in carry no token.** The two header checks guard them, and need no state, so ADR-0048's
  pre-session record is dropped.
- **No GET changes state,** since `SameSite=Lax` sends the cookie on cross-site navigations, and pages that carry a
  form token are served with `Cache-Control: no-store`.
- **Signing in ends any session the browser presented,** before starting a new one (`v5.0.0-7.2.4`).

### Sessions, as built

- **Postgres's clock is the only clock** for sessions, with each realm's limits kept in the SQL beside it.
- **Each signed-in request writes its last request,** in the same call that checks the session. For a few operators
  the write costs nothing that matters; frameworks that serve many users throttle it, and slice 7 revisits it when
  customers arrive.
- **Spent sessions are removed when presented, or when their account next signs in.** Those of an account that never
  returns stay, as hashes no request can use, until slice 7 adds a sweep.
- **Fifteen minutes idle stays the operator limit.** OWASP suggests 2 to 5 minutes for high-value applications, but
  NIST SP 800-63B-4 sets 15 minutes at its highest assurance level, AAL3, and an operator's every change still needs
  the form token and the header checks.

## Alternatives considered

- **A cap on concurrent sessions,** such as Keycloak's limiter, evicting the oldest or refusing the newest. It
  locks out an owner whose device is lost, or lets an attacker evict the owner, for no gain the session list does not
  give.
- **Ask before ending other sessions after a passkey change,** as ASVS allows and Keycloak does. Leaves a stolen
  session running when the person forgets to tick it.
- **Retire the form token,** relying on the header checks, as Rails 8.2's default and Go 1.25 do. OWASP still names
  the token the default for server-side sessions, and it costs little now that it is masked.
- **Keep the pre-session record.** State that the header checks make unnecessary.
- **Throttle the last-request write now.** Saves writes Tetsu does not yet need to save, and makes the idle limit
  less exact.

## Consequences

- #110's later steps build the operator command's session controls, the `Sec-Fetch-Site` check, the session list,
  and ending sessions on passkey changes and sign-in. The session list may move to a follow-up issue, done before
  slice 9's Level 2 review.
- Staff and customer realms follow the same rules, with their administrators decided in slices 6 and 7.
- Revisit the per-request write and the sweep at slice 7, and the idle limit if operators' actions widen.

## Sources

- [Operator sessions and CSRF: a practice review](../research/identity/session-and-csrf-review.md), 2026-11-14,
  and its sources.
- OWASP, "Application Security Verification Standard 5.0.0", chapter V7,
  <https://github.com/OWASP/ASVS/tree/v5.0.0_release/5.0/en>.
- OWASP, [Cross-Site Request Forgery Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html),
  read 2026-11-14.
- NIST, "SP 800-63B-4: Authentication and Authenticator Management", July 2025,
  <https://pages.nist.gov/800-63-4/sp800-63b.html>.
