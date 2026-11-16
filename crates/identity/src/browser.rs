//! What a browser carries for sign-in: each realm's session cookie, and the origin a state-changing request comes
//! from (ADR-0048). Plain text in and out, so no HTTP framework's types reach Identity (ADR-0007); the adapter
//! that serves pages sets and reads the headers.

use crate::domain::{Error, Realm, Session, SessionToken};

impl Realm {
    /// The cookie this realm's session token travels in. The `__Host-` prefix makes a browser refuse it unless it
    /// is `Secure`, has path `/`, and names no domain, so no other host, not even a subdomain, can set it
    /// (`v5.0.0-3.3.3`).
    #[must_use]
    pub fn cookie_name(self) -> &'static str {
        match self {
            Self::Operator => "__Host-tetsu-operator",
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

    /// The `Set-Cookie` value that removes the session cookie, as signing out does.
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

/// What a state-changing request brings for sign-in's checks: every `Cookie` header it carries, its `Origin` header,
/// and the form token it submitted, as the page adapter reads them. Its parts are private, so the session token is
/// always read through [`Realm::token_in`], and a cookie sent twice is never taken for one. It has no `Debug`, so it
/// is never printed with a token inside.
pub struct Change<'a> {
    cookies: Vec<&'a str>,
    origin: Option<&'a str>,
    form_token: &'a str,
}

impl<'a> Change<'a> {
    #[must_use]
    pub fn new(
        cookies: impl IntoIterator<Item = &'a str>,
        origin: Option<&'a str>,
        form_token: &'a str,
    ) -> Self {
        Self {
            cookies: cookies.into_iter().collect(),
            origin,
            form_token,
        }
    }

    /// The realm's session token, if the request carries its cookie once, comes from one of these origins, and
    /// carries that session's own form token, compared in constant time through `subtle`, which a test cannot time
    /// reliably (ADR-0048, `v5.0.0-3.5.1`).
    #[must_use]
    pub fn verified(&self, realm: Realm, origins: &Origins) -> Option<SessionToken> {
        let token = SessionToken::parse(realm.token_in(self.cookies.iter().copied())?)?;
        (origins.allow(self.origin) && token.form_token_matches(self.form_token)).then_some(token)
    }
}

/// A live session whose request passed the checks for a change: the realm's own origin and the session's form
/// token. Only [`crate::Operators::session_for_change`] makes one, so a use case that changes state on a session's
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
    /// The request came from elsewhere, or without its session's form token, or with its cookie sent twice.
    Refused,
}

/// What signing out did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignOut {
    /// A live session ended.
    Ended,
    /// The request passed the checks, but its session had already ended or never was.
    NothingToEnd,
    /// The request came from elsewhere, or without its session's form token, or with its cookie sent twice.
    Refused,
}

/// The origins a realm's pages are served from, such as `https://tetsu-demo-ops.onrender.com`. A state-changing
/// request is accepted only from one of them, so another site's form or script can never act for a signed-in
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

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn token() -> SessionToken {
        SessionToken::parse(&"ab".repeat(32)).expect("a token")
    }

    #[test]
    fn a_session_cookie_is_host_only_secure_and_hidden_from_scripts() {
        assert_eq!(
            Realm::Operator.session_cookie(&token()),
            format!(
                "__Host-tetsu-operator={}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=28800",
                "ab".repeat(32)
            )
        );
        assert_eq!(
            Realm::Operator.cleared_cookie(),
            "__Host-tetsu-operator=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0"
        );
    }

    #[test]
    fn the_token_is_found_among_other_cookies() {
        let token = "ab".repeat(32);
        assert_eq!(
            Realm::Operator
                .token_in([format!("theme=dark; __Host-tetsu-operator={token}; lang=en").as_str()]),
            Some(token.as_str())
        );
        assert_eq!(
            Realm::Operator.token_in([
                "theme=dark",
                format!("__Host-tetsu-operator={token}").as_str()
            ]),
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
            let planted = format!("{space}__Host-tetsu-operator={token}");
            assert_eq!(
                Realm::Operator.token_in([planted.as_str()]),
                None,
                "{space:?}"
            );
        }
        let spaced = format!(" \t__Host-tetsu-operator={token}");
        assert_eq!(
            Realm::Operator.token_in([spaced.as_str()]),
            Some(token.as_str())
        );
    }

    /// A second copy in another `Cookie` header, as HTTP/2 may send, is still a second copy.
    #[test]
    fn a_cookie_sent_twice_across_headers_names_no_token() {
        assert_eq!(
            Realm::Operator.token_in(["__Host-tetsu-operator=a", "__Host-tetsu-operator=b"]),
            None
        );
        assert_eq!(Realm::Operator.token_in([]), None);
    }

    #[test]
    fn no_cookie_or_a_cookie_sent_twice_names_no_token() {
        for cookies in [
            "",
            "theme=dark",
            "tetsu-operator=x",
            "__host-tetsu-operator=x",
            "__Host-tetsu-operator",
            "__Host-tetsu-operator=a; __Host-tetsu-operator=b",
            "__Host-tetsu-operator=a; __Host-tetsu-operator=a",
        ] {
            assert_eq!(Realm::Operator.token_in([cookies]), None, "{cookies:?}");
        }
    }

    /// A change passes only with the session cookie sent once, a listed origin, and that session's form token.
    #[test]
    fn a_change_is_verified_only_with_every_part_right() {
        let origins = Origins::parse(&["https://ops.example.com"]).expect("origins");
        let token = token();
        let (form, cookie) = (
            token.form_token().expect("a form token"),
            format!("__Host-tetsu-operator={}", token.to_text()),
        );
        let origin = Some("https://ops.example.com");
        assert_eq!(
            Change::new([cookie.as_str()], origin, &form).verified(Realm::Operator, &origins),
            Some(token.clone())
        );
        for change in [
            Change::new([cookie.as_str(), cookie.as_str()], origin, &form),
            Change::new([], origin, &form),
            Change::new(["__Host-tetsu-operator=not-a-token"], origin, &form),
            Change::new([cookie.as_str()], Some("https://evil.example"), &form),
            Change::new([cookie.as_str()], origin, ""),
        ] {
            assert_eq!(change.verified(Realm::Operator, &origins), None);
        }
    }

    #[test]
    fn origins_are_https_or_this_machine_and_nothing_else() {
        assert!(
            Origins::parse(&[
                "https://tetsu-demo-ops.onrender.com",
                "https://ops.example.com:8443",
                "https://ops2.example.com",
                "http://localhost:8080",
                "http://127.0.0.1:8080",
            ])
            .is_ok()
        );
        let refused = [
            "tetsu-demo-ops.onrender.com",
            "http://tetsu-demo-ops.onrender.com",
            "https://tetsu-demo-ops.onrender.com/",
            "https://Tetsu-Demo-Ops.onrender.com",
            "https://ops.example.com:99999",
            "https://ops.example.com:+8443",
            "https://ops.example.com:08443",
            "https://ops.example.com:0",
            "https://ops.example.com:443",
            "http://localhost:80",
            "https://.",
            "https://-",
            "https://ops..example.com",
            "https://-ops.example.com",
            "https://ops-.example.com",
            "https://ops_example.com",
            "https://ops.example.com:",
            "https://",
            "https://user@ops.example.com",
            "ftp://ops.example.com",
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
        let origins =
            Origins::parse(&["https://ops.example.com", "http://localhost:8080"]).expect("origins");
        assert!(origins.allow(Some("https://ops.example.com")));
        assert!(origins.allow(Some("http://localhost:8080")));
        for other in [
            None,
            Some("null"),
            Some(""),
            Some("https://ops.example.com.evil.example"),
            Some("https://evil.example"),
            Some("http://ops.example.com"),
            Some("https://ops.example.com:443"),
            Some("https://OPS.example.com"),
            Some("http://localhost:8081"),
        ] {
            assert!(!origins.allow(other), "{other:?}");
        }
    }

    proptest! {
        /// Whatever a browser sends, an origin is allowed only if it is listed exactly.
        #[test]
        fn nothing_unlisted_is_allowed(origin in "\\PC{0,40}") {
            let origins = Origins::parse(&["https://ops.example.com"]).expect("origins");
            prop_assert_eq!(origins.allow(Some(&origin)), origin == "https://ops.example.com");
        }

        /// A token set in a cookie is read back from among any other cookies.
        #[test]
        fn a_set_token_reads_back(text in "[0-9a-f]{64}", before in "[a-z]{1,8}=[a-z0-9]{0,8}") {
            let token = SessionToken::parse(&text).expect("a token");
            let set = Realm::Operator.session_cookie(&token);
            let pair = set.split(';').next().expect("a pair");
            let cookies = format!("{before}; {pair}");
            prop_assert_eq!(Realm::Operator.token_in([cookies.as_str()]), Some(text.as_str()));
        }
    }
}
