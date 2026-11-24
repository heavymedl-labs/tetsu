# Where each realm's login starts

Researched 2026-11-24 for [#111](https://github.com/heavymedl-labs/tetsu/issues/111), after the maintainer asked
whether Tetsu's home page should carry a "Log in" or "Sign up" button at the top right, whether every realm could
share one login flow, and noted that an admin has to remember `/admin/login` to log back in. The public sites below
were read on 2026-11-24. How each platform separates its accounts is in
[commerce, payments, and booking realms](commerce-booking-realms.md), and Related Origin Requests in
[RP ID mechanics](../passkey-domains/rpid-mechanics.md#related-origin-requests-well-knownwebauthn).

## Summary

Every business-facing booking and payments site read puts a login link in its header, beside a sign-up or demo
button, and the link leads to the business's own dashboard, usually on a separate host. Only Fresha shows a chooser,
"Fresha for customers" or "Fresha for professionals", and it can because its customers hold one marketplace account;
Tetsu's customers hold one account per business ([ADR-0040](../../adr/0040-identity-and-sign-in.md)), so a chooser
would have to ask which business. Customers are sent to a booking page or an app, never to the business login.
Sharing one login page across realms would also undo ADR-0040's passkey domains: a page can use only one RP ID's
passkeys per ceremony, and reaching another's needs Related Origin Requests, which would let customer pages ask for
staff passkeys. Sharing the template, script, and copy has none of these costs. A platform's own back office is
reached by its address, which sends a logged-out visitor to log in and back, as WordPress's `/wp-admin` does and
Tetsu's `/admin` already does; OWASP treats a hidden admin address as something to test for, not a protection.

## The header's login link goes to the business's dashboard

**Takeaway:**

The pattern is uniform: "Log in" or "Sign in" in the header, next to "Sign up", "Start for free", or "Get a demo",
leading to a dashboard host. Mindbody alone labels it for staff, "Staff sign in". Labels and targets often disagree,
as Fresha's "Sign up" leading to a sign-in page.

**Cited Findings:**

Each site's header, read on 2026-11-24:

| Site | Login link | Beside it | Where login leads |
| --- | --- | --- | --- |
| [Fresha for business](https://www.fresha.com/for-business) | "Sign up" | | `partners.fresha.com/users/sign-in` |
| [Vagaro Pro](https://www.vagaro.com/pro) | "Log In" | "Start Free Trial" | `vagaro.com/pro-login` |
| [Mindbody business](https://www.mindbodyonline.com/business) | "Staff sign in" (menu and footer) | "Get a Demo" | `clients.mindbodyonline.com/launch` |
| [Square Appointments](https://squareup.com/us/en/appointments) | "Log in" | "Sign up" | `app.squareup.com/login` |
| [Boulevard](https://www.joinblvd.com/) | "Login" (header and footer) | "Get a Demo" | `dashboard.boulevard.io/login` |
| [Glofox](https://www.glofox.com/) | "Customer login" | "Get Free Demo" | `app.glofox.com/dashboard/` |
| [Jane](https://jane.app/) | "Sign in" | "Sign up", "Book a demo" | `jane.app/sign-in` |
| [Shopify](https://www.shopify.com/) | "Log in" | "Start for free" | `shopify.com/login` |
| [Stripe](https://stripe.com/) | "Sign in" (header and footer) | "Contact sales", "Start now" | `dashboard.stripe.com/login` |

Glofox's "Customer login" means Glofox's customers, the businesses, which shows how the word "customer" misleads on a
platform with two kinds.

## Customers start from a business, not from the platform's login

**Takeaway:**

No business-facing site links its customers to a web login. They are sent to book: Mindbody's "Not a business? Book
on the Mindbody app", Vagaro's "Book a Service", and app badges. Fresha's general homepage is the one exception, and
its chooser serves a marketplace.

**Cited Findings:**

- [Fresha's homepage](https://www.fresha.com/) header reads "Log in or sign up" and leads to
  [a chooser](https://www.fresha.com/user-flow): "Fresha for customers, Book salons and spas near you" goes to
  `fresha.com/auth`, and "Fresha for professionals, Manage and grow your business" to `partners.fresha.com`.
- [Mindbody](https://www.mindbodyonline.com/business) shows "Not a business? Book on the Mindbody app", leading to
  its consumer marketplace.
- [Vagaro Pro](https://www.vagaro.com/pro) shows "Book a Service", leading to `vagaro.com`, and customer app badges in
  its footer.
- Customer accounts belong to one business in Tetsu, "after Mindbody's shared login drew a backlash"
  ([ADR-0040](../../adr/0040-identity-and-sign-in.md)).

## One login page across realms costs what ADR-0040 decided against

**Takeaway:**

Sharing the page that asks for a passkey is not neutral. Each WebAuthn ceremony names one RP ID, so one page for
every realm works only while they share a domain, as ADR-0061 has them until Tetsu has domains. After that, a shared
page needs Related Origin Requests: the staff RP ID would list the customer domain as a related origin, and every
customer page could then ask for staff passkeys, which ADR-0040 forbids (`v5.0.0-3.5.4`). An email-first page that
routes by realm also tells whoever types an address which realm holds it, which OWASP asks login not to reveal.
Passkey login has no identifier step at all, so it has nothing to reveal.

**Cited Findings:**

- "Such Relying Parties MUST choose a common RP ID to use across all ceremonies from related origins," from WebAuthn
  Level 3 §5.11, as quoted in [RP ID mechanics](../passkey-domains/rpid-mechanics.md#related-origin-requests-well-knownwebauthn).
- [web.dev, Related Origin Requests](https://web.dev/articles/webauthn-related-origin-requests): browsers that
  support them fetch `/.well-known/webauthn` from the RP ID and allow only the origins it lists. Chrome and Safari
  support them, and Firefox was still considering them in January 2026; a vendor blog's claim of Firefox 152 support
  was not confirmed against Mozilla.
- [OWASP Authentication Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html):
  login, reset, and recovery answer with a generic message whether or not the account exists. The
  [Web Security Testing Guide](https://owasp.org/www-project-web-security-testing-guide/stable/4-Web_Application_Security_Testing/03-Identity_Management_Testing/04-Testing_for_Account_Enumeration_and_Guessable_User_Account.html)
  tests for it.

## A back office is reached by its address

**Takeaway:**

Platforms do not link their own staff's back office from the public site. People reach it by its address, which
sends a logged-out visitor to log in and back to where they were going. Tetsu's `/admin` already does this, so
remembering `/admin`, or bookmarking it, is enough. An address no one links is not a protection, and Tetsu does not
treat it as one: every admin page requires a session.

**Cited Findings:**

- [WordPress: Administration Screens](https://wordpress.org/documentation/article/administration-screens/) live
  under `/wp-admin`. Its pages call
  [`auth_redirect()`](https://developer.wordpress.org/reference/functions/auth_redirect/), which sends a visitor who
  is not logged in to the login page, and back to the page they asked for once they are.
- [OWASP WSTG: Enumerate Infrastructure and Application Admin Interfaces](https://owasp.org/www-project-web-security-testing-guide/latest/4-Web_Application_Security_Testing/02-Configuration_and_Deployment_Management_Testing/05-Enumerate_Infrastructure_and_Application_Admin_Interfaces)
  treats finding unlinked admin interfaces as a test step, so an unlinked address protects nothing by itself.

## Recommendations for Tetsu

- **"Log in" in Tetsu's site header, for staff,** leading to `/app/login`
  ([ADR-0084](../../adr/0084-realm-routes.md)), beside "See plans". It comes with #111, when staff can log in.
- **No "Sign up" yet.** Sign-ups stay off until the Level 2 review (ADR-0040), and what a business signs up for is
  [#23](https://github.com/heavymedl-labs/tetsu/issues/23)'s decision. "See plans" holds the place until then.
- **No chooser.** Customers log in on their business's page, branded with it, and start from booking. A chooser
  earns a place only if customers gain an account across businesses
  ([#117](https://github.com/heavymedl-labs/tetsu/issues/117)).
- **One login template and script for every realm, with an entry point per realm.** The passkey ceremony, copy,
  errors, session dialog, and account area are shared; the page's realm, its RP ID, and its branding are not.
- **Admins keep `/admin` as their address,** with no public link. Bookmarking it is the remedy for remembering it.

## Gaps

- The login pages behind the header links (Vagaro's `/pro-login`, Shopify's `/login`, Boulevard's, Square's, and
  Glofox's dashboard) were not opened, so whether any of them is itself a chooser is unconfirmed.
- Linear's and Square's pages were read only in part, and Shopify's storefront customer login was not covered.
- No source measured whether a header login link changes how staff find their login; the pattern's uniformity is the
  evidence.
