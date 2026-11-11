# Sign-in and passkeys on multi-tenant platforms with custom domains

Researched 2026-11-11. About 20 search and fetch calls. Vendor docs are primary sources. Blog posts from passkey vendors (MojoAuth, Corbado, Descope) are secondary and partly marketing.

## How real platforms host end-customer sign-in when the tenant has its own domain

### Takeaway

The platforms split into two models. In the first, the platform runs one account shared across tenants and signs users in on its own domain (Shopify's Shop, Substack, Mindbody). In the second, each site has its own members and signs them in on the tenant's domain (Ghost, Wix, Kajabi). Shopify is a hybrid. Its store accounts now sit on a branded `account.<store domain>` subdomain, while its passkeys belong to the platform-wide Shop identity on shop.app.

### Cited Findings

**Shopify (new customer accounts):**

- With no custom domain, customer account pages "use a `shopify.com` URL format as their base domain", with a store ID in the path, for example `shopify.com/1234567890/account`. The default sign-in link looks like `shopify.com/authentication/1234567890/login`. — [Shopify Help: connect a domain for customer accounts](https://help.shopify.com/en/manual/domains/add-a-domain/connecting-domains/connect-domain-customer-account)
- A merchant can connect a subdomain of the store's domain, after which customers "could access the sign-in page at `account.your-store.com/authentication/login`". The store must already have a connected domain, and the subdomain must be set as the primary customer accounts domain. Setup is a CNAME to `shops.myshopify.com`, and verification can take up to 48 hours. — [Shopify Help](https://help.shopify.com/en/manual/domains/add-a-domain/connecting-domains/connect-domain-customer-account); [search summary of the same help page](https://help.shopify.com/en/manual/domains/add-a-domain/connecting-domains/connect-domain-customer-account)
- Changelog, 21 May 2025: "We've improved the customer account login experience by automatically applying your branded domain to the login page." Before this, the login page stayed on shopify.com even with a custom account domain. — [Shopify Changelog: Branded URLs on customer login pages](https://changelog.shopify.com/posts/branded-urls-on-customer-login-pages)
- Older help text said the subdomain applies "except for the login page", which stays at `shopify.com/<shop_id>/auth`. Merchants also reported the address bar showing shopify.com until sign-in finished. This shows the login page was the last part to move to the tenant domain. — [Shopify Community thread](https://community.shopify.com/t/main-domain-shows-as-shopify-com-on-the-address-bar/300772); [Shopify Community: redirects to shopify.com/authentication](https://community.shopify.com/t/when-creating-a-new-account-login-shopify-redirects-to-shopify-com-authentication/416184)
- Third-party guides say one account domain serves every market, with no per-market account domain. (Secondary source; not confirmed on the help page I fetched.) — [Ringly guide](https://www.ringly.io/blog/shopify-customer-accounts)
- New customer accounts are passwordless and use email one-time codes. Legacy accounts are deprecated as of 2026. — [Ecommerce Partners update, March 2026](https://ecommercepartners.com/2024/10/01/shopifys-new-customer-accounts-advantages-and-limitations/); [Ringly](https://www.ringly.io/blog/shopify-customer-accounts)
- Shop passkeys belong to the Shop identity, not to each store. A Shop passkey works for "signing in on shop.app", Shop Pay checkout, following stores, email sign-up, and "logging into customer accounts". It requires a verified Shop Pay account. — [Shop Help: Set up a Shop Passkey](https://help.shop.app/en/shop/shop-pay-settings/security/passkeys)
- At Shop Pay checkout on a store, a passkey icon in the email field replaces the code. — [Shop Help: Check out with Shop Pay](https://help.shop.app/en/shop/shop-pay/check-out)
- Shop began rolling out passkeys in December 2022, replacing email and SMS verification. — [Shopify Engineering: Supporting passkeys in Shop authentication flows](https://shopify.engineering/supporting-passkeys-in-shop-authentication-flows)
- "Sign in with Shop" was added to classic customer accounts. It is a federated, platform-level sign-in button on tenant stores. — [Shopify Changelog](https://changelog.shopify.com/posts/sign-in-with-shop-added-to-classic-customer-accounts); [Sign in with Shop](https://www.shopify.com/shop-sign-in)
- Descope reports that Shopify Plus merchants who want enterprise SSO or custom authentication must connect their own OIDC provider. (Vendor blog; conflicts with sources saying passkeys come built in.) — [Descope blog](https://descope.com/blog/post/shopify-auth-sso)

**Substack:**

- Readers sign in to one Substack account with email, by "Email sign in link" or a password, at substack.com/sign-in. — [Substack FAQ: Logging in to Substack](https://faq.substack.com/p/logging-in-to-substack); [third-party guide](https://substackcourse.com/how-to-login-to-your-substack-profile-as-a-publisher/)
- A custom domain costs a one-time $50 and is connected by CNAME. Setup can take up to 36 hours. — [Substack announcement](https://on.substack.com/p/new-add-a-custom-domain-to-your-substack); [IONOS guide](https://www.ionos.com/digitalguide/domains/domain-tips/substack-with-custom-domain)
- Users report "For your security, we need to re-authenticate you" prompts and broken dashboards around custom-domain redirects. These are anecdotes. — [Substack comments](https://on.substack.com/p/new-add-a-custom-domain-to-your-substack/comments)

**Ghost:**

- Members sign in through Portal on the site itself with a magic link or a one-time code. The session is a first-party cookie (`ghost-members-ssr`) on the site's domain. — [Ghost Help: members sign-in](https://ghost.org/help/members-sign-in); [Ghost Changelog: one-time codes](https://ghost.org/changelog/one-time-codes-for-sign-in/); [Ghost forum: cookie not found](https://forum.ghost.org/t/member-sign-up-issue/13390)
- Ghost(Pro) supports one custom domain per site. Other domains can only redirect to it. — [Ghost forum](https://forum.ghost.org/t/pre-sales-q-auto-login-with-custom-url/44786)
- Ghost has no cross-site member identity, and I found no passkey support for members.

**Wix, Kajabi, Teachable, Webflow, Squarespace:**

- Wix site members sign in through a member login form on the site, which can be customized. Headless builds can use a Wix-hosted login page or a custom page with Wix-managed auth. — [Wix: About the member login form](https://support.wix.com/en/article/about-the-member-login-form); [Wix Headless: About member login](https://dev.wix.com/docs/go-headless/develop-your-project/authentication/members/about-member-login)
- A Kajabi site can have only one custom domain at a time. Member login pages are served on that domain. Owners and admins sign in separately at app.kajabi.com. — [Kajabi: connect a custom domain](https://help.kajabi.com/articles/website/domains/connect-a-custom-domain); [Kajabi: default subdomain vs custom domain](https://help.kajabi.com/articles/website/domains/kajabi-default-subdomain-vs-custom-domain)
- Teachable custom domains must be "DNS only" because Teachable runs Cloudflare itself. — [Teachable: custom domains](https://support.teachable.com/en/articles/11682510)
- For Webflow and Squarespace I found only third-party membership tools, such as Memberstack (which sells a "cross-domain login" feature) and Memberful. These hosted login screens can sit on a custom domain. — [Memberstack custom domains](https://docs.memberstack.com/hc/en-us/articles/15793943107995); [Memberstack cross-domain login](https://MemberStack.io/features/cross-domain-login); [Memberful website builders](https://memberful.com/docs/plans/promote-and-sell/website-builders)

**Booking platforms (Mindbody, Vagaro, Booksy, Glofox):**

- A Mindbody account gives clients one username and password that works across Mindbody products and across businesses that use Mindbody. The account is not shared with or transferred to any business. (Secondhand: an integrator's FAQ, not Mindbody's own docs.) — [LiveEdit: Mindbody account login FAQ](https://help.liveeditplatform.com/support/solutions/articles/11000129386-mindbody-account-login-faq)
- Booksy uses its consumer app and network as a marketing channel across businesses. That is commercial, not an authentication detail. — [SpreadSimple: booking platforms research](https://blog.spreadsimple.com/blog/booking-platforms-research)

### Inferences

- Platforms that run a marketplace or consumer app (Shop, Mindbody, Substack, Booksy) favor one platform identity on a platform domain. The network effect is the reason. Pure site builders (Ghost, Wix, Kajabi) keep separate members per site on the site's domain.
- Shopify's path is the closest analogue for Tetsu. It started on the platform domain with tenant IDs in the path, then moved account pages and later the login page to `account.<tenant domain>`. Passkeys stayed on one platform identity (shop.app). So the branded login page and passkey binding are separate decisions. Shopify avoided binding passkeys to thousands of tenant domains.
- Kajabi's split between `app.kajabi.com` for owners and the tenant domain for members suggests a common pattern for Tetsu: staff on the platform domain, customers on the tenant domain.

### Gaps

- I could not find official docs on how Substack carries a substack.com session onto a custom domain. From memory, it redirects to substack.com and returns with a token that sets a cookie on the custom domain, but I did not verify this.
- I could not confirm how Shop passkeys work on a store's `account.<domain>` page, whether through a popup, an iframe on shop.app, or a redirect. Shopify does not publish its RP ID. It is probably shop.app, but that is unverified.
- I did not find primary docs for Vagaro, Booksy, or Glofox client sign-in, or for Mindbody branded apps. Nothing showed whether a branded web or app sign-in uses a tenant domain.
- I found no native Webflow User Accounts or Squarespace member-area docs on login domains. Webflow User Accounts was reportedly sunset. That is unverified here.

## Identity vendors' custom-domain features and the RP ID each uses for passkeys

**Takeaway:**
Every vendor binds passkeys to the custom auth domain or its registrable parent, and every one warns that changing the domain later strands existing passkeys. Auth0 is the most explicit: each custom domain gets its own RP ID, passkeys don't carry across, and related origins are not yet supported. Clerk advises against passkeys on satellite domains.

**Cited Findings:**

- **Auth0:** "Each custom domain automatically gets its own RP ID", set to the domain itself (for example `login.example.com`). "A passkey enrolled on `login.brand1.com` cannot be used on `login.brand2.com`." When moving from one custom domain to several, "Existing passkeys only work on the original domain", and "Passkeys cannot be transferred" between custom domains. Related origins are listed as "not yet supported" and "Planned for post-GA release". — [Auth0: Multiple custom domains and passkeys](https://auth0.com/docs/customize/custom-domains/multiple-custom-domains/passkeys)
- The Auth0 Management API lets you set `relying_party_identifier` per custom domain. Setting it to null falls back to the full domain. A localized page calls per-domain RP ID setting Early Access. (Search summary; status may have changed.) — [Auth0 API: patch custom domain](https://auth0.com/docs/api/management/v2/custom-domains/patch-custom-domains-by-id); [Auth0 (ja-jp)](https://auth0.com/docs/ja-jp/customize/custom-domains/multiple-custom-domains/passkeys)
- Auth0 support documents an "Unexpected registration response origin" error. It occurs when a passkey's origin matches neither the RP ID domain nor the tenant's custom domain. — [Auth0 Support](https://support.auth0.com/center/s/article/auth0-myaccount-api-passkey-enrollment-returns-unexpected-registration-response-origin-error)
- **Clerk satellite domains:** The primary domain "is where the authentication state lives". Sign-in and sign-up always happen on the primary domain, and the satellite redirects there and back with a `__clerk_synced` parameter. An optional `satelliteAutoSync` redirects every first visit. Signing out on one domain signs out everywhere. The feature needs a paid plan, and Clerk calls it "advanced" with support it "cannot guarantee". On passkeys: "It is not recommended to use passkeys as a form of authentication when sharing sessions across different domains." — [Clerk: Satellite domains](https://clerk.com/docs/guides/dashboard/dns-domains/satellite-domains)
- The Clerk passkeys guide adds that passkeys "won't be portable between your primary domain and your satellite domains". Passkeys made on the dev Account Portal (`*.accounts.dev`) work only there. — [Clerk: passkeys custom flow](https://clerk.com/docs/guides/development/custom-flows/authentication/passkeys)
- **WorkOS AuthKit:** "Developers should configure an AuthKit custom domain before enabling passkeys in production." "Passkeys are bound to the domain they were registered on." "Adding the domain later would prevent the usage of passkeys that were registered on the old domain." Custom domains are paid and production-only, and WorkOS's Cloudflare setup conflicts with a customer's own Cloudflare proxy. — [WorkOS: AuthKit passkeys](https://workos.com/docs/authkit/passkeys); [WorkOS: Custom domains](https://workos.com/docs/custom-domains/authkit)
- **Okta / Okta CIC:** You can set the RP ID to a registrable suffix of the custom domain. If you change the RP ID, existing enrollments stop working and must be reset through the API. — [Okta Help: Customize the passkeys RP ID](https://help.okta.com/oie/en-us/content/topics/identity-engine/authenticators/passkeys-relying-party-id.htm)
- Okta lists a known issue: passkey registration or authentication fails on iOS when the RP ID is set to a root domain. — [Okta Support](https://support.okta.com/help/s/article/passkey-registration-authentication-fails-on-ios-when-relying-party-id-in-okta-is-set-to-a-root-domain)
- **Corbado:** Recommends the root domain as RP ID in production, and a subdomain only to restrict passkeys. A custom CNAME points to `<project>.frontendapi.cloud.corbado.io`, and the RP ID must match the browser URL. — [Corbado: What is the relying party ID](https://docs.corbado.com/helpful-guides/passkeys/relying-party-id); [Corbado: URL and ID configuration](https://docs.corbado.com/overview/configuration/url-id-configuration)
- **Descope:** Its cookies default to SameSite=Strict, so the auth domain must be a CNAME subdomain of the app's site. A separate "Passkey Top-Level Domain" setting picks the domain passkeys bind to. — [Descope: CNAME guide](https://docs.descope.com/tutorials/cnameguide/)
- **Stytch:** Custom domains are CNAME subdomains aliased to stytch.com. The docs I saw do not say how the passkey RP ID is set. — [Stytch: custom domains](https://stytch.com/docs/resources/branding/custom-domains)
- **AWS Cognito** requires the fully qualified custom domain as the passkey RP ID when passkeys are enabled. (Search summary.) — [AWS SDK: WebAuthnConfigurationType relyingPartyId](https://docs.aws.amazon.com/sdk-for-kotlin/api/latest/cognitoidentityprovider/aws.sdk.kotlin.services.cognitoidentityprovider.model/-web-authn-configuration-type/relying-party-id.html)

**Inferences:**

- No vendor offers "one passkey that works on every tenant's unrelated domain". The options are per-domain enrollment (Auth0 multi-domain), a shared parent domain (Corbado, Okta, Descope), or federation to one auth domain (Clerk satellites, Okta's guidance).
- The vendor model fits Tetsu's tenants poorly. Vendors expect one customer to own a handful of brand domains. Tetsu would have one domain per business, possibly thousands, and all of them unrelated.

**Gaps:**

- I found no Hanko docs on custom domains or RP ID.
- Auth0's related-origins support was "planned post-GA". I could not confirm whether it shipped by late 2026.
- Stytch's passkey RP ID under custom domains is not covered in the docs I found.

## Is a central sign-in domain with redirects back to the tenant domain a common pattern?

**Takeaway:**
Yes. Okta calls it "same-party federation" and recommends it as the most robust passkey architecture across domains. Clerk satellites and Shopify's original shopify.com login work the same way. The usual mechanics: redirect to the central domain, authenticate there, return with a one-time code or token, and have the tenant domain set its own first-party session cookie.

**Cited Findings:**

- Okta's preferred multi-domain architecture is same-party federation: host passkey login on one IdP domain, and have other apps redirect there over OIDC or SAML. Okta calls it "the most robust and platform-agnostic option", especially when domains share no suffix. Alternatives are a root-domain RP ID, which clutters passkey managers for multi-brand setups, and Related Origin Requests, where you should "always ship a fallback flow". Okta's advice: "Always use a dedicated custom domain as the RP ID", and "Once a user creates a passkey, the RP ID cannot change." — [Okta Support: Passkey implementation and architecture for multiple domains](https://support.okta.com/help/s/article/passkey-implementation-and-architecture-for-multiple-domains)
- Clerk satellites follow the same pattern. The satellite redirects to the primary for sign-in, returns, and recognizes the session through a sync parameter. Each satellite origin must be listed in the primary's `allowedRedirectOrigins`. — [Clerk: Satellite domains](https://clerk.com/docs/guides/dashboard/dns-domains/satellite-domains)
- Firebase warns that cross-domain sign-in that depends on third-party storage fails in browsers that block it. Its fixes are to proxy auth through the app's own domain or to use first-party redirect handling. This is why tenant domains need their own first-party session cookie after the redirect. — [Firebase: redirect best practices](https://firebase.google.com/docs/auth/web/redirect-best-practices)
- **Related Origin Requests** are the WebAuthn alternative. The RP hosts `/.well-known/webauthn` listing allowed origins. Clients "MUST support at least five registrable origin labels", Chrome's maximum is 5, and passkeys.dev says no known clients support more than 5. Labels count at eTLD+1, so example.com and example.de count as one label. — [web.dev: Related Origin Requests](https://web.dev/articles/webauthn-related-origin-requests); [passkeys.dev: related origins](https://passkeys.dev/docs/advanced/related-origins/); [W3C mailing list on spec text](https://lists.w3.org/Archives/Public/public-webauthn/2024Jun/0100.html)
- A secondary source claims Chrome silently ignores labels beyond 5. (Unverified.) — [MojoAuth](https://mojoauth.com/blog/related-origin-requests-passkeys-across-cctlds)

**Inferences:**

- Related Origin Requests cannot serve thousands of tenant domains because of the 5-label cap. For a platform like Tetsu, the realistic choices are (a) one platform RP ID, either a central auth domain or federation to the platform domain, or (b) per-tenant RP IDs with passkeys enrolled separately on each tenant domain.
- With a central RP ID, the tenant domain never sees the WebAuthn ceremony. It receives an OIDC-style authorization code on a callback and sets its own host-only session cookie. That keeps sessions first-party and works when third-party cookies are blocked.

**Gaps:**

- I found no public write-up from a booking SaaS describing this exact flow for end customers.

## Documented passkey breakage after a domain change or on custom domains

**Takeaway:**
Breakage is well documented as a design consequence, not as a named public incident. Changing the RP ID, including narrowing or widening it, strands every existing passkey. The fix is always re-enrollment.

**Cited Findings:**

- Auth0: "Existing passkeys only work on the original domain"; "Passkeys cannot be transferred". — [Auth0](https://auth0.com/docs/customize/custom-domains/multiple-custom-domains/passkeys)
- WorkOS: adding a custom domain later "would prevent the usage of passkeys that were registered on the old domain". — [WorkOS](https://workos.com/docs/authkit/passkeys)
- Okta: changing the RP ID invalidates existing enrollments, which must be reset through the API so users re-enroll. — [Okta Help](https://help.okta.com/oie/en-us/content/topics/identity-engine/authenticators/passkeys-relying-party-id.htm)
- Ory advises against changing the RP ID in production because it breaks passkeys for users who already signed up. — [Ory: passkeys WebAuthn security error](https://www.ory.com/docs/troubleshooting/passkeys-webauthn-security-error)
- Dfns publishes a passkey domain migration guide in which users register new credentials on the new domain. — [Dfns: passkey domain migration](https://docs.dfns.co/guides/developers/passkey-domain-migration)
- MojoAuth: changing one character of the RP ID, or moving between `login.example.com` and `example.com`, invalidates every issued passkey. (Vendor blog.) — [MojoAuth: roll back a passkey launch](https://mojoauth.com/blog/passkey-rollback-without-lockout)
- Forum reports: XenForo "invalid rpId hash" after a domain change, Bitwarden users asking how to change a passkey's domain (they cannot), and 1Password WebAuthn URL mismatch. — [XenForo](https://xenforo.com/community/threads/226456); [Bitwarden community](https://community.bitwarden.com/t/change-domain-of-a-passkey/87053); [1Password community](https://www.1password.community/developers-69/webauthn-integration-not-working-url-mismatch-22446)
- Okta has a known iOS failure when the RP ID is a root domain. — [Okta Support](https://support.okta.com/help/s/article/passkey-registration-authentication-fails-on-ios-when-relying-party-id-in-okta-is-set-to-a-root-domain)

**Inferences:**

- If Tetsu binds passkeys to each business's custom domain, a business that changes or drops its domain, or moves from a Tetsu subdomain to its own domain, strands all of its customers' passkeys. Binding to one stable platform domain avoids that.

**Gaps:**

- I found no public incident post-mortem from a large platform about mass passkey loss after a domain migration.

## Branding and trust: platform domain versus business domain, and phishing implications

**Takeaway:**
Research shows most users judge sign-in pages by branding and content, not by the URL. A familiar domain therefore adds little trust for typical users. The real security gain from passkeys is that they are origin-bound and resist phishing whatever domain the user sees. I found no study that directly compares trust in a platform-domain login with a business-domain login.

**Cited Findings:**

- In a CHI 2011 domain-highlighting study (n=22), one group of users trusted pages based on "brand, content and previous experiences". Only another group checked the address bar. — [Lin et al., CHI 2011](https://grouplab.cpsc.ucalgary.ca/grouplab/uploads/Publications/Publications/2011-DomainHighlighting.CHI.pdf)
- An online study with 411 participants found that drawing attention to the address bar and pruning the URL gave "a large improvement in phish detection". — [Berkeley EECS-2009-26](https://www2.eecs.berkeley.edu/Pubs/TechRpts/2009/EECS-2009-26.pdf)
- SSO login pages are an attractive spoofing target, since phishers "only need to spoof the login webpages of IdPs". — [Mines LEET'13 paper](https://people.mines.edu/chuanyue/wp-content/uploads/sites/256/2024/04/LEET13.pdf)
- In a USENIX Security 2021 study (n=51), only 10% of participants were judged unlikely to fall for phishing. — [Ulqinaku et al., USENIX Security 2021](https://usenix.org/system/files/sec21-ulqinaku.pdf)
- Users often ignore security indicators when they log in. — [NDSS: real-world decision making, secure vs insecure websites](https://www.ndss-symposium.org/wp-content/uploads/2017/09/real-world-decision-making-logging-into-secure-vs-insecure-websites.pdf)
- Merchants complained when Shopify showed shopify.com in the address bar during sign-in, and Shopify later branded the login page. This is evidence of merchant preference for a branded domain, not of customer distrust. — [Shopify Community](https://community.shopify.com/t/main-domain-shows-as-shopify-com-on-the-address-bar/300772); [Shopify Changelog](https://changelog.shopify.com/posts/branded-urls-on-customer-login-pages)

**Inferences:**

- When users enter codes or passwords on many tenant domains, they learn that "any domain can be a legitimate Tetsu login". That makes lookalike phishing easier. One sign-in domain gives users, and password managers, one origin to recognize.
- Passkeys weaken this argument, because a passkey won't respond on a phishing origin. Email one-time codes and magic links remain phishable fallbacks, though, so the domain-training effect still matters for them.
- Merchant demand for a branded URL is real, as Shopify's history shows. A branded `account.<business domain>` or `book.<business domain>` page can meet that demand. Passkeys and the identity can still be bound to a platform RP ID through federation.

**Gaps:**

- I found no peer-reviewed study comparing end-user trust or conversion on a platform-domain login versus a tenant-domain login. Claims either way, such as "users abandon when redirected to another domain", appear only in vendor marketing and were not cited.
- The excerpts above come from search snippets. I did not read the full papers, so exact figures should be checked before anyone quotes them.
