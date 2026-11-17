# 0072. Rate limits by network address

- Status: Accepted
- Date: 2026-11-17
- Supersedes the failed passkey login row of [ADR-0048](0048-sign-in-design.md)'s rate limits

## Context

[ADR-0048](0048-sign-in-design.md) limits failed passkey logins to 10 per network address per 15 minutes, then a
growing delay, and slows down rather than locks out. It does not limit login begins or joining, which the sign-in
and enrollment reviews asked for, and nothing in Tetsu reads a network address yet.

The [research](../research/identity/rate-limits.md) found that a passkey cannot be guessed, so these limits guard
the server and the ceremony table rather than a secret, and that a limit by account would let a stranger slow its
owner. It measured the demo: Render appends to an `X-Forwarded-For` the client sent, so its leftmost entry is
forgeable, and three proxies always add the entries after the client's own.

## Decision

- **Each limit keys on the requester's network address:** the connection's own address, or with
  `TETSU_TRUSTED_PROXIES` set to N, the Nth `X-Forwarded-For` entry from the right. It is required, `0` locally and
  `3` on Render. A header with fewer entries, or one that does not parse, falls back to the connection's address.
- **An IPv6 address keys on its /64,** and an IPv4 address, mapped or not, on itself.
- **The limits:**

  | What | Per network address | Past it |
  | --- | --- | --- |
  | Login begins | 30 per 15 minutes | Refused until there is room |
  | Failed passkey logins | 10 per 15 minutes | Refused for 1 minute, doubling with each further failure to 1 hour |
  | Join begins and finishes | 10 per hour | Refused until there is room |

- **The numbers live in Identity's schema,** beside the clock that applies them, as session limits do, so no caller
  can pass others. Each limit is one row a network, with the time all its room is free again (GCRA) and, for
  failures, how many times they met the limit, which is forgotten once that time passes.
- **A row keys on a hash of the address with a secret the database made,** so the table holds no address. Rows a
  day past their use are swept.
- **A success does not clear an address's failures,** since one address can hold an attacker and an operator.
- **A refusal answers 429 with `Retry-After`,** and the page says, in words, when to try again. It never says which
  limit was met. The request's log line shows the 429 and its route, which names the limit, and never the address.
- **The ceremony table keeps its bounds** behind the limits.

## Alternatives considered

- **A limit by account,** as for passwords. A passkey login names no account until it ends, and a stranger who knew
  a credential's ID could slow its owner.
- **The leftmost forwarded entry,** as Render's article shows. The measurement shows any client can set it.
- **`CF-Connecting-IP`,** which Cloudflare sets. It ties Tetsu to Render keeping Cloudflare; the count does not.
- **Limits in memory.** They reset on every deploy and are not shared by a second instance.
- **Waiting before answering** rather than refusing. A flood would hold connections open.

## Consequences

- An operator behind an address that floods Tetsu waits too; the delay is at most an hour.
- Render's settings gain `TETSU_TRUSTED_PROXIES=3`, set before deploying, since `serve` refuses to start without it.
  A change in Render's proxies needs a new measurement.
- Login gets slower by one database call before each begin and finish.

## Sources

- [Rate limits for operator login and joining](../research/identity/rate-limits.md), 2026-11-17.
