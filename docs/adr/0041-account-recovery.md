# 0041. Account recovery

- Status: Proposed
- Date: 2026-11-11
- Supersedes three rules in [ADR-0040](0040-identity-and-sign-in.md): that staff grant an account the right to
  act for a client profile at once, and merge a walk-in's profile at once, which now follow the relink below; and
  that an email belongs to one account, which an account being retired now shares with its replacement

## Context

[ADR-0040](0040-identity-and-sign-in.md) makes a passkey the only credential, with no passwords, and moves
recovery here ([#105](https://github.com/heavymedl-labs/tetsu/issues/105)), with any relaxation of
`v5.0.0-6.3.3` it needs. A new device is not recovery: the person signs in with a passkey they still hold, such as
on their phone, and adds one. Recovery is for someone who has lost every passkey, which synced passkeys make rare.

Three doubt-driven reviews of earlier drafts found that rescuing an account an attacker may already hold cannot be
made safe: whatever channel the rescue uses, the attacker holds it too. They also found that staff could link any
client profile to any account at once, which bypassed every control. Yet a customer account holds little:
bookings, history, waivers, and family live on client profiles, which belong to the business. So recovery without
a code does not restore an account. The person makes a new one, and the business relinks their client profiles to
it.

`v5.0.0-6.4.3` asks that a reset not bypass multi-factor, `v5.0.0-6.4.4` for identity proof at the level of
enrollment when a factor is lost, and `v5.0.0-6.3.3` for multi-factor on every account, which
[ADR-0029](0029-security-target-revised.md) lets an ADR relax for an account type with a documented rationale and
controls. NIST SP 800-63B-4 accepts recovery codes for accounts that were never identity-proofed, as no Tetsu
account is, and requires a notice of every recovery.

## Decision

**A recovery code with an email code restores an account. Without a code, an account is never recovered: the
person makes a new account, and the business relinks their client profiles to it after a wait and notices.**

### Recovery codes

- **Staff must save recovery codes** when they join, and confirm they have them. **Customers are offered them**
  in their account settings, never during sign-up or booking, beside a nudge to add a second passkey.
- **An account holds ten,** each 23 characters from a 32-character alphabet without look-alike letters, so at least
  112 bits from a secure random source. Each is shown once, used once, and stored with a standard hash, which 112
  bits allows (`v5.0.0-6.5.1` to `v5.0.0-6.5.3`). Making or replacing them, like adding a notification address,
  needs a fresh passkey sign-in (`v5.0.0-7.5.1`) and sends a notice.
- **A code with an email code** lets the person enroll a new passkey on the same account. Then every old passkey
  is removed, every session ends (`v5.0.0-7.4.3`), the set of codes is replaced, and linked identities are unlinked
  until the person links them again. Operators cannot use codes.
- **This is a relaxation of `v5.0.0-6.3.3`** for customer and staff accounts: a code and an email code are, to
  ASVS, one factor, though NIST accepts them. The rationale is that a person who kept their codes should get back
  in at once. The controls are the 112-bit code, the email, the notices and their dispute link, and, for staff, a
  72-hour lock on exporting customer data, changing roles, and inviting anyone.

### Relinking client profiles

**Relinking** gives an account the right to act for a client profile that has history or another account: a
person's own profile after they lost an account, a minor's, or a walk-in's. It is the only way to grant that
right, for recovery and everything else, including merging a walk-in's profile. A profile with no history and no
other account, such as a minor's created at a guardian's sign-up or a walk-in created that day, is linked at once,
since it holds nothing to protect. Removing a link is always at once.

- **The new account** may use the email of the account being retired. Signing up with an email that has an
  account offers this path, and creates the new account pending a relink: until the relink completes or is
  cancelled, the email belongs to both, and a cancelled relink deletes the new account.
- **In person:** a staff member, in a role #20 allows, starts it for an account the person signed in to on their
  own device. It completes after 24 hours.
- **Remote:** the person asks from their new account, and it completes 7 days after a staff member approves it.
- **While it waits,** notices go to every address of the account the profile is linked to now, if any, and to the
  business, at once and never batched. Each carries a "this was not me" link, which pauses the relink until staff
  confirm again; a pause ends only when staff confirm or cancel it.
- **When it completes,** the profile is linked, and any account it was linked to for the same person is retired:
  disabled and its sessions ended (`v5.0.0-7.4.2`).
- **For 72 hours after an in-person relink, and 7 days after a remote one,** the account can view and book the
  profile's sessions, but cannot view its history or waivers, sign a waiver, or relink further profiles. A dispute
  in that time reverts the relink and restores the retired account. When the lock ends, the retired account's
  passkeys are removed and it cannot be restored.
- A relink is an authorization change, checked like any other (`v5.0.0-8.2.2`, `v5.0.0-8.4.1`), and logged with
  who started, approved, paused, cancelled, or reverted it (`v5.0.0-16.3.1`).

### Staff, owners, and operators

- **A staff member without codes** is invited again by each business they work for, each acting for itself, so no
  business learns about another or changes a shared identity. The invite is a single-use link
  (`v5.0.0-6.4.1`) that creates a new identity with the same email, which the old identity shares until it holds no
  staff role and is retired.
- **A sole owner without codes** has no one to invite them. Whether Tetsu may set up a new owner, and on what proof,
  is [#24](https://github.com/heavymedl-labs/tetsu/issues/24)'s decision, with what proves an owner at business
  sign-up ([#23](https://github.com/heavymedl-labs/tetsu/issues/23)). No real sole owner exists before both decide,
  since no real business signs up before the Level 2 review (ADR-0029), and the maintainer's demo businesses
  recover with the maintainer's own codes.
- **An operator who loses both hardware keys** gets a new account from ADR-0040's command, with attested keys, and
  the old one is disabled; the maintainer records who asked and why. The maintainer's own access is recovered
  through infrastructure access, under ADR-0029 and [#27](https://github.com/heavymedl-labs/tetsu/issues/27).

### Every recovery

- **Notices** go to the account's email and its notification address, at once, and name only the business they
  concern. Every notice carries a dispute link. After a code recovery, a dispute tells the business, which can
  relink the person to a new account or remove a staff role.
- **Google and a business's identity provider** never recover an account. A Google sign-in that matches an
  account offers a passkey sign-in, or a new account and a relink.
- **Rate limits,** documented in #20, slow requests and code guesses per requester and account without locking
  anyone out (`v5.0.0-6.1.1`, `v5.0.0-6.3.1`).
- **Everything is logged,** never a code: every code use and failure, relink step, retirement, and re-invite
  (`v5.0.0-16.2.5`, `v5.0.0-16.3.1`).

### Each path's strength

These are all of Tetsu's recovery pathways (`v5.0.0-6.1.3`, `v5.0.0-6.3.4`):

- **Code and email code:** one factor to ASVS, a NIST recovery for an unproofed account; the relaxation above. It
  proves more than enrollment did, which was the email alone or an invite to it (`v5.0.0-6.4.4`), and replaces the
  passkeys rather than bypassing them (`v5.0.0-6.4.3`).
- **Relink, and a staff re-invite:** no factor is recovered. The new account or identity signs in with its own
  passkey, full multi-factor, so nothing bypasses it (`v5.0.0-6.4.3`, `v5.0.0-6.4.4`); what the business decides,
  with a wait, notices, and a dispute, is which profiles or roles it reaches.
- **An operator's new account:** made as at enrollment, by ADR-0040's command with attested keys.

Sign-in's issue, #20, names every other requirement that applies, as ADR-0040 asks.

### An example

Jordan Reyes, a guardian at Anvil Row, loses their phone and never saved codes. Jordan makes a new account with
the same email on a laptop, with a new passkey, and asks at the front desk to be relinked. A day later Jordan's
profile and Ari's are linked to the new account, and the old account is retired. For 72 hours Jordan can book
Ari's sessions but cannot view Ari's history or waiver. Had someone else asked, Jordan's dispute link would have
paused it, or, within the 72 hours, reverted it. A family new to Anvil Row is not slowed: Ari's profile, made at
Jordan's sign-up, is linked at once.

## Alternatives considered

- **Rescuing the old account** in person or by email. Three reviews found that an attacker who holds the account
  holds every channel a rescue uses, so it cannot be made safe.
- **Recovery codes required for everyone.** Adds a step to every customer's sign-up, at the moment they are most
  likely to leave.
- **Recovery codes optional for staff too.** Leaves the people whose loss stops a front desk on the slower paths.
- **Relinking at once,** as ADR-0040 first allowed. A request at the front desk becomes a same-minute view of a
  minor's waiver.
- **Relinking every profile, even a new one.** Makes a family's first visit wait four days for a waiver, to
  protect a profile with nothing in it.

## Consequences

A person who kept their codes is back at once. A customer who did not makes a new account and is relinked the
next day through the business, or in a week remotely; their saved cards and settings start fresh, and their
bookings, history, and waivers come back. Staff are invited again by their businesses. No recovery is silent, a
wrong relink can be reverted for 72 hours or 7 days, and an attacker who took an old account loses it at the
relink. Recovery is Tier 1 ([ADR-0005](0005-testing-and-performance.md)), and #20 builds it, once an email provider
is chosen.

**Trade-offs.** A staff member at several businesses must be invited again by each. Sole owners have no recovery
until #24 and #23 decide. A wrong relink not disputed before its lock ends stands until the business removes it.

**Weakest evidence.** How often people lose every passkey and code, and how often disputes catch a wrong relink,
is unknown until real use. Relinks, pauses, disputes, and reverts are logged so the waits can be tuned with
evidence.

**Revisit** when #24 and #23 decide, or when the logs show the waits are too long or too short.

## Sources

- [Identity and sign-in research](../research/identity.md), and its notes on
  [recovery and incidents](../research/identity/recovery-mfa-incidents.md), read 2026-11-11.
- OWASP, "Application Security Verification Standard 5.0.0", chapters V6, V7, V8, and V16,
  <https://github.com/OWASP/ASVS/tree/v5.0.0_release/5.0/en>.
- NIST, "SP 800-63B-4: Authentication and Authenticator Management", July 2025,
  <https://pages.nist.gov/800-63-4/sp800-63b.html>.
