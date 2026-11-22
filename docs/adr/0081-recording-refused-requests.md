# 0081. Recording refused requests

- Status: Accepted
- Date: 2026-11-22
- Adds two limits to [ADR-0072](0072-rate-limits-by-network-address.md)'s table

## Context

Identity refuses a state-changing request that comes from another origin, carries no session cookie, or lacks
its session's form token, and a session id in no form an id takes ([ADR-0059](0059-session-controls-and-request-checks.md)).
Each refusal answered as forbidden, but nothing recorded it ([#152](https://github.com/heavymedl-labs/tetsu/issues/152)).
ASVS 5.0 asks for failed authorization attempts (`v5.0.0-16.3.2`) and attempts to get past a security control
(`v5.0.0-16.3.3`) to be logged, and
[ADR-0045](0045-operations-baseline.md) sends security events to the audit record rather than the log.

Anyone can send these requests, so recording each one lets anyone grow the audit record, which nothing yet removes
([#22](https://github.com/heavymedl-labs/tetsu/issues/22)).

## Decision

- **Every request refused at a check is recorded** as `identity.request_refused`, naming its route, the check it
  failed (`origin`, `cookie`, `form_token`, or `field`), and the device it came from: logging out, ending sessions,
  adding a passkey, staying logged in, and the steps before login.
- **The origin is checked first,** then the cookie, then the form token. The origin holds no secret, and a browser
  sends no `SameSite=Lax` cookie with another site's form, so another site's request is recorded as one. On a route
  that takes JSON, the origin is checked before the body, since another site's form is what a browser sends
  without asking.
- **A refused request names no operator,** even when its cookie is a live session's: it proved nothing, so naming
  the session's owner would put an action they never took in their record.
- **Each network may have ten refusals recorded in fifteen minutes,** under a `refusal` limit counted as
  ADR-0072's are. Past that its requests are still refused, and the first of each window, under a
  `refusal_flood` limit of one in fifteen minutes, records `identity.refusals_suppressed`, so a silent stretch is
  never silent in the record.
- **A refusal that cannot be recorded** answers unavailable, never refused unseen.

| Limit | Room | Span |
| --- | --- | --- |
| `refusal` | 10 | 15 minutes |
| `refusal_flood` | 1 | 15 minutes |

- **The bound is per network,** as every ADR-0072 limit is: an IPv4 address, or an IPv6 /64. Someone holding
  many networks can still add many records, about 1,000 a day for each network they hold. That is the same reach
  they have over login's limits, and a cap across every network would let one flood silence everyone's records.
- **A request with a live session's cookie but not its form token is a failed authorization** (`v5.0.0-16.3.2`),
  recorded as `form_token`; the other checks are attempts past a control (`v5.0.0-16.3.3`).
- **A request a rate limit refuses is not recorded:** each would add one record for every request of the flood the
  limit exists to stop, so it stays in the request log, as [ADR-0072](0072-rate-limits-by-network-address.md)
  decides, and the limit's own row keeps the network's state.
- **A request that passes the checks but whose session has ended** is not recorded: a session timing out is
  ordinary, and the checks it passed are the controls. Malformed input from the realm's own origin, such as a body
  that is not JSON, is refused unrecorded, as input validation.
- **Adding the kinds widens the limits table's check,** the one migration this repository lets drop a constraint,
  by the maintainer's choice: the table holds only rate-limit counters that Identity's own functions read and
  write.

## Alternatives considered

- **Log refusals instead of recording them.** Logs cost nothing to keep and expire after 14 days, but ADR-0045
  keeps security events in the audit record, which may not be lost.
- **No limit.** Every refusal recorded, but anyone could grow the audit record at will.
- **A cap across every network.** It bounds the record outright, but one flood would stop everyone's refusals
  being recorded.
- **Name the cookie's operator.** It says whose session was used, but the request proved nothing, and the record
  would blame them for it.

## Consequences

- Cross-site attempts against a logged-in operator show in the audit record, and so does a flood large enough to
  be suppressed.
- Each refused request costs one database statement, two past a network's limit.
- The audit record grows by at most about 1,000 records a day for each network, until
  [#22](https://github.com/heavymedl-labs/tetsu/issues/22) decides how long it keeps them.

## Sources

- OWASP, [Application Security Verification Standard 5.0](https://github.com/OWASP/ASVS/tree/v5.0.0), V16 Security
  Logging and Error Handling.
- [ADR-0072](0072-rate-limits-by-network-address.md)'s limits, which these are counted as.
