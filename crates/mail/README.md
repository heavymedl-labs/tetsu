# Mail

The `Mailer` port's adapters ([ADR-0044](../../docs/adr/0044-email-delivery.md),
[ADR-0074](../../docs/adr/0074-operator-email-in-development.md)). Tier 1, since it carries invite links.
`post_office(pool, smtp, allowlist)` returns a `PostOffice`, which the composition root hands to the modules that
send.

- **A message goes once per key.** The post office claims the key in `mail.sends` as sending, then settles it as
  sent, captured, or refused. When the server answers with an error, it did not take the message, so the key is
  given back and the next attempt tries at once. When the outcome is unknown, after a timeout, a failed handshake,
  or a dropped connection, the claim is left to lapse after two minutes, as one a stop left does; while it is held,
  another attempt is told to try later. So a message the server took goes at most twice, which its `Message-ID`,
  made from the key, lets a reader see. Settling or keeping also forgets kept mail past a week and keys past thirty
  days.
- **Only a refused mailbox is final.** A 550, 551, or 553 whose enhanced code is 5.1 is `Refused`. Every other
  failure, such as a refused login or sender, a quota, TLS, an outage, a send past its limit, or the store, is
  `Unavailable` and worth trying again. Each is logged by its reply code and kind, never the address.
- **Only the allowlist gets real email.** With an SMTP server set, a message to an address on the allowlist goes out
  over implicit TLS, never STARTTLS; every other message is kept in `mail.captured` for the admins' mailbox page,
  for a week. Without a server, every message is kept. A message marked never kept, such as an invite with its live
  link, goes only to the allowlist and is otherwise withheld, since every admin can read the mailbox.
- **Addresses are plain** (`tetsu_kernel::Address`): no quoted local part, display name, or line break, so none can
  carry a second recipient or a header. The SMTP adapter builds `lettre`'s address without checking it again,
  which a test keeps safe.
- **A sent message's body is never stored,** since it may hold a live link; only its key, route, and time are.
- **Every table is global** ([ADR-0042](../../docs/adr/0042-tenant-isolation.md)): mail to admins belongs to no
  business. A business's mail gets tables of its own when staff mail arrives
  ([#111](https://github.com/heavymedl-labs/tetsu/issues/111)).
- **The SMTP tests send to Mailpit,** which `scripts/install-tools.sh` installs, over implicit TLS with a
  certificate made for each test and a login it checks.
