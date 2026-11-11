# 0041. Account recovery

- Status: Proposed
- Date: 2026-11-11
- Supersedes rules in [ADR-0040](0040-identity-and-sign-in.md): that an account acts for another client profile
  when staff grant it, and that staff merge a walk-in's profile, both of which now follow the relink below; that
  staff remove such a link, which the account may now do too; and that adding or removing a passkey needs a fresh
  passkey sign-in, which a recovery does without

## Context

[ADR-0040](0040-identity-and-sign-in.md) makes a passkey the only credential, with no passwords, and moves
recovery here ([#105](https://github.com/heavymedl-labs/tetsu/issues/105)), with any relaxation of
`v5.0.0-6.3.3` it needs. A new device is not recovery: the person signs in with a passkey they still hold, such as
on their phone, and adds one. Recovery is for someone who has lost every passkey, which synced passkeys make rare.

The [identity research](../research/identity.md) found two postures. Commerce and booking platforms such as
Shopify, Toast, and Mindbody treat the inbox as the account: whoever holds it holds the account. Apple and Google
offer recovery codes and trusted contacts, then a delayed recovery by email that the real owner cancels just by
using a device still signed in. They accept that an attacker who holds the inbox, and is not stopped, can win
after the wait. Tetsu requires multi-factor for every account ([ADR-0029](0029-security-target-revised.md)), which
is stricter than the booking platforms, so it takes Apple's posture, with that risk recorded as a relaxation.

Three doubt-driven reviews also found that no remote channel can rescue an account an attacker already holds,
since the attacker holds that channel too, and that staff could link any client profile to any account at once.
A customer account holds little: bookings, history, waivers, and family live on client profiles, which belong to
the business. So a hijacked account is not rescued: the business retires it in person and relinks the person's
profiles to a new one.

`v5.0.0-6.4.4` asks for identity proof at the level of enrollment when a factor is lost. `v5.0.0-6.4.3` covers
resetting a password, which Tetsu has none of, as ADR-0040 says. NIST SP 800-63B-4 accepts saved and issued
recovery codes for accounts that were never identity-proofed, as no Tetsu account is, and requires a notice of
every recovery.

## Decision

**A recovery code restores an account at once. A customer without one recovers the same account by email after
a week, unless they sign in or the business stops it. A hijacked account is retired in person, and the business
relinks the person's client profiles to a new one.**

### Recovery codes

- **Staff and owners must save recovery codes** when their account is created, by invite, business sign-up, or the
  seed command, and confirm they have them. Owners must also confirm a notification address other than their
  account's email, so a recovery of an owner is never told only to the inbox it may be using. **Customers are offered them** in their account settings, never during
  sign-up or booking, beside a nudge to add a second passkey.
- **An account holds ten,** each 23 characters from a 32-character alphabet without look-alike letters, so at least
  112 bits from a secure random source. Each is shown once, used once, and stored with a standard hash, which 112
  bits allows (`v5.0.0-6.5.1` to `v5.0.0-6.5.3`). Making or replacing them, like adding a notification address,
  needs a fresh passkey sign-in (`v5.0.0-7.5.1`) and sends a notice.
- **A code with an email code** lets the person enroll a new passkey on the same account. Operators cannot use
  codes.

### Recovering by email

A customer without a code can ask to recover by email. It is for customers only.

- **The request waits 7 days.** Use of the account by its real owner cancels it, as Apple cancels when an account
  is in use on a signed-in device: at least a passkey sign-in, and what else counts is for #20, without letting the
  requester's own session cancel their own request. The business is told and can cancel it too.
- **Requests are rate limited** per account and per requester, answer the same whether or not the email has an
  account, and one request is pending at a time (`v5.0.0-6.1.1`, `v5.0.0-6.3.1`).
- **When the wait ends,** a link goes to the account's email, single use, valid for 24 hours, and able only to enroll
  a new passkey on that account (`v5.0.0-6.4.1`).
- **An attacker who already holds the account** cancels these requests by signing in. The real owner then recovers
  in person, below.

### What every recovery of an account does

For a code or email recovery: every old passkey is removed, every session ends (`v5.0.0-7.4.3`), any codes are
replaced, and linked identities are unlinked until the person links them again. Notices go to the account's email,
its notification address, and the business, or for staff to every owner of each business they work for, other
than themselves, when it starts and when it ends, at once and never batched. Each names only the business it concerns and carries a "this was not me"
link, which cancels a pending email recovery.

Then the account is locked: 72 hours after a code recovery, 7 days after an email one. It can book and view its
own profile's sessions, but cannot pay with a saved card, view another profile's bookings, history, or waivers,
sign a waiver, or change its email, notification address, passkeys, codes, or linked identities. A staff member
also cannot export customer data, open customers' waivers or history, retire an account, start or confirm a relink,
change a role, or invite anyone.

### Relinking client profiles

**Relinking** gives an account the right to act for a client profile that has history or another account. It is
the only way to grant that right, including adding a second guardian and merging a walk-in's profile. A profile
an account creates itself, such as a child's added at a guardian's sign-up, is linked at once.

- **Every relink is started at the business** by a staff member in a role #20 allows (`v5.0.0-8.2.1`), after
  checking the person face to face. For a hijacked account, the person signs up for a new account on their own
  device, with an email other than the old account's, and the old account is retired when the relink completes:
  disabled, its sessions ended, its passkeys removed, and its email freed, when it becomes a **retired account**.
- **It completes after 24 hours.** Notices go at once to every account already linked to the profile, their emails
  and notification addresses, and the business, each with a "this was not me" link.
- **A dispute pauses it, and a different, senior staff member settles it,** an owner or a role #20 names, never
  the one who started it. They hear both sides and decide whether it goes ahead.
- **Removing another account's link follows the same path,** with its notices and dispute, so no one can unlink the
  real guardian and relink silently. An account may still unlink itself at once.
- **After it completes,** for 72 hours the account can view and book its own profile's sessions, and book the
  relinked profiles' sessions, but cannot view their bookings, history, or waivers, sign a waiver, or relink further
  profiles.
- A relink is an authorization change, checked like any other (`v5.0.0-8.2.1`, `v5.0.0-8.2.2`, `v5.0.0-8.4.1`), and
  logged with who started, disputed, settled, or cancelled it (`v5.0.0-16.3.1`).

The proof is the weak point. A photo ID shows that someone carries the name on a profile, not that they are the
person who enrolled; what actually guards a relink is the business's judgment, the wait, the notices, and a second
staff member settling any dispute.

### Staff, owners, and operators

- **A staff member without codes** is removed by each business they work for, which tells the old identity, and
  invited again with a single-use link (`v5.0.0-6.4.1`). Once the old identity holds no staff role it is retired;
  until then, an invite uses another email. The new identity has the staff lock above for 72 hours.
- **A sole owner without codes** has no one to invite them. Whether Tetsu may set up a new owner, and on what proof,
  is [#24](https://github.com/heavymedl-labs/tetsu/issues/24)'s decision, with what proves an owner at business
  sign-up ([#23](https://github.com/heavymedl-labs/tetsu/issues/23)). No real business may sign up until #24 has
  decided; #23 and the Level 2 review ([#83](https://github.com/heavymedl-labs/tetsu/issues/83)) carry that gate.
  The maintainer's demo businesses recover with the maintainer's own codes.
- **An operator who loses both hardware keys** gets a new account from ADR-0040's command, with attested keys, and
  the old one is retired; the maintainer records who asked and why. The maintainer's own access is recovered
  through infrastructure access, under ADR-0029 and [#27](https://github.com/heavymedl-labs/tetsu/issues/27).
- **Google and a business's identity provider** never recover an account.

Everything is logged, never a code or a link: every request, code use and failure, cancellation, retirement,
relink step, and rate-limit hit (`v5.0.0-16.2.5`, `v5.0.0-16.3.1`).

### Each path's strength

These are all of Tetsu's recovery pathways (`v5.0.0-6.1.3`, `v5.0.0-6.3.4`):

- **A code with an email code:** to ASVS one factor, and to NIST two recovery codes for an unproofed account. It
  proves more than enrollment did, which was the email or an invite to it (`v5.0.0-6.4.4`).
- **By email:** the email alone, as a customer enrolled (`v5.0.0-6.4.4`), and an issued code to NIST.
- **A relink:** no factor is recovered, since the new account signs in with its own passkey, but it hands over what
  the old account was worth. Its proof, a face-to-face check by the business, is not identity proofing
  (`v5.0.0-6.4.4`); the wait, the notices to every linked account, the dispute settled by a second staff member, and
  the lock bound it.
- **A staff re-invite:** the business's own invite, as at enrollment, under the staff lock.
- **An operator's new account:** made as at enrollment, by ADR-0040's command with attested keys.

The code and email paths are a relaxation of `v5.0.0-6.3.3`, for customer and staff accounts and for customer
accounts respectively. The rationale is that losing every passkey should not end an account for good. The controls
are the code's strength, the 7-day wait, cancellation by any sign-in or the business, the notices and their dispute
link, the locks, and the log. Sign-in's issue, #20, names every other requirement that applies, as ADR-0040 asks.

### What #20's design must answer

Reviews of this ADR found details that only an implementation and its tests can settle. #20's design answers
each, and its fresh review checks them:

- What counts as use of an account that cancels an email recovery, without the requester's own session cancelling
  it, and without letting an attacker's use of a hijacked account block the real person.
- Whether a dispute after a settlement pauses a relink again, whether the wait restarts, and what the old account
  can still see while a relink is paused, including whether staff may suspend it meanwhile.
- What happens to bookings made on a new account before its own empty profile is removed.
- What each lock shows, so a person always sees their own bookings.
- Which roles may start, dispute, and settle a relink, and retire an account (`v5.0.0-8.2.1`).
- Rate limits for requests, codes, and notices, documented as `v5.0.0-6.1.1` asks.

### An example

Jordan Reyes, a guardian at Anvil Row, loses their phone and never saved codes. Jordan asks to recover by email
from a laptop. Seven days later the link arrives, Jordan enrolls a passkey, and for a week Jordan can book Ari's
sessions but not open Ari's waiver. Had an attacker asked instead, Jordan's next sign-in would have cancelled it.
Had an attacker taken the account outright, Jordan would show ID at the front desk and sign up again, and a day
later Jordan's and Ari's profiles would relink to the new account and the old one would be retired. The attacker's
dispute would pause it until a second staff member, hearing both sides, settled it.

## Alternatives considered

- **The inbox as the account,** as Shopify, Toast, and Mindbody do. Needs multi-factor relaxed for customers for
  good, which ADR-0040 rejected because a guardian's account holds a minor's waiver.
- **In person only,** as a bank does. Strongest, but a customer who cannot visit has no way back.
- **Rescuing a hijacked account remotely.** Three reviews found the attacker holds every channel a rescue uses.
- **Recovery codes required for everyone.** Adds a step to every customer's sign-up.
- **Relinking at once.** A request at the front desk becomes a same-minute view of a minor's waiver.

## Consequences

A person who kept their codes is back at once. A customer without them is back in a week, with their history and
cards, or the next day through the business if their account was taken. Staff are invited again by their
businesses. No recovery is silent. Recovery is Tier 1 ([ADR-0005](0005-testing-and-performance.md)), and #20 builds
it, once an email provider is chosen.

**Trade-offs.** An attacker who holds a customer's inbox, while the customer does not sign in for a week and the
business does not act, takes the account, as Apple's model accepts; the lock keeps its waivers and saved cards out
of reach for a further week. A staff member at several businesses must be invited again by each. Real businesses
wait on #24 before they can sign up.

**Weakest evidence.** How often people lose every passkey, and how often a sign-in or a business stops a false
recovery, is unknown until real use. Every request, cancellation, and relink is logged, so the waits can be tuned.

**Revisit** when #24 and #23 decide, or when the logs show the waits are too long or too short.

## Sources

- [Identity and sign-in research](../research/identity.md), and its notes on
  [recovery and incidents](../research/identity/recovery-mfa-incidents.md) and
  [commerce and booking platforms](../research/identity/commerce-booking-realms.md), read 2026-11-11.
- OWASP, "Application Security Verification Standard 5.0.0", chapters V6, V7, V8, and V16,
  <https://github.com/OWASP/ASVS/tree/v5.0.0_release/5.0/en>.
- NIST, "SP 800-63B-4: Authentication and Authenticator Management", July 2025,
  <https://pages.nist.gov/800-63-4/sp800-63b.html>.
