# Operator lifecycles and their extensions

Research on 2026-11-17 for [#110](https://github.com/heavymedl-labs/tetsu/issues/110), after step 4d shipped. It
maps each thing an operator's login passes through as a lifecycle, lists its extensions (every way off the main
flow), and compares how Tetsu handles each with current practice. The timeout values themselves were settled in the
[session review](session-and-csrf-review.md) and [ADR-0048](../../adr/0048-sign-in-design.md); this note is about what
a person sees and can do at each edge.

## Summary

The server side is sound: every limit is enforced in Postgres, and every refusal is safe. The gaps are in what the
pages tell a person and where they send them. Five are worth building now:

1. **A logged-in operator who opens the login page goes to their page**, as Django's `redirect_authenticated_user`
   does. Its fingerprinting warning does not apply, since the operator cookie is `SameSite=Lax` and no other site's
   image request carries it.
2. **A warning two minutes before the idle limit,** with "Stay logged in" and "Log out", as HMRC's service timeout
   pattern does. WCAG 2.2.1, level A, requires a warning and a simple way to extend.
3. **At the idle limit, the page goes to the login page and says why:** "For your security, we logged you out."
4. **An action refused because the session ended offers to log in,** rather than only saying so.
5. **An invite that cannot be used says what to do next,** and logging out clears the browser's cached pages.

The warning needs one new piece: a way to ask how long a session has left **without** extending it. NIST counts
only the subscriber's activity, and OWASP warns that automatic requests let a stolen session live forever, so a
timer that renewed the session as it checked would defeat the idle limit.

## Lifecycles

| Thing | States | Ends by |
| --- | --- | --- |
| Invite link | open | used (joined), an hour passing, a newer invite, the operator disabled |
| Join or login ceremony | begun | finished, ten minutes passing, the island's 60 second limit, a newer press |
| Session | live, fresh for its first 10 minutes | logging out, 15 minutes idle, 8 hours, a disable, a new login in that browser, adding a passkey (ends the others) |
| Operator | invited, joined (one or two passkeys), disabled | `remove-passkeys` returns them to invited; enable lets them log in afresh |

## Extensions

Each row is a state and an event off the main flow. "Today" is the behavior at commit `abbde04`.

| # | State, then event | Today | Practice | Sources |
| --- | --- | --- | --- | --- |
| E1 | Logged in, opens `/ops/login` | Shows the login page; logging in again replaces the session | Redirect to the app | [Django](https://docs.djangoproject.com/en/5.2/topics/auth/default/) |
| E2 | Logged in, 13 minutes idle | Nothing | Warn with two minutes left, a seconds countdown in the last minute, focus on "Stay logged in", Escape stays | [HMRC](https://design.tax.service.gov.uk/hmrc-design-patterns/service-timeout), [WCAG 2.2.1](https://www.w3.org/WAI/WCAG22/Understanding/timing-adjustable.html) |
| E3 | Page open, idle limit passes | The page stays; the next click fails | Go to the login page, which says the person was logged out for their security | [HMRC](https://design.tax.service.gov.uk/hmrc-design-patterns/service-timeout), [OWASP](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html) |
| E4 | Logged in, 8 hour limit near | Nothing | Cannot be extended; warn, then log in again, which resets both limits | [NIST 800-63B-4](https://pages.nist.gov/800-63-4/sp800-63b/session/) |
| E5 | Session ended, presses an action | Red "you are logged out; log in again" | Offer the login, and keep what was being done | [WCAG 2.2.5](https://www.w3.org/WAI/WCAG22/Understanding/re-authenticating.html) |
| E6 | Logged out, opens a deeper `/ops` page | Login, then always `/ops` | Return to the page asked for, through a relative `/ops` path checked on the server | [Django](https://docs.djangoproject.com/en/5.2/topics/auth/default/), [OWASP](https://cheatsheetseries.owasp.org/cheatsheets/Unvalidated_Redirects_and_Forwards_Cheat_Sheet.html) |
| E7 | Two tabs, one logs out or stays active | The other tab is stale until used | Tell other tabs over a `BroadcastChannel`, and check again when a tab is shown | [HMRC](https://design.tax.service.gov.uk/hmrc-design-patterns/service-timeout) (`data-synchronise-tabs`) |
| E8 | Logged out, presses Back | `no-store` keeps the page out of the back-forward cache | Also `Clear-Site-Data: "cache"` on logout, and a `pageshow` check | [OWASP](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html), [web.dev](https://web.dev/articles/bfcache) |
| E9 | Opens an invite that is used, old, or replaced | "This invite link has ended or is not valid." after pressing | Say what happened and the next step: ask for a new invite, or log in if already joined | [Auth0 community](https://community.auth0.com/t/error-message-for-expired-invitations-should-tell-users-the-invitation-expired-and-what-to-do/115114), [Wristband](https://docs.wristband.dev/changelog/ux-improved-for-expired-invitations) |
| E10 | Disabled while logged in | The next request ends the session and goes to login; login is refused generically | Same; a refusal never says an account is disabled | [OWASP](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html) |
| E11 | Logs in on a second device | Both sessions live | Show active sessions and let one end the others | #145 |
| E12 | Device finds no passkey, or the prompt is cancelled | Hint at 15 seconds, closed at 60, a red message | Same; WebAuthn hides whether a passkey exists | [ADR-0067](../../adr/0067-sign-in-after-a-first-passkey.md) |
| E13 | Joined, the chained login fails | The button turns to "Continue with passkey" | Same | [ADR-0067](../../adr/0067-sign-in-after-a-first-passkey.md) |

## Findings that shape the design

- **Activity is the person's, not the page's.** NIST: "Session activity SHALL reset the inactivity timeout", and the
  relying party "MAY alert the subscriber that the session is about to be terminated and allow the subscriber to
  make the session active or reauthenticate". OWASP warns that periodic requests let a hijacked session live on. So
  the page's clock must read the session's time left without renewing it, and only "Stay logged in" renews.
- **Each tab cannot keep its own clock.** A tab left open while the operator works in another would warn and log
  out a live session. HMRC syncs tabs over a `BroadcastChannel` named `session-activity`. Asking the server before
  acting is the sure check, since a message misses a sleeping tab.
- **The warning is a dialog with a countdown.** HMRC moves focus to the dialog, reads it, then focuses the button,
  and tested it with JAWS, NVDA, ZoomText, and VoiceOver. DWP and the Home Office reuse it with two minutes' notice.
  The Home Office notes open questions with screen readers on mobile.
- **The logged-out page says what happened.** HMRC's "For your security, we signed you out" page carries a "Sign
  in" button. Tetsu's login page can say the same from a query flag that names no person, such as `?ended=idle`.
- **The absolute limit cannot be extended.** NIST sets it at 12 hours for AAL3 and has reauthentication reset both
  limits. A warning before it, and a login that returns the operator where they were, is all a page can do.
- **WCAG 2.2.5 and 2.2.6 (AAA)** ask that re-authenticating loses no data, and that a person is told up front how
  long inactivity may last. Operator pages hold little unsaved work today; forms that do will need it.
- **`Clear-Site-Data`** is widely supported since September 2023, per MDN. Its `"cookies"` value clears every cookie
  for the site, which would log a person out of another realm on the same host, so `"cache"` alone fits, beside the
  cookie Tetsu already expires.

## Recommendations

1. **Now, small:** E1 redirect, E9 wording ("This invite has ended. Ask an operator for a new one."), and E8's
   `Clear-Site-Data: "cache"` on logout.
2. **Now, one island:** a `<tetsu-session>` island on operator pages for E2, E3, E5, and E7. It reads the time left
   from a new `GET /ops/session` that does not renew, warns with two minutes left, renews only on "Stay logged in"
   (a form-token POST through `use_session`), shares state with other tabs over a `BroadcastChannel`, checks again
   on `visibilitychange` and `pageshow`, and sends a logged-out page to `/ops/login?ended=idle`.
3. **Later:** E4's absolute warning with the island, E6's return path once a second operator page exists, and E11
   with #145.

## Gaps

- No source tests a timeout warning with passkey reauthentication rather than a password.
- HMRC does not say whether its keep-alive extends on a timer or only on the button.
- The `BroadcastChannel` sources are engineering blogs, not standards.
