# Staff pages and their host

Researched 2026-11-25 for [#111](https://github.com/heavymedl-labs/tetsu/issues/111)'s sub-slice 2d, the `/app` pages,
after the maintainer asked that each slice be checked against industry leaders before it is built. What is settled
is not repeated here: staff routes ([ADR-0084](../../adr/0084-realm-routes.md),
[ADR-0089](../../adr/0089-staff-join-and-login.md)), redeeming an invite
([staff join and login](staff-join-and-login.md#redeeming-an-invite-binds-it-to-one-account)), and one account area
for every realm ([account area](account-area-and-dashboards.md)). This page covers what 2d opens: where staff pages
are hosted while Tetsu has no domain, what sharing a passkey relying party (RP) ID with admins would cost, and how an
invite page treats someone who already has an account. Sources were read through search summaries on 2026-11-25,
and say so where they are a vendor's community rather than its documentation.

## Summary

[ADR-0040](../../adr/0040-identity-and-sign-in.md) gives staff a host of their own, as an RP ID of that host alone
(`v5.0.0-3.5.4`). Tetsu has no domain until a client showcase is close, but a free Render service that sleeps when
idle spends free hours only while used, so each realm can have its own `onrender.com` host now. Sharing one host
instead would mean each login page's passkey picker offers both realms' passkeys, told apart only by their names, and
would make one WebAuthn signal unsafe: telling a device a passkey is unknown would delete another realm's. Invites tie to the
account they name; leaders either refuse the wrong account or, as Auth0 does, spend the invite on it, and users of
both report the confusion.

## A host per realm fits the free hours, since idle services sleep

**Takeaway:**

Without a domain, a host of its own means a free Render service of its own. Free hours are pooled, but a sleeping
service spends none, and the demo sleeps unless visited, so a service for each realm costs only the hours people use.

**Cited Findings:**

- Render grants 750 free instance hours per workspace each month, shared by every free service, and suspends all free
  web services once they are spent; a service spun down by idleness spends none
  ([Render: deploy for free](https://render.com/docs/free)). Services awake all month would need 720 hours each, but
  the demo sleeps after 15 minutes without traffic, and nothing keeps it awake
  ([demo environment](../../operations/production.md)).
- A passkey made on `tetsu-demo.onrender.com` can use only that host as its RP ID, since `onrender.com` is a public
  suffix, so demo passkeys are disposable whatever the layout ([passkey domains](../passkey-domains.md)).

## One RP ID for two realms: the picker shows both, and only names tell them apart

**Takeaway:**

With discoverable credentials, the browser offers every passkey for the RP ID, so a staff login page also offers an
admin's passkey, and the reverse. The passkey's name is what a person tells them apart by. Of the WebAuthn signals,
one that names a user is safe across realms; one that names only a credential is not.

**Cited Findings:**

- A login that leaves out `allowCredentials` lets the browser list every discoverable passkey for the RP ID, each by
  the `user.name` set when it was made ([web.dev: discoverable credentials](https://web.dev/articles/webauthn-discoverable-credentials)).
- `user.name` exists to tell apart accounts whose display names look alike; passkeys made with the same names were
  indistinguishable in the picker in a W3C thread
  ([W3C public-webauthn, 2023-08](https://lists.w3.org/Archives/Public/public-webauthn/2023Aug/0131.html)).
- `signalUnknownCredential` tells the passkey provider an RP does not know a credential ID, and the provider should
  remove that passkey; MDN suggests calling it whenever a discoverable login fails
  ([MDN](https://developer.mozilla.org/en-US/docs/Web/API/PublicKeyCredential/signalUnknownCredential_static);
  [Chrome: Signal API](https://developer.chrome.com/docs/identity/webauthn-signal-api)). On a shared RP ID, the staff
  login does not know an admin's passkey, so this signal would delete it. No source discusses shared RP IDs; this is
  inference from the API's scope, an RP ID and a credential ID.
- `signalAllAcceptedCredentials`, which Tetsu's passkey script sends from the Security page, names one user handle and
  that user's passkeys (same sources). Each realm's user handles are random UUIDs, so one realm's signal never names
  another's passkeys.

## An invite ties to the account it names

**Takeaway:**

Leaders tie an emailed invite to one account. An existing account logs in and accepts; a page opened while logged
in as someone else either refuses, or, at Auth0, spends the invite on nobody. Tetsu's SQL already refuses and leaves
the invite open, so the page's job is to say which account it is for and how to switch.

**Cited Findings:**

- Shopify's invite email has an "Accept invite" button, after which a person makes a Shopify account or logs in to one;
  a pending staff member reaches nothing until they accept ([Shopify Help: inviting users](https://help.shopify.com/manual/your-account/users/invite-users)).
  Its community reports an existing account sent to a login screen rather than the page they expected
  ([Shopify Community](https://community.shopify.com/t/why-cant-my-colleague-create-a-limited-access-staff-account/284914)).
- GitHub prompts a person to switch accounts when a page needs one they are not using
  ([GitHub changelog, 2023-11-03](https://github.blog/changelog/2023-11-03-multi-account-support-on-github-com/)), and
  its docs do not explain which account an invite is for ([github/docs#20191](https://github.com/github/docs/issues/20191)).
- An invite opened while logged in as another Auth0 account is spent without adding anyone, and the fix is a new
  invite ([Auth0 Support](https://support.auth0.com/center/s/article/auth0-organization-invitation-disappears-without-being-accepted));
  its community asks for a way to refuse instead
  ([Auth0 Community](https://community.auth0.com/t/prevent-users-from-accepting-an-organization-invite-when-the-current-session-is-incompatible/132837)).
- Bitbucket users note how easy it is to press "Accept" without seeing which account is active
  ([Bitbucket issue](https://bitbucket.org/site/master/issues/5523/verify-identity-before-automatically)), and Discourse
  users asked that a link not accept for whoever is logged in
  ([Discourse Meta](https://meta.discourse.org/t/invite-link-should-not-auto-accept-invitation-if-user-is-already-logged-in/39529)).
- Linear made accepting work for someone logged in to another account in 2021; its current behavior was not found
  ([Linear changelog, 2021-09-03](https://linear.app/changelog/2021-09-03-easier-invites-and-webhook-improvements)).

## Logging in to accept carries the invite through the login

A third pass, on 2026-11-26, asked how leaders take someone with an account from an invite link through login to
accepting, while the invite's token survives the login.

**Takeaway:**

Identity platforms carry the invite's token into the login itself: the link tells the app whether to sign the person
in or up, and signing in with the token accepts the invite in the same step. The account must be the invited one.
Where the token reached a login page that did not carry it, existing users were sent to sign up instead, the
commonest complaint read. Tokens kept out of query strings, or cleared from the address at once, leak least.

**Cited Findings:**

- Clerk's invite link carries its ticket and a status, `sign_in` or `sign_up`; for `sign_in`, the app signs the
  person in with the ticket, which accepts the invite, and the ticket also verifies their email
  ([Clerk: accept organization invitations](https://clerk.com/docs/guides/development/custom-flows/organizations/accept-organization-invitations.md)).
- WorkOS invites suit new and existing users alike: an existing user signs in from the emailed link and becomes a
  member; its sign-in methods take the invitation token, accepting while signing in, and an app that accepts for a
  signed-in user first checks the invite is that user's ([WorkOS: invitations](https://workos.com/docs/authkit/invitations)).
- Auth0's invite link carries the invitation and organization to its login; its community's recurring report is
  existing users sent to sign up instead of log in, and "user already exists" errors
  ([Auth0 Community](https://community.auth0.com/t/can-we-redirect-to-u-login-invitation-when-the-existing-user-clicks-on-the-invitation-link/114307);
  [Auth0 Community](https://community.auth0.com/t/invite-flow-error-user-already-exists-one-connection-one-application-multiple-organisations/94365)).
- GitHub accepts an emailed invite only for an account holding that verified address, and asks a signed-out invitee
  to authenticate before joining ([GitHub Docs](https://docs.github.com/en/organizations/managing-membership-in-your-organization/inviting-users-to-join-your-organization);
  [University of Illinois guide](https://answers.uillinois.edu/102588)).
- A token in a query string can leak through `Referer` and logs; a fragment avoids the `Referer` but stays in
  history and in reach of scripts, so a page clears it and spends it once
  ([OWASP: query strings](https://owasp.org/www-community/vulnerabilities/Information_exposure_through_query_strings_in_url);
  [thoughtbot](https://thoughtbot.com/blog/is-your-site-leaking-password-reset-links)).

## Recommendations for sub-slice 2d

- **Each realm on a host of its own before 2d builds staff pages,** as ADR-0040 intends: one image deployed as a free
  service per realm, each its own RP ID, so no picker mixes realms and no signal reaches another realm's passkeys.
  The maintainer chose this over sharing a host for now, since shared-host code would be thrown away. A domain
  (#102) later changes the hosts' names, not their layout.
- **Passkey names keep leading with the realm,** as they do: "Tetsu staff · Name" and "Tetsu admin · Name".
- **One join page, `/app/join`, for both kinds of invitee,** after Clerk's and WorkOS's sign-in with the invite. The
  page reads the token from the fragment, clears it from the address, and asks what the invite gives. Someone new
  registers a passkey. Someone with one sees the business and role and one button, "Log in to accept", whose passkey
  login is followed at once by accepting, with the token kept in the page's memory; someone already logged in as the
  invitee sees "Accept". The person's own press starts either, never the link alone.
- **Logged in as someone else, the page says so and refuses,** naming the account it is for by name only, and offering
  to log out and open the link again. The invite stays open, as the SQL already keeps it.
- **One router for both realms,** with the paths sub-slice 1 set out, each served only on its realm's host. The mailbox stays an admin page, and each realm's passkey
  cap is its own: two for admins and ten for staff, as noted on #111.

## Gaps

- Pages were read through search summaries, not opened. Shopify's and GitHub's accept pages were not seen as written.
- No source tested a shared RP ID with two account types, or `signalUnknownCredential` across them; that finding is
  inference from the API. It matters only if realms ever share a host.
- No booking competitor's staff invite or accept page was found.
