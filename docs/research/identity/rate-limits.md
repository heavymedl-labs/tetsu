# Rate limits for operator login and joining

Research on 2026-11-17 for [#110](https://github.com/heavymedl-labs/tetsu/issues/110)'s last open criterion, rate
limits for failed logins. [ADR-0048](../../adr/0048-sign-in-design.md) set the limit, 10 failed passkey logins per
network address per 15 minutes and then a growing delay, and that Tetsu slows down rather than locks out. This note
checks that against current practice, adds what the table left out, and settles how Tetsu finds a network address,
which nothing in Tetsu reads yet.

## Summary

1. **A passkey cannot be guessed,** so its limits guard the server and the ceremony table, not a secret. NIST's
   cap of 100 failures per authenticator, and OWASP's per-account counters, protect passwords and codes. A failed
   passkey login is a bad signature or an unknown credential; an attacker learns nothing from it.
2. **Limit by network address, never by account.** A passkey login names no account until it finishes, and a
   per-account limit on a passkey would let a stranger who knows a credential's ID slow its owner down. Both
   OWASP and ASVS 6.1.1 ask that limits not lock out a real user.
3. **Limit begins too, not only failures,** as the [sign-in review](passkey-sign-in-review.md) (item 13) and the
   [enrollment review](passkey-enrollment-review.md) (item 14) asked. A begin costs a database write and needs no
   answer, so it is the cheaper flood. ADR-0048's table lists neither.
4. **Find the address from the rightmost forwarded entry Tetsu's own proxies did not add,** never the leftmost.
   Render documents only that the address is in `X-Forwarded-For` and that Cloudflare sits in front; it does not say
   whether a client's own header is replaced or appended to. One measurement on the demo settles how many entries
   Render's hops add.
5. **Key IPv6 on its /64,** since one home or phone holds a whole /64 and can rotate within it.
6. **Answer 429 with `Retry-After`,** and say on the page how long to wait.
7. **Keep the counters in Postgres,** as ADR-0047 says, holding a keyed hash of the address rather than the
   address, and deleting rows once their window ends.

## What the standards ask

- **NIST SP 800-63B-4, §3.2.2:** the verifier "SHALL limit consecutive failed authentication attempts" on an
  authenticator "to no more than 100", after which it is disabled. Delays that grow toward the limit, "30 seconds
  up to an hour", are optional, as are bot checks and risk signals such as the address. After a success, it
  "SHOULD disregard any previous failed attempts". Nothing exempts passkeys, but the cap protects against guessing a
  secret; a cryptographic authenticator's own PIN or biometric is limited on the device.
- **OWASP ASVS 5.0:** 6.1.1 (L1) asks the documentation to say how "rate limiting, anti-automation, and adaptive
  response" defend against attacks, and how they "prevent malicious account lockout". 6.3.1 (L1) asks that they be
  built as documented. ADR-0048's table is that documentation.
- **OWASP Credential Stuffing cheat sheet:** blocking addresses "should not be used as the sole or primary
  defense", since proxy networks spread an attack. Use both short and long windows, and "not rely on a single
  predictable volume limit". Blocks "should be temporary".
- **RFC 6585 §4:** 429 Too Many Requests "MAY include a Retry-After header". It leaves how a server identifies the
  requester to the server.
- **Lucia's guide to login throttling:** exponential backoff, doubling from 1 second, answering 429 until it
  passes, keyed on the account with the address as well. It warns that per-account limits let an attacker lock a
  real user out.

Most of this targets passwords and codes. What carries over to passkeys is the shape: temporary, growing delays,
more than one window, and limits that a stranger cannot turn against an owner.

## Finding the network address

Every limit here keys on the requester's address, so a forged one defeats them all.

- **The leftmost `X-Forwarded-For` entry is whatever the client sent.** A proxy that appends keeps it. Only entries
  added by proxies Tetsu trusts can be believed, so the address is the first entry from the right that those
  proxies did not add ([adam-p](https://adam-p.ca/blog/2022/03/x-forwarded-for/)). The same post finds that "a lot
  of rate limiter implementations are using spoofable IPs".
- **Count the trusted hops rather than list their addresses.** Render's and Cloudflare's addresses change and are
  not published for this; the number of hops is fixed by the platform.
- **With no proxy, use the connection's own address.** A header is then anyone's to write, so it is ignored.
  Locally, `cargo tetsu serve` has no proxy.
- **Render's documentation is silent on the details.** Its
  [DDoS article](https://render.com/articles/how-render-handles-ddos-attacks) says all traffic passes through
  Cloudflare, and to read `x-forwarded-for` for the client, but its example takes the leftmost entry without saying
  why that is safe. A 2021 report on Render's feedback board found that Render appends to a client's header rather
  than replacing it, which would make the leftmost entry forgeable. Nothing documents whether Cloudflare's
  `CF-Connecting-IP` reaches the service.

**So Tetsu measures it once.** The demo logs, for one request sent with a made-up `X-Forwarded-For` of
`203.0.113.7` (an address reserved for documentation), how many entries arrive and which one is the made-up one,
without logging any real address. That fixes the hop count, which then becomes a setting with no default, so a host
whose proxies differ has to say so.

**IPv6 keys on its /64.** A home, an office, or a phone network gives each subscriber a /64, and a client can change
address within it at will. Wider prefixes, such as /48, catch more but also group unrelated people
([Better Auth](https://better-auth.com/docs/concepts/rate-limit) uses /64 by default). IPv4 keys on the full
address.

## Algorithms and storage

- **A fixed window counts per address per period** and resets at its edge, letting up to twice the limit through
  across a boundary. **A sliding log** is exact but stores every request. **GCRA** keeps one timestamp per key and
  behaves like a token bucket, and Redis's `redis-cell` and Traefik use it.
- **For a small limit and a few keys, a per-address row in Postgres is enough.** One `INSERT ... ON CONFLICT DO
  UPDATE ... RETURNING` decides atomically, so two requests at once cannot both pass. ADR-0047 already puts Tetsu's
  limits in Postgres, beside the ceremonies they protect, with no new service.
- **The row holds a keyed hash of the address,** not the address, since an address is personal data
  ([ADR-0045](../../adr/0045-operations-baseline.md) keeps it out of logs too). Rows go once their window ends.
- **A delay is a refusal, not a wait.** Holding a connection open for the delay would let a flood tie up the
  server; answering 429 at once costs nothing.

## What Tetsu has today

The `begin_login` function keeps the ceremony table small without refusing anyone: each begin sweeps up to a hundred
ended ceremonies, and past ten thousand open logins it ends the oldest. Its own comment says a flood of begins "can
still end a real ceremony before its person answers, until those rate limits". Joining has a single-use invite of
high entropy, which cannot be guessed, so its limits too guard the server.

## Recommendations

For a Proposed ADR that supersedes ADR-0048's passkey row and adds the rows it lacks:

| What | Per network address | Then |
| --- | --- | --- |
| Login begins | 30 per 15 minutes | Refused until the window ends |
| Failed passkey logins | 10 per 15 minutes, as ADR-0048 sets | A delay from 1 minute, doubling to 1 hour |
| Join begins and finishes | 10 per hour | Refused until the window ends |

- **A success does not clear an address's failures,** since a shared address may hold an attacker and an operator
  at once; the window ends them instead.
- **A refusal answers 429 with `Retry-After`,** and the passkey island says, in words, when to try again. It
  never says which limit was met.
- **A refusal is logged at warn,** with the realm and the limit, never the address.
- **The ceremony table keeps its bounds** as a backstop behind the limits.
- **`TETSU_TRUSTED_PROXIES`, the number of hops whose entries Tetsu trusts,** is required when `TETSU_DEMO` is set
  on a host, after the measurement above sets it for Render.

## Sources

- NIST, SP 800-63B-4, §3.2.2 "Rate Limiting (Throttling)", <https://pages.nist.gov/800-63-4/sp800-63b.html>.
- OWASP, ASVS 5.0, V6 Authentication,
  <https://github.com/OWASP/ASVS/blob/master/5.0/en/0x15-V6-Authentication.md>.
- OWASP, Credential Stuffing Prevention Cheat Sheet,
  <https://cheatsheetseries.owasp.org/cheatsheets/Credential_Stuffing_Prevention_Cheat_Sheet.html>.
- IETF, RFC 6585, §4 "429 Too Many Requests", <https://www.rfc-editor.org/rfc/rfc6585#section-4>.
- Lucia, "Login throttling", <https://v2.lucia-auth.com/guidebook/login-throttling>.
- Adam Pritchard, "The perils of the 'real' client IP", 2022, <https://adam-p.ca/blog/2022/03/x-forwarded-for/>.
- Render, "How Render handles DDoS attacks", <https://render.com/articles/how-render-handles-ddos-attacks>.
- Render feedback, "Send the correct X-Forwarded-For", 2021,
  <https://feedback.render.com/features/p/send-the-correct-xforwardedfor>.
- Better Auth, "Rate limit", <https://better-auth.com/docs/concepts/rate-limit>.
- Tony Finch, "GCRA vs leaky / token buckets", 2026, <https://dotat.at/@/2026-01-15-gcra.html>.
