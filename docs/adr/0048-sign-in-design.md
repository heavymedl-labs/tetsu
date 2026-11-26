# 0048. Login design

- Status: Accepted
- Failed passkey login row superseded by [ADR-0072](0072-rate-limits-by-network-address.md)
- Date: 2026-11-11
- Session controls, the checks on state-changing requests, and the pre-session record are amended by
  [ADR-0059](0059-session-controls-and-request-checks.md)
- Realms share one demo service until Tetsu has domains, by [ADR-0061](0061-one-service-until-domains.md);
  [ADR-0092](0092-a-service-per-realm-before-domains.md), proposed, gives each realm its own service and host instead

## Context

[ADR-0040](0040-identity-and-sign-in.md) and [ADR-0041](0041-account-recovery.md) decide identity and recovery, and
leave details to [#20](https://github.com/heavymedl-labs/tetsu/issues/20): staff roles, session timeouts, how one
host keeps several businesses' sessions, rate limits, and the questions in ADR-0041's "What #20's design must
answer". [ADR-0042](0042-tenant-isolation.md), [ADR-0043](0043-domain-event-delivery.md),
[ADR-0044](0044-email-delivery.md), [ADR-0045](0045-operations-baseline.md), and
[ADR-0047](0047-sign-in-libraries.md) supply storage, events, email, logging, and libraries. This ADR settles the
details, so #20 can be broken into build tasks.

Sign-ups and guest booking stay off until the Level 2 review, so on the demo every account is invited by command,
and only fictional people log in.

## Decision

### The Identity module

- **A new `identity` crate and schema** holds staff identities, staff roles, customer accounts, operators,
  passkeys, Google links, login attempts, sessions, recovery codes, recovery requests, and relinks. Client
  profiles stay a reference by ID until a Clients module owns them.
- **Staff identities, their passkeys, operators, and sessions are global tables.** Staff roles, customer accounts
  and their passkeys, recovery requests, and relinks are tenant-owned (ADR-0042).
- **Sessions are found through a lookup function** that takes a token's hash and returns one session, as ADR-0042
  requires of anything read before a tenant is known.
- **A separate `audit` crate** consumes Identity's events and keeps the audit record per tenant, with global
  entries for operators (ADR-0045).

### Staff roles

ADR-0040 leaves the roles to #20. A staff member holds one role at each business:

| Role | May |
| --- | --- |
| Owner | Everything, including roles, invites, exports, retiring accounts, and settling relinks |
| Manager | Run the business day to day, invite front desk and instructors, start and settle relinks, retire accounts |
| Front desk | Check people in, manage bookings, start a relink after checking ID |
| Instructor | See and run their own sessions and attendance |

- **A relink is settled by an owner or manager who did not start it** (ADR-0041). A business with one such person
  has an owner settle it; a business whose only senior staff member started it waits for another to join, and is
  told so.
- **Every authorization check names a permission,** not a role, so roles can change later without touching checks
  (`v5.0.0-8.2.1`). Each permission has a negative test.

### Sessions

- **Tokens are 256 random bits,** stored only as SHA-256 hashes (ADR-0047), sent in `__Host-` cookies that are
  `Secure`, `HttpOnly`, and `SameSite=Lax` (`v5.0.0-3.3.3`).
- **Each realm has its own cookie.** A customer's cookie holds one browser token, and the server keeps one customer
  session per business under it, so Lee Park logged in at Anvil Row and Juniper Lane holds two separate sessions,
  each checked against the business in the path (`v5.0.0-8.4.1`).
- **Timeouts,** with NIST SP 800-63B-4's Level 2 limits of 24 hours and 1 hour idle as the ceiling
  (`v5.0.0-7.1.1`):

  | Realm | Absolute | Idle | Why |
  | --- | --- | --- | --- |
  | Customer | 24 hours | 1 hour | Level 2's ceiling; a passkey login is one touch |
  | Staff | 12 hours | 1 hour | One working day at a front desk |
  | Operator | 8 hours | 15 minutes | Reaches every business |

- **Logging out ends the session on the server,** and a fresh passkey login within 10 minutes counts as "fresh"
  for the changes ADR-0040 and ADR-0041 guard (`v5.0.0-7.5.1`).

### Forms and pages

- **Every state-changing request checks its `Origin` header against the realm's hosts and carries a per-session
  token,** and a short-lived pre-session record holds the token for forms before login (`v5.0.0-3.5.1`).
- **Login pages keep `frame-ancestors 'none'`,** and the content security policy gains `script-src 'self'`,
  `connect-src 'self'`, and a `form-action` that allows Tetsu and Google's login (`v5.0.0-3.4.3`).
- **The passkey script is TypeScript,** built by Vite, and waits on CONSTRAINTS.md's exception E5 closing.

### Rate limits

Documented as `v5.0.0-6.1.1` asks. Limits on sending key on the email together with the requester, never the email
alone, so a stranger cannot use up an owner's quota. Past a ceiling, Tetsu slows down rather than locks out
(`v5.0.0-6.3.1`).

**Email codes are 8 digits,** about 26.6 bits, above ADR-0040's floor of 20.

| What | Limit |
| --- | --- |
| Email codes sent | 3 per attempt, 10 per email and network address per hour, 30 per network address per hour |
| Code guesses | 5 per code, then the attempt ends |
| Failed code guesses per email, across every attempt | After 20 in 24 hours, a new code for that email waits, starting at 1 minute and doubling to 1 hour |
| Failed passkey logins | 10 per network address per 15 minutes, then a growing delay |
| Recovery requests | One pending per account, 3 per email and network address per day |
| Notices | Never limited or batched; sent at least once per event (ADR-0043) |

- **The per-email ceiling bounds guessing.** At its slowest, one email takes about 120 wrong guesses a day, so the
  chance of guessing one target's code within a year is about 1 in 2,300.
- **The real owner always has a way past a stranger's requests.** A passkey login is never slowed by code
  limits. And the "this wasn't me" link in a recovery notice, which only the email's holder receives, cancels the
  pending request and lets the owner start their own at once, past the recovery limits.

### Answers to ADR-0041's questions

- **What cancels an email recovery:** a passkey login to the account, a recovery code used on it, or the
  "this wasn't me" link in its notice, after the request was made. The requester holds none of these, so they
  cannot cancel their own request by accident. An attacker who holds the account can cancel it, as ADR-0041
  accepts, and the real person then relinks in person, which the attacker can only dispute.
- **Disputes after a settlement:** a settled relink stands. New evidence starts a new relink, with a new wait.
  While a relink is paused, the old account keeps its own bookings but cannot open the relinked profiles' history
  or waivers, and an owner or manager may suspend it until the settlement, which ends its sessions and sends a
  notice.
- **Bookings made on the new account before its empty profile is removed** move to the relinked profile when the
  relink completes, and stay where they are if it is refused.
- **What each lock shows:** a locked account always sees its own bookings, upcoming sessions, settings, and
  notices. ADR-0041's lists say what it cannot open.
- **Who may act on a relink:** front desk, managers, and owners start one; any linked account, or any staff member
  of the business, may dispute; an owner or manager who did not start it settles it; owners and managers retire
  accounts.

### The demo

- **The maintainer invites demo staff and customers** through the operator command that ADR-0040 sets, run once per
  fictional person, so every demo login uses a real passkey. Email to fictional people lands in the capture
  mailbox, and email to the maintainer goes out for real (ADR-0044).
- **Each realm is its own free demo service** until Tetsu has domains (ADR-0042), so ADR-0040's device test runs
  on the demo's realm hosts now and again on Tetsu's domains before the Level 2 review.

### Build order

Login is built in slices, each its own issue under #20, test first, each leaving the demo working:

1. Tenant isolation in the Postgres crate: roles, the tenant transaction, catalog checks, the version floor, and
   Tenancy moved onto them ([#63](https://github.com/heavymedl-labs/tetsu/issues/63),
   [#25](https://github.com/heavymedl-labs/tetsu/issues/25))
2. Structured logs, and the outbox, inbox, and dispatcher, with the audit consumer
3. TypeScript tests, coverage, and boundaries in `web/`, closing E5
4. The ADR-0047 spike: the library failure tests
5. Operators: the invite command, passkey login, sessions, logout, and the capture mailbox page
6. Staff: roles, invites, recovery codes on joining, passkey login, switching business
7. Customers: accounts per business, invites by command on the demo, login on each business's pages
8. Recovery: codes, email recovery, relinks, locks, and notices (ADR-0041)
9. The device test, and the Tier 1 and ASVS review of the whole
10. Google login, before sign-ups open

## Alternatives considered

- **One cookie per business,** with a path attribute. `__Host-` cookies must use path `/`, so this would give up
  the prefix's protection.
- **Longer customer sessions,** such as 30 days as many shops use. That is beyond NIST's Level 2 ceiling, which
  ASVS 5.0.0 cites for session timeouts. A passkey makes logging in again a single touch.
- **Roles as free-form permission sets per business.** Flexible, but nothing in the reference businesses needs it
  yet, and four fixed roles are easier to test.

## Consequences

- #20 becomes a parent issue with one sub-issue per slice, in that order.
- Recovery (slice 8) and Google login (slice 10) complete before the Level 2 review, not before the first demo
  login.
- The audit crate is the first event consumer, so slice 2 proves ADR-0043 before login depends on it.
- Revisit the roles when the first real business asks for one that does not fit.

## Sources

- NIST SP 800-63B-4, [Digital Identity Guidelines: Authentication and Authenticator Management](https://pages.nist.gov/800-63-4/sp800-63b.html),
  session reauthentication at AAL2.
- OWASP [Application Security Verification Standard 5.0.0](https://github.com/OWASP/ASVS/tree/v5.0.0), chapters
  V3, V6, V7, and V8.
- OWASP [Cross-Site Request Forgery Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html).
- IETF, [RFC 6265bis, cookie prefixes](https://datatracker.ietf.org/doc/draft-ietf-httpbis-rfc6265bis/).
