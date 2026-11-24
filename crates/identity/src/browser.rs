//! What a browser carries for login: each realm's session cookie, and the origin a state-changing request comes
//! from (ADR-0048). Plain text in and out, so no HTTP framework's types reach Identity (ADR-0007); the adapter
//! that serves pages sets and reads the headers.

use crate::domain::{Error, FailedCheck, Realm, Session, SessionToken};

impl Realm {
    /// The cookie this realm's session token travels in. The `__Host-` prefix makes a browser refuse it unless it
    /// is `Secure`, has path `/`, and names no domain, so no other host, not even a subdomain, can set it
    /// (`v5.0.0-3.3.3`).
    #[must_use]
    pub fn cookie_name(self) -> &'static str {
        match self {
            Self::Admin => "__Host-tetsu-admin",
        }
    }

    /// The `Set-Cookie` value that hands a browser a session's token. Scripts cannot read it, other sites' forms
    /// do not send it, and it lasts no longer than the session can (ADR-0048).
    #[must_use]
    pub fn session_cookie(self, token: &SessionToken) -> String {
        format!(
            "{}={}; {ATTRIBUTES}; Max-Age={}",
            self.cookie_name(),
            token.to_text(),
            self.limits().absolute.as_secs()
        )
    }

    /// The `Set-Cookie` value that removes the session cookie, as logging out does.
    #[must_use]
    pub fn cleared_cookie(self) -> String {
        format!("{}=; {ATTRIBUTES}; Max-Age=0", self.cookie_name())
    }

    /// The session token text among every `Cookie` header a request carries. HTTP/2 lets a browser split its
    /// cookies across several, so the caller passes them all, never only the first. A cookie sent twice, in one
    /// header or across several, is ambiguous, as when another site planted a second one, so it names none.
    #[must_use]
    pub fn token_in<'a>(self, headers: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
        let mut found = headers
            .into_iter()
            .flat_map(|header| header.split(';'))
            .filter_map(|pair| {
                // ASCII whitespace alone: a name led by Unicode whitespace is another cookie to the browser.
                let (name, value) = pair.trim_ascii().split_once('=')?;
                (name == self.cookie_name()).then_some(value)
            });
        let first = found.next()?;
        found.next().is_none().then_some(first)
    }
}

const ATTRIBUTES: &str = "Path=/; Secure; HttpOnly; SameSite=Lax";

/// What a state-changing request brings for login's checks: every `Cookie` header it carries, its `Origin` and
/// `Sec-Fetch-Site` headers, and the form token it submitted, as the page adapter reads them. Its parts are private, so the session token is
/// always read through [`Realm::token_in`], and a cookie sent twice is never taken for one. It has no `Debug`, so it
/// is never printed with a token inside.
pub struct Change<'a> {
    cookies: Vec<&'a str>,
    origin: Option<&'a str>,
    fetch_site: Option<&'a str>,
    form_token: &'a str,
}

impl<'a> Change<'a> {
    #[must_use]
    /// A change from its request's `Cookie` headers, its `Origin`, its `Sec-Fetch-Site`, which [`Origins::admit`]
    /// checks along with the origin, and the form token it submitted. Every part is required, so a page adapter
    /// cannot leave a check out by forgetting it.
    pub fn new(
        cookies: impl IntoIterator<Item = &'a str>,
        origin: Option<&'a str>,
        fetch_site: Option<&'a str>,
        form_token: &'a str,
    ) -> Self {
        Self {
            cookies: cookies.into_iter().collect(),
            origin,
            fetch_site,
            form_token,
        }
    }

    /// The realm's session token, if the request carries its cookie once, is admitted by these origins, and
    /// carries that session's own form token, compared in constant time through `subtle`, which a test cannot time
    /// reliably (ADR-0048, `v5.0.0-3.5.1`). The origin comes first: it holds no secret, and a browser sends no
    /// `SameSite=Lax` cookie with another site's form, so a request from elsewhere is told as one.
    ///
    /// # Errors
    /// Fails with the first check the request failed: its origin, its cookie, then its form token.
    pub fn verified(&self, realm: Realm, origins: &Origins) -> Result<SessionToken, FailedCheck> {
        if !origins.admit(self.origin, self.fetch_site) {
            return Err(FailedCheck::Origin);
        }
        let token = realm
            .token_in(self.cookies.iter().copied())
            .and_then(SessionToken::parse)
            .ok_or(FailedCheck::Cookie)?;
        if !token.form_token_matches(self.form_token) {
            return Err(FailedCheck::FormToken);
        }
        Ok(token)
    }
}

/// A live session whose request passed the checks for a change: the realm's own origin and the session's form
/// token. Only [`crate::Admins::session_for_change`] makes one, so a use case that changes state on a session's
/// behalf, such as adding a passkey, cannot be reached without them. It stands for the one request it checked: it
/// cannot be cloned, and a page adapter must not keep it past that request, since a later one would skip the
/// checks.
#[derive(Debug, PartialEq, Eq)]
pub struct CheckedSession(pub(crate) Session);

impl CheckedSession {
    #[must_use]
    pub fn session(&self) -> &Session {
        &self.0
    }
}

/// What a state-changing request may do. A refused request is told apart from one whose session has ended, so the
/// page adapter can answer it as forbidden and record it (`v5.0.0-3.5.1`, `v5.0.0-16.3.3`).
#[derive(Debug, PartialEq, Eq)]
pub enum ForChange {
    /// The request passed the checks, and acts for this live session.
    Acting(CheckedSession),
    /// The request passed the checks, but its session has ended or never was.
    NoSession,
    /// The request failed a check: it came from elsewhere, or without its session's form token, or with its cookie
    /// missing or sent twice.
    Refused(FailedCheck),
}

/// What logging out did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Logout {
    /// A live session ended.
    Ended,
    /// The request passed the checks, but its session had already ended or never was.
    NothingToEnd,
    /// The request failed a check: it came from elsewhere, or without its session's form token, or with its cookie
    /// missing or sent twice.
    Refused(FailedCheck),
}

/// The origins a realm's pages are served from, such as `https://tetsu-demo-admin.onrender.com`. A state-changing
/// request is accepted only from one of them, so another site's form or script can never act for a logged-in
/// person (ADR-0048, `v5.0.0-3.5.1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origins(Vec<String>);

impl Origins {
    /// Origins written in the form browsers send them: a scheme, a lowercase host of plain labels, and a port only
    /// when it is not the scheme's own, with no path. Each is `https`, or `http` for this machine alone, as when
    /// running Tetsu locally. The check catches a mistyped setting; it is not a full host name grammar, and does
    /// not need to be, since a request's origin must match one of these exactly.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`] for an empty list, or an origin that is not written that way.
    pub fn parse(origins: &[&str]) -> Result<Self, Error> {
        if origins.is_empty() {
            return Err(Error::Invalid("at least one origin is needed".to_string()));
        }
        origins
            .iter()
            .map(|origin| origin_from(origin))
            .collect::<Result<_, _>>()
            .map(Self)
    }

    /// Whether a request's `Origin` header names one of these origins exactly. A request with none, or with the
    /// `null` a browser sends from a sandbox or a redirect, is refused.
    #[must_use]
    pub fn allow(&self, origin: Option<&str>) -> bool {
        origin.is_some_and(|origin| self.0.iter().any(|allowed| allowed == origin))
    }

    /// Whether a state-changing request may go on, with or without a session: its `Origin` must name one of these
    /// exactly, and its `Sec-Fetch-Site`, when the browser sends one, must say it came from the same origin. Two
    /// independent signals, so a single browser bug opens no gap (ADR-0059). A request before login has nothing
    /// else to check, so its page adapter calls this directly.
    #[must_use]
    pub fn admit(&self, origin: Option<&str>, fetch_site: Option<&str>) -> bool {
        self.allow(origin) && fetch_site.is_none_or(|site| site == "same-origin")
    }
}

fn origin_from(text: &str) -> Result<String, Error> {
    let written_as_browsers_send = text.split_once("://").is_some_and(|(scheme, authority)| {
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        };
        host_fits(scheme, host) && port.is_none_or(|port| port_fits(scheme, port))
    });
    if written_as_browsers_send {
        Ok(text.to_string())
    } else {
        Err(Error::Invalid(format!(
            "{text:?} is not an origin such as https://example.com"
        )))
    }
}

/// An `https` host of dot-separated labels, or this machine over `http`.
fn host_fits(scheme: &str, host: &str) -> bool {
    match scheme {
        "https" => host.split('.').all(label),
        "http" => matches!(host, "localhost" | "127.0.0.1"),
        _ => false,
    }
}

/// One part of a host name: lowercase letters, digits, and hyphens, never starting or ending with a hyphen.
fn label(label: &str) -> bool {
    let bytes = label.as_bytes();
    !bytes.is_empty()
        && bytes.first() != Some(&b'-')
        && bytes.last() != Some(&b'-')
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// A port as a browser writes it: a number from 1 to 65535 in its plain form, so no sign or leading zero, and never
/// the scheme's own, which a browser leaves out of `Origin`.
fn port_fits(scheme: &str, port: &str) -> bool {
    let default = if scheme == "https" { 443 } else { 80 };
    port.parse::<u16>()
        .is_ok_and(|number| number != 0 && number != default && number.to_string() == port)
}

/// The longest return path a login keeps; an admin page's address is far shorter.
const LONGEST_RETURN: usize = 120;

/// Where a login may send the browser next, if `raw` is one of the admin pages: `/admin`, or a path under it
/// written only in ASCII letters, digits, `-`, `_`, and `.`, with no empty, `.`, or `..` segment. So it can name no
/// other site, scheme, or realm, and needs no decoding, which leaves nothing for a browser to read differently
/// (OWASP, "Unvalidated Redirects and Forwards").
#[must_use]
pub fn return_path(raw: &str) -> Option<&str> {
    let rest = raw.strip_prefix("/admin")?;
    let plain = |segment: &str| {
        !matches!(segment, "" | "." | "..")
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    };
    let fits = raw.len() <= LONGEST_RETURN
        && (rest.is_empty()
            || rest
                .strip_prefix('/')
                .is_some_and(|rest| rest.split('/').all(plain)));
    fits.then_some(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn a_return_path_is_an_admin_page_written_plainly() {
        for path in [
            "/admin",
            "/admin/passkeys",
            "/admin/a-b/c_d/e.f",
            "/admin/2026",
        ] {
            assert_eq!(return_path(path), Some(path), "{path}");
        }
    }

    #[test]
    fn a_return_path_names_no_other_site_scheme_or_page() {
        let long = format!("/admin/{}", "a".repeat(200));
        for path in [
            "",
            "/",
            "/adminx",
            "/admin/",
            "//evil.example/admin",
            "/admin//evil.example",
            "/admin/../login",
            "/admin/./a",
            "/admin/a/..",
            "/admin\\evil.example",
            "/admin/%2e%2e",
            "/admin/a?b",
            "/admin/a#b",
            "/admin/a b",
            "/admin/é",
            "https://evil.example/admin",
            "javascript:alert(1)",
            "admin/a",
            long.as_str(),
        ] {
            assert_eq!(return_path(path), None, "{path}");
        }
    }

    proptest! {
        #[test]
        fn a_return_path_is_either_refused_or_a_plain_admin_page(
            raw in prop_oneof!["\\PC{0,40}", "/admin(/[a-z.%]{0,3}){0,4}"],
        ) {
            if let Some(path) = return_path(&raw) {
                prop_assert!(path == "/admin" || path.starts_with("/admin/"));
                // With a slash after it, an empty, `.`, or `..` segment shows as one of these anywhere in it.
                let ended = format!("{path}/");
                for bad in ["//", "/./", "/../", "%"] {
                    prop_assert!(!ended.contains(bad), "{} holds {}", path, bad);
                }
            } else {
                prop_assert_ne!(raw.as_str(), "/admin");
            }
        }
    }

    fn token() -> SessionToken {
        SessionToken::parse(&"ab".repeat(32)).expect("a token")
    }

    #[test]
    fn a_session_cookie_is_host_only_secure_and_hidden_from_scripts() {
        assert_eq!(
            Realm::Admin.session_cookie(&token()),
            format!(
                "__Host-tetsu-admin={}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=28800",
                "ab".repeat(32)
            )
        );
        assert_eq!(
            Realm::Admin.cleared_cookie(),
            "__Host-tetsu-admin=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0"
        );
    }

    #[test]
    fn the_token_is_found_among_other_cookies() {
        let token = "ab".repeat(32);
        assert_eq!(
            Realm::Admin
                .token_in([format!("theme=dark; __Host-tetsu-admin={token}; lang=en").as_str()]),
            Some(token.as_str())
        );
        assert_eq!(
            Realm::Admin.token_in(["theme=dark", format!("__Host-tetsu-admin={token}").as_str()]),
            Some(token.as_str())
        );
    }

    /// A name led by Unicode whitespace is another cookie, which a browser stores apart from ours, so it is never
    /// read as ours: only ASCII whitespace is trimmed, which closes the cookie prefix bypass published in September
    /// 2025.
    #[test]
    fn a_name_led_by_unicode_whitespace_is_not_ours() {
        let token = "ab".repeat(32);
        for space in ["\u{2000}", "\u{a0}", "\u{85}"] {
            let planted = format!("{space}__Host-tetsu-admin={token}");
            assert_eq!(Realm::Admin.token_in([planted.as_str()]), None, "{space:?}");
        }
        let spaced = format!(" \t__Host-tetsu-admin={token}");
        assert_eq!(
            Realm::Admin.token_in([spaced.as_str()]),
            Some(token.as_str())
        );
    }

    /// A second copy in another `Cookie` header, as HTTP/2 may send, is still a second copy.
    #[test]
    fn a_cookie_sent_twice_across_headers_names_no_token() {
        assert_eq!(
            Realm::Admin.token_in(["__Host-tetsu-admin=a", "__Host-tetsu-admin=b"]),
            None
        );
        assert_eq!(Realm::Admin.token_in([]), None);
    }

    #[test]
    fn no_cookie_or_a_cookie_sent_twice_names_no_token() {
        for cookies in [
            "",
            "theme=dark",
            "tetsu-admin=x",
            "__host-tetsu-admin=x",
            "__Host-tetsu-admin",
            "__Host-tetsu-admin=a; __Host-tetsu-admin=b",
            "__Host-tetsu-admin=a; __Host-tetsu-admin=a",
        ] {
            assert_eq!(Realm::Admin.token_in([cookies]), None, "{cookies:?}");
        }
    }

    /// A change passes only with the session cookie sent once, a listed origin, no other site named as its source,
    /// and that session's form token.
    #[test]
    fn a_change_is_verified_only_with_every_part_right() {
        let origins = Origins::parse(&["https://admin.example.com"]).expect("origins");
        let token = token();
        let (form, cookie) = (
            token.form_token().expect("a form token"),
            format!("__Host-tetsu-admin={}", token.to_text()),
        );
        let origin = Some("https://admin.example.com");
        assert_eq!(
            Change::new([cookie.as_str()], origin, None, &form).verified(Realm::Admin, &origins),
            Ok(token.clone())
        );
        // Each names the first check it failed: the origin, then the cookie, then the form token.
        for (change, failed) in [
            (
                Change::new([cookie.as_str(), cookie.as_str()], origin, None, &form),
                FailedCheck::Cookie,
            ),
            (Change::new([], origin, None, &form), FailedCheck::Cookie),
            (
                Change::new(["__Host-tetsu-admin=not-a-token"], origin, None, &form),
                FailedCheck::Cookie,
            ),
            (
                Change::new([], Some("https://evil.example"), None, ""),
                FailedCheck::Origin,
            ),
            (
                Change::new([cookie.as_str()], Some("https://evil.example"), None, &form),
                FailedCheck::Origin,
            ),
            (
                Change::new([cookie.as_str()], Some("https://evil.example"), None, ""),
                FailedCheck::Origin,
            ),
            (
                Change::new([cookie.as_str()], origin, None, ""),
                FailedCheck::FormToken,
            ),
            (
                Change::new([cookie.as_str()], origin, Some("cross-site"), &form),
                FailedCheck::Origin,
            ),
        ] {
            assert_eq!(change.verified(Realm::Admin, &origins), Err(failed));
        }
    }

    #[test]
    fn origins_are_https_or_this_machine_and_nothing_else() {
        assert!(
            Origins::parse(&[
                "https://tetsu-demo-admin.onrender.com",
                "https://admin.example.com:8443",
                "https://ops2.example.com",
                "http://localhost:8080",
                "http://127.0.0.1:8080",
            ])
            .is_ok()
        );
        let refused = [
            "tetsu-demo-admin.onrender.com",
            "http://tetsu-demo-admin.onrender.com",
            "https://tetsu-demo-admin.onrender.com/",
            "https://Tetsu-Demo-Admin.onrender.com",
            "https://admin.example.com:99999",
            "https://admin.example.com:+8443",
            "https://admin.example.com:08443",
            "https://admin.example.com:0",
            "https://admin.example.com:443",
            "http://localhost:80",
            "https://.",
            "https://-",
            "https://admin..example.com",
            "https://-admin.example.com",
            "https://admin-.example.com",
            "https://ops_example.com",
            "https://admin.example.com:",
            "https://",
            "https://user@admin.example.com",
            "ftp://admin.example.com",
            "null",
        ];
        for origin in refused {
            assert_eq!(
                Origins::parse(&[origin]),
                Err(Error::Invalid(format!(
                    "{origin:?} is not an origin such as https://example.com"
                ))),
                "{origin}"
            );
        }
        assert_eq!(
            Origins::parse(&[]),
            Err(Error::Invalid("at least one origin is needed".to_string()))
        );
    }

    #[test]
    fn only_an_exact_listed_origin_is_allowed() {
        let origins = Origins::parse(&["https://admin.example.com", "http://localhost:8080"])
            .expect("origins");
        assert!(origins.allow(Some("https://admin.example.com")));
        assert!(origins.allow(Some("http://localhost:8080")));
        for other in [
            None,
            Some("null"),
            Some(""),
            Some("https://admin.example.com.evil.example"),
            Some("https://evil.example"),
            Some("http://admin.example.com"),
            Some("https://admin.example.com:443"),
            Some("https://OPS.example.com"),
            Some("http://localhost:8081"),
        ] {
            assert!(!origins.allow(other), "{other:?}");
        }
    }

    /// A state-changing request is admitted only from a listed origin, and, when the browser says where the request
    /// came from, only from the same origin: two independent signals, so one browser bug opens no gap (ADR-0059).
    #[test]
    fn a_request_is_admitted_only_from_its_own_origin_by_both_signals() {
        let origins = Origins::parse(&["https://admin.example.com"]).expect("origins");
        let ours = Some("https://admin.example.com");
        assert!(origins.admit(ours, Some("same-origin")));
        assert!(
            origins.admit(ours, None),
            "an older browser sends no Sec-Fetch-Site"
        );
        for site in ["cross-site", "same-site", "none", "", "Same-Origin"] {
            assert!(!origins.admit(ours, Some(site)), "{site}");
        }
        assert!(!origins.admit(Some("https://evil.example"), Some("same-origin")));
        assert!(!origins.admit(None, Some("same-origin")));
    }

    proptest! {
        /// Whatever a browser sends, an origin is allowed only if it is listed exactly.
        #[test]
        fn nothing_unlisted_is_allowed(origin in "\\PC{0,40}") {
            let origins = Origins::parse(&["https://admin.example.com"]).expect("origins");
            prop_assert_eq!(origins.allow(Some(&origin)), origin == "https://admin.example.com");
        }

        /// A token set in a cookie is read back from among any other cookies.
        #[test]
        fn a_set_token_reads_back(text in "[0-9a-f]{64}", before in "[a-z]{1,8}=[a-z0-9]{0,8}") {
            let token = SessionToken::parse(&text).expect("a token");
            let set = Realm::Admin.session_cookie(&token);
            let pair = set.split(';').next().expect("a pair");
            let cookies = format!("{before}; {pair}");
            prop_assert_eq!(Realm::Admin.token_in([cookies.as_str()]), Some(text.as_str()));
        }
    }
}
