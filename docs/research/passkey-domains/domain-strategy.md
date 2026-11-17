# Domain and hostname strategy for Tetsu login and sessions

Researched 2026-11-11. Context: Tetsu serves businesses at `/<slug>` on one host today, uses Rust/axum on
Render behind Cloudflare, server-rendered with strict CSP, plans `__Host-` cookies, targets ASVS 5.0.0 L2.

## 1. Path-per-tenant vs subdomain-per-tenant vs custom domains: cookies, isolation, same-site, PSL, passkeys

### Takeaway

Because Tetsu serves only its own server-rendered code (no tenant-authored script), path-per-tenant on one host
gives no script isolation between tenants but needs none; `__Host-` cookies then bind sessions to the one host.
Subdomain-per-tenant buys origin isolation only if tenants can run content, and costs cookie-tossing exposure
and same-site ambiguity unless the zone is on a separate registrable domain or the PSL; a PSL zone breaks
any shared passkey RP ID across tenants, so central login must live on a host outside the tenant suffix.

### Cited Findings

- Same-origin means same scheme, hostname, and port; same-site means same scheme and same eTLD+1, so
  subdomains and ports do not matter for same-site. — [web.dev, same-site vs same-origin](https://web.dev/articles/same-site-same-origin)
- The PSL defines effective TLDs; for `https://www.project.github.io/foo` the eTLD is `.github.io` and the
  site is `project.github.io`. — [web.dev](https://web.dev/articles/same-site-same-origin)
- `__Host-` cookies "must be set with the `Secure` attribute by a secure page (HTTPS)... must not have a
  `Domain` attribute specified, and the `Path` attribute must be set to `/`." Browsers without prefix support
  accept prefixed cookies with no checks. — [MDN Set-Cookie](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Set-Cookie)
- A cookie's Domain "must be the domain of the server that sends the Set-Cookie response header, or a parent
  domain... It cannot be a public suffix such as `com`, `co.uk`, or `github.io`." Omitting Domain makes a
  host-only cookie. A response from `api.example.com` can set `Domain=example.com` but not
  `Domain=other.example.com`. — [MDN Set-Cookie](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Set-Cookie)
- SameSite controls sending on requests from a different *site* (including scheme); `Lax` allows cross-site
  only for top-level safe-method navigation. — [MDN Set-Cookie](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Set-Cookie)
- Cookie tossing: a subdomain can set cookies for its parent domain, so a sibling subdomain the attacker
  controls (via XSS or a service handing out subdomains to customers) can plant cookies the main app receives.
  A more specific Path makes the attacker cookie sort first on targeted endpoints. SameSite Lax/Strict gives no
  protection because the attack is same-site. — [Snyk Labs, 2024-11-26](https://labs.snyk.io/resources/hijacking-oauth-flows-via-cookie-tossing/)
- GitPod CVE-2024-21583: JS on a workspace subdomain of gitpod.io set the attacker's JWT cookie scoped to
  `/api/authorize` and `/auth/bitbucket/callback`, binding the victim's Bitbucket to the attacker's account;
  reported 2024-06-26, fixed 2024-07-01 by adopting the `__Host-` prefix. — [Snyk Labs](https://labs.snyk.io/resources/hijacking-oauth-flows-via-cookie-tossing/)
  (Note: the article writes "__Host__" and says such cookies cannot set Path; in fact `__Host-` requires
  `Path=/` per MDN above.)
- `__Host-` prefix bypasses (published 2025-09-03): a leading Unicode whitespace (e.g. U+2000) makes browsers
  treat the cookie as unprefixed while servers that trim (Django, ASP.NET) read it as `__Host-...`; a
  `$Version=1` legacy-parse trick affects Tomcat and Jetty. Injection is possible "from a subdomain or over an
  insecure origin". Duplicate-name handling (often last wins) lets the forged value reach the app. —
  [PortSwigger, Cookie Chaos](https://portswigger.net/research/cookie-chaos-how-to-bypass-host-and-secure-cookie-prefixes)
- Firefox CVE-2022-40958: a prepended `=` in Set-Cookie bypassed `__Secure-`/`__Host-`. —
  [Mozilla Bugzilla 1779993](https://bugzilla.mozilla.org/show_bug.cgi?id=1779993)
- GitHub moved Pages off github.com subdomains (2013-04-05) to remove "potential vectors for cross domain
  attacks targeting the main github.com session": Pages sites "may include custom JavaScript and were hosted
  on github.com subdomains", letting attackers "write (but not read) github.com domain cookies" for session
  fixation and CSRF-token fixation, and enabling phishing relying on the github.com name. The post does not
  mention the PSL. — [GitHub blog](https://github.blog/news-insights/company-news/new-github-pages-domain-github-io/)
- PSL private-section entries are for owners "who themselves issue subdomains to mutually-untrusting
  parties"; submitted only by the domain owner; verified by a `_psl` TXT record kept permanently; domain must
  have 2+ years remaining; "NO SERVICE LEVEL AGREEMENTS ON TIME"; browser/OS copies update on their own
  schedules; projects with fewer than thousands of users, or test/beta projects, are "quite likely to be
  declined"; PRs aimed only at circumventing third-party rate limits are declined. —
  [PSL Guidelines wiki](https://github.com/publicsuffix/list/wiki/Guidelines)
- WebAuthn: "The RP ID must be equal to the origin's effective domain, or a registrable domain suffix of the
  origin's effective domain"; scheme must be https (or http on localhost). — [W3C WebAuthn L3](https://www.w3.org/TR/webauthn-3/)
- The HTML Standard's "is a registrable domain suffix of or is equal to" algorithm (which WebAuthn uses)
  uses the PSL; WebKit discussion notes an exact match passes even when both are public suffixes. —
  [WebKit bug 181950](https://bugs.webkit.org/show_bug.cgi?id=181950)
- Related Origin Requests let one RP ID's passkeys be used on a limited set of other origins listed in
  `https://<rpid>/.well-known/webauthn` (`{"origins":[...]}`, served as application/json). Clients must
  support at least 5 unique registrable-origin labels, and none supports more, so treat 5 as the maximum;
  detect via `PublicKeyCredential.getClientCapabilities()` `relatedOrigins`. —
  [passkeys.dev, related origins](https://passkeys.dev/docs/advanced/related-origins/)

### Inferences

- Path-per-tenant (current): all tenants share one origin, so a script injection on any tenant page reaches
  every tenant's and staff's session on that host. This is acceptable only because Tetsu ships no
  tenant-authored HTML/JS; strict CSP plus output encoding is the control. `__Host-` cookies work with no
  change, and per-tenant authorization must be enforced server-side (a cookie cannot be scoped to `/<slug>`
  under `__Host-`, and Path is not a security boundary anyway).
- Subdomain-per-tenant on the main domain (`anvil-row.tetsu.app`, with app at `tetsu.app`): tenants become
  same-site with the app, so SameSite gives no CSRF protection between them, and any tenant subdomain that ever
  runs untrusted script, or is taken over via dangling DNS, can toss cookies onto the app. `__Host-` blocks
  Domain-scoped overwrites of the prefixed name, but the app must reject unprefixed duplicates and avoid
  whitespace-trimming cookie parsing (check the Rust `cookie` crate / axum-extra behavior).
- Putting the tenant zone on the PSL makes each tenant its own site (SameSite and cookies isolate), but then
  a passkey with RP ID `tetsu-sites.com` cannot be used from `anvil-row.tetsu-sites.com` (RP ID would be a
  public suffix). Each tenant subdomain would be its own RP ID, so a customer needs one passkey per business.
  Tetsu is also likely too small for PSL acceptance today.
- A separate registrable domain for tenant pages (e.g. `tetsu.app` for staff/operator/login,
  `tetsusites.com` for public tenant pages) gives cross-site isolation from the app without the PSL, keeps one
  RP ID on the app domain, and Related Origin Requests could extend that RP ID to at most 4 other labels, not
  per-tenant custom domains.
- Custom domains are each their own site: no cookie sharing with the app is possible, and passkeys bound to
  `tetsu.app` cannot be used there except via ROR (capped at 5 labels), so login on a custom domain must be a
  redirect to the central host plus a handoff (section 4).

### Gaps

- I did not retrieve the HTML Standard text of "is a registrable domain suffix of or is equal to" itself, nor
  WebAuthn's create/get step that rejects public-suffix RP IDs; the exact-match edge case in the WebKit thread
  suggests `rp.id == effectiveDomain` passes even for a PSL entry. Verify at
  <https://html.spec.whatwg.org/multipage/browsers.html> before relying on it.
- Did not confirm that onrender.com, netlify.app, vercel.app are on the PSL (widely believed; check
  <https://publicsuffix.org/list/public_suffix_list.dat>).
- Did not check how the Rust `cookie`/axum-extra crates handle duplicate names or Unicode whitespace.
- Current browser support matrix for Related Origin Requests not retrieved.

## 2. Separate registrable domains for realms, and what ASVS 5.0.0 says

__Takeaway:__
ASVS 5.0.0 L2 requires separate applications on different hostnames (3.5.4) and `__Host-` cookies unless
shared by design (3.3.3), and L2 HSTS must cover subdomains (3.4.1); it does not require separate registrable
domains, but major platforms use them for untrusted content, which is the precedent for any realm that serves
content Tetsu does not fully control.

### Cited Findings (ASVS 5.0.0, V3 Web Frontend Security — [source](https://raw.githubusercontent.com/OWASP/ASVS/v5.0.0/5.0/en/0x12-V3-Web-Frontend-Security.md))

- 3.3.1 (L1): cookies have the Secure attribute; if `__Host-` is not used, the name uses `__Secure-`.
- 3.3.2 (L2): each cookie's SameSite value is set according to its purpose, to limit UI redress and CSRF.
- 3.3.3 (L2): cookies use the `__Host-` prefix "unless they are explicitly designed to be shared with other
  hosts".
- 3.3.4 (L2): session-token cookies are HttpOnly and set only via Set-Cookie.
- 3.4.1 (L1): HSTS on all responses with max-age at least one year; "for L2 and up, the policy must apply to all
  subdomains as well".
- 3.4.5 (L2): referrer policy so sensitive data, including "for internal non-public applications also the
  hostname", does not leak.
- 3.5 intro: the server must ensure a sensitive request "is initiated by the application itself or by a trusted
  party"; mechanisms are Same-Origin Policy, SameSite cookies, and CORS preflight.
- 3.5.1 (L1): without reliance on preflight, sensitive cross-origin requests are "validated to ensure they
  originate from the application itself" (anti-forgery tokens or non-safelisted headers).
- 3.5.3 (L1): sensitive functions use POST/PUT/PATCH/DELETE, or strictly validate Sec-Fetch-* headers.
- 3.5.4 (L2): "separate applications are hosted on different hostnames" to use same-origin restrictions and
  hostname-based cookie restrictions.
- 3.5.5 (L2): discard postMessage from untrusted origins.
- 3.5.8 (L3): authenticated resources loadable cross-origin only when intended (Sec-Fetch-* or restrictive CORP).
- 3.2.1 (L1): prevent rendering content in the wrong context, e.g. "a user-uploaded file or other resource is
  requested directly".
- 3.7.2 (L2): "only automatically redirect the user to a different hostname or domain" on an allowlist.
- 3.7.4 (L3): top-level domain on the HSTS preload list.
- GitHub precedent: user content with custom JS moved to a separate registrable domain (github.io) to protect
  the github.com session. — [GitHub blog, 2013](https://github.blog/news-insights/company-news/new-github-pages-domain-github-io/)

__Inferences:__

- If staff dashboard, customer booking, and operator console are "separate applications", 3.5.4 at L2 wants
  distinct hostnames (e.g. `app.`, `book.`/tenant host, `ops.`), each with host-only `__Host-` cookies (3.3.3).
  Subdomains of one registrable domain satisfy 3.5.4's letter, but stay same-site, so CSRF defense must come
  from tokens/Sec-Fetch/Origin checks (3.5.1, 3.5.3), not SameSite alone.
- Operator console is the highest-value target: a separate registrable domain (cross-site from tenant pages)
  is a cheap extra boundary, and keeps it out of tenant-facing HSTS/cookie scope.
- Under 3.4.1 L2, HSTS `includeSubDomains` on the app domain covers every realm on it; a separate domain per
  realm lets each have its own HSTS policy.

__Gaps:__

- Not verified this session: Google googleusercontent.com rationale (the 2012 Google Security Blog post
  "Content hosting for the modern web", 2012-08-29, loaded without body:
  <https://security.googleblog.com/2012/08/content-hosting-for-modern-web.html>), GitHub githubusercontent.com,
  Slack slack-edge.com, and Stripe dashboard vs checkout domains. Do not cite specifics for these without
  checking.
- ASVS 5.0.0 does not appear to have a requirement naming registrable domains or the PSL; only quotes above
  were seen (WebFetch limited verbatim length, so wording is partly paraphrased).

## 3. Practicalities on Render and Cloudflare

__Takeaway:__
Render supports wildcard custom domains with automatic Let's Encrypt certs (needs the apex on Render too) and
charges $0.25/month per custom domain beyond plan allowance, so a wildcard covers a subdomain-per-tenant zone
cheaply; tenant custom domains at scale fit Cloudflare for SaaS (100 included, $0.10 each after, up to 50,000
on self-serve plans), but Render itself fronts with Cloudflare, so the two-zone (O2O) path needs care.

__Cited Findings:__

- Render: only web services and static sites support custom domains; wildcards are supported; "your root domain
  must also point to Render", else "your wildcard configuration will fail". DNS: `*` CNAME to the service's
  onrender.com host, `_acme-challenge` CNAME to `<service>.verify.renderdns.com` (Let's Encrypt issuance and
  renewal), `_cf-custom-hostname` CNAME to `<service>.hostname.renderdns.com`. With Cloudflare proxying, Render
  advises gray-cloud for the root when a wildcard is added. CAA records must allow Let's Encrypt and Google
  Trust Services, with `issuewild`. — [Render custom domains](https://render.com/docs/custom-domains)
- Render included custom domains: Hobby 2, Pro 15, Scale 25; "Each custom domain you add beyond this amount
  costs $0.25 per month." Page states no per-service limit and does not mention HSTS. — [Render custom domains](https://render.com/docs/custom-domains) (fetched 2026-11-11)
- Cloudflare for SaaS (page updated 2026-08-14): Free/Pro/Business include 100 custom hostnames, max 50,000,
  $0.10 per additional hostname; Enterprise custom pricing, "Unlimited, but contact sales if using over
  50,000"; wildcard custom hostnames Enterprise only; custom certs/CSR/selectable CA and mTLS Enterprise only;
  WAF for SaaS uses current zone plan rules below Enterprise. — [Cloudflare for SaaS plans](https://developers.cloudflare.com/cloudflare-for-platforms/cloudflare-for-saas/plans/)
- Orange-to-Orange: when a customer's own Cloudflare zone proxies a CNAME to a SaaS provider on Cloudflare,
  the customer zone's settings apply first, then the provider's; provider zone sets `cf-connecting-o2o: 1`.
  Requires different accounts, a proxied CNAME (no apex A-record), and Cloudflare DNS. —
  [Cloudflare for SaaS, how it works](https://developers.cloudflare.com/cloudflare-for-platforms/cloudflare-for-saas/saas-customers/how-it-works/)
- HSTS preload requires `includeSubDomains`, `preload`, max-age at least 31536000, HTTP-to-HTTPS redirect on the
  same host, and HTTPS on all subdomains; "applies to all subdomains, including internal subdomains";
  "inclusion in the preload list cannot easily be undone" (months via Chrome updates). — [hstspreload.org](https://hstspreload.org/)

__Inferences:__

- Render's `_cf-custom-hostname` record indicates Render itself uses Cloudflare for SaaS; Tetsu behind its own
  Cloudflare zone already runs O2O. Running Tetsu's own Cloudflare for SaaS for tenant custom domains in front
  of Render would chain zones; confirm with Render support whether that works and which zone terminates TLS.
- Alternative for few custom domains: add each tenant domain to the Render service at $0.25/month each,
  managed through Render's API; this avoids Cloudflare for SaaS until scale demands it.
- Tenant custom domains cannot be covered by Tetsu's HSTS preload; each tenant controls their own. Preloading
  `tetsu.app` with includeSubDomains is safe only if every subdomain, including future tenant subdomains, is
  HTTPS-only, which a wildcard cert covers.

__Gaps:__

- Render's maximum custom domains per service, its API for adding domains, and wildcard plan restrictions not
  found on the page.
- Cloudflare for SaaS prices are as of the 2026-08-14 page update; whether Cloudflare for SaaS can sit in front
  of a Render origin that itself uses Cloudflare for SaaS was not confirmed.

## 4. Session establishment on tenant custom domains after central login

__Takeaway:__
Use an OAuth-authorization-code-shaped handoff: central host authenticates, redirects to an exact,
pre-registered callback on the tenant domain with a short-lived single-use code, and the tenant host redeems
it server-side and sets its own `__Host-` cookie; bind the flow with state/PKCE to stop login CSRF and
code injection.

__Cited Findings:__

- RFC 9700 (OAuth 2.0 Security BCP, January 2025): codes "MUST be invalidated by the authorization server after
  their first use at the token endpoint" (4.2.4); authorization servers "MUST support PKCE", public clients
  "MUST use PKCE", PKCE "RECOMMENDED" for confidential clients (2.1.1); clients must prevent CSRF and "MAY rely
  on the CSRF protection provided by PKCE", or use a random `state` CSRF token (2.1, 4.7.1); redirect URIs use
  "exact string matching" (2.1, 4.1.3); clients and servers "MUST NOT expose URLs that forward the user's
  browser to arbitrary URIs" (2.1). — [RFC 9700](https://www.rfc-editor.org/rfc/rfc9700.html)
- ASVS 5.0.0 3.7.2 (L2): automatic redirects to another hostname or domain only to allowlisted destinations. —
  [ASVS V3](https://raw.githubusercontent.com/OWASP/ASVS/v5.0.0/5.0/en/0x12-V3-Web-Frontend-Security.md)
- Cookie tossing on OAuth callback paths shows callback endpoints are prime targets for session-fixation. —
  [Snyk Labs](https://labs.snyk.io/resources/hijacking-oauth-flows-via-cookie-tossing/)

__Inferences:__

- Pattern: (1) tenant host `book.anvilrow.com` sets a `__Host-` pre-auth cookie holding a random state and
  PKCE verifier, redirects to `tetsu.app/signin?tenant=..&state=..&code_challenge=..`; (2) central host
  validates the return URL against the tenant's registered custom domain (exact match), authenticates (passkey
  RP ID `tetsu.app`), stores a random code bound to user, tenant, return origin, challenge, with a lifetime of
  seconds; (3) redirects to the tenant callback; (4) tenant host checks state against its cookie, redeems the
  code over a server-side call (or a shared DB, since both run in the same Tetsu service) with the verifier,
  deletes it atomically, rotates and sets a fresh `__Host-` session cookie, then redirects to a fixed path.
- Risks: replay (single-use, short TTL, atomic delete), injection of an attacker's code / login CSRF (state and
  PKCE bound to the pre-auth cookie), open redirect (exact allowlist of tenant domains, per 3.7.2), code leaks
  via Referer or logs (put the code in a POST form auto-submit or strip it with an immediate redirect; set
  `Referrer-Policy: no-referrer` on the callback), session fixation (always issue a new session ID after
  redemption), and cross-tenant use (bind the code to the tenant origin and check on redemption).
- Because Tetsu is one service, redemption can be a DB lookup rather than a network token call; the security
  properties come from single use, binding, and short TTL, not from the transport.

__Gaps:__

- No primary source found this session describing a named SaaS's (e.g. Shopify, Squarespace) custom-domain
  session handoff; RFC 9700 code lifetime guidance was not in the portion read (RFC 6749 recommends a maximum
  of 10 minutes, not verified this session).
