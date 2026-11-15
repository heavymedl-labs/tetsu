//! Identity's values. Fields are private, so every value passes its checks when it is made, and a session token is
//! never printed.

use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

const MAX_NAME: usize = 100;
/// 256 random bits (ADR-0048).
const TOKEN_BYTES: usize = 32;
const HEX: &[u8; 16] = b"0123456789abcdef";

/// Which kind of person signs in. Each realm has its own host, passkeys, cookie, and session limits (ADR-0040,
/// ADR-0048); operators come first, in slice 5.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Realm {
    Operator,
}

impl Realm {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Operator => "operator",
        }
    }

    /// The realm a stored name names, if any.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "operator" => Some(Self::Operator),
            _ => None,
        }
    }

    /// How long a session lasts in this realm, within NIST SP 800-63B-4's Level 2 ceiling (ADR-0048).
    #[must_use]
    pub fn limits(self) -> Limits {
        match self {
            Self::Operator => Limits {
                absolute: Duration::from_hours(8),
                idle: Duration::from_mins(15),
            },
        }
    }
}

/// A session ends at whichever comes first: `absolute` after it starts, or `idle` after its last request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub absolute: Duration,
    pub idle: Duration,
}

/// An operator, as Postgres numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperatorId(i64);

impl OperatorId {
    pub(crate) fn new(id: i64) -> Self {
        Self(id)
    }

    #[must_use]
    pub fn get(self) -> i64 {
        self.0
    }
}

/// What an operator is called in Tetsu's own tools and the audit record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorName(String);

impl OperatorName {
    /// A name of 1 to 100 characters, trimmed of whitespace at its ends, with no control characters.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`] for an empty or overlong name, or one holding a control character.
    pub fn parse(name: &str) -> Result<Self, Error> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > MAX_NAME {
            return Err(Error::Invalid(format!(
                "a name has 1 to {MAX_NAME} characters"
            )));
        }
        if name.chars().any(char::is_control) {
            return Err(Error::Invalid(
                "a name has no control characters".to_string(),
            ));
        }
        Ok(Self(name.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What a session's cookie holds: 256 random bits, written as 64 lowercase hexadecimal digits. Only its hash is
/// stored (ADR-0047), and it is never printed, so neither a log line nor a stored row can sign anyone in.
#[derive(Clone, PartialEq, Eq)]
pub struct SessionToken([u8; TOKEN_BYTES]);

impl SessionToken {
    /// A fresh token from a secure random source: the operating system's, given as `getrandom::fill`, outside
    /// tests.
    ///
    /// # Errors
    /// Fails with [`Error::Unavailable`] if the source cannot be read.
    pub(crate) fn generate(fill: Fill) -> Result<Self, Error> {
        let mut bytes = [0; TOKEN_BYTES];
        fill(&mut bytes).map_err(no_randomness)?;
        Ok(Self(bytes))
    }

    /// The token a cookie holds, if it holds one in the form [`Self::to_text`] writes, and nothing else.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let digits = text.as_bytes();
        if digits.len() != TOKEN_BYTES * 2 {
            return None;
        }
        let mut bytes = [0; TOKEN_BYTES];
        for (byte, [high, low]) in bytes.iter_mut().zip(digits.as_chunks::<2>().0) {
            *byte = digit(*high)? * 16 + digit(*low)?;
        }
        Some(Self(bytes))
    }

    /// The token as a cookie holds it.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut text = String::with_capacity(TOKEN_BYTES * 2);
        for byte in self.0 {
            text.push(char::from(HEX[usize::from(byte >> 4)]));
            text.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        text
    }

    /// The SHA-256 hash that stands for the token in storage (ADR-0047).
    pub(crate) fn hash(&self) -> [u8; 32] {
        Sha256::digest(self.0).into()
    }
}

impl std::fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SessionToken(hidden)")
    }
}

/// Fills bytes from a secure random source.
pub(crate) type Fill = fn(&mut [u8]) -> Result<(), getrandom::Error>;

/// One lowercase hexadecimal digit's value. Uppercase is refused, so each token has exactly one written form.
fn digit(character: u8) -> Option<u8> {
    match character {
        b'0'..=b'9' => Some(character - b'0'),
        b'a'..=b'f' => Some(character - b'a' + 10),
        _ => None,
    }
}

fn no_randomness(error: getrandom::Error) -> Error {
    Error::Unavailable(format!("no secure random source: {error}"))
}

/// A signed-in operator's session, as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub(crate) realm: Realm,
    pub(crate) operator: OperatorId,
    pub(crate) started_at: SystemTime,
    pub(crate) last_seen_at: SystemTime,
    pub(crate) ends_at: SystemTime,
}

impl Session {
    #[must_use]
    pub fn realm(&self) -> Realm {
        self.realm
    }

    #[must_use]
    pub fn operator(&self) -> OperatorId {
        self.operator
    }

    #[must_use]
    pub fn started_at(&self) -> SystemTime {
        self.started_at
    }

    #[must_use]
    pub fn last_seen_at(&self) -> SystemTime {
        self.last_seen_at
    }

    #[must_use]
    pub fn ends_at(&self) -> SystemTime {
        self.ends_at
    }

    /// Whether the session still holds at `now`: before its absolute end, and within its realm's idle limit of its
    /// last request. Either limit ends it for good (ADR-0048).
    #[must_use]
    pub fn is_live(&self, now: SystemTime) -> bool {
        now < self.ends_at && now < self.last_seen_at + self.realm.limits().idle
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Invalid(String),
    /// Worth retrying: the connection dropped, or a cached statement went stale (ADR-0011).
    Unavailable(String),
    Storage(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (what, why) = match self {
            Self::Invalid(why) => ("invalid", why),
            Self::Unavailable(why) => ("temporarily unavailable", why),
            Self::Storage(why) => ("storage failed", why),
        };
        write!(f, "{what}: {why}")
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const HOUR: Duration = Duration::from_hours(1);
    const MINUTE: Duration = Duration::from_mins(1);

    fn session(started: SystemTime, last_seen: SystemTime) -> Session {
        Session {
            realm: Realm::Operator,
            operator: OperatorId::new(1),
            started_at: started,
            last_seen_at: last_seen,
            ends_at: started + Realm::Operator.limits().absolute,
        }
    }

    #[test]
    fn operators_get_eight_hours_and_fifteen_idle_minutes() {
        assert_eq!(
            Realm::Operator.limits(),
            Limits {
                absolute: 8 * HOUR,
                idle: 15 * MINUTE
            }
        );
    }

    #[test]
    fn a_realm_reads_back_its_own_name_and_no_other() {
        assert_eq!(
            Realm::parse(Realm::Operator.as_str()),
            Some(Realm::Operator)
        );
        assert_eq!(Realm::parse("Operator"), None);
        assert_eq!(Realm::parse("staff"), None);
    }

    #[test]
    fn a_name_is_trimmed_and_bounded() {
        assert_eq!(
            OperatorName::parse("  Kurt Medley ").map(|name| name.as_str().to_string()),
            Ok("Kurt Medley".to_string())
        );
        assert!(OperatorName::parse(&"é".repeat(100)).is_ok());
        let length = Err(Error::Invalid("a name has 1 to 100 characters".to_string()));
        let control = Err(Error::Invalid(
            "a name has no control characters".to_string(),
        ));
        assert_eq!(OperatorName::parse(""), length);
        assert_eq!(OperatorName::parse("   "), length);
        assert_eq!(OperatorName::parse(&"a".repeat(101)), length);
        assert_eq!(OperatorName::parse("Kurt\nMedley"), control);
        assert_eq!(OperatorName::parse("Kurt\u{7}"), control);
    }

    #[test]
    fn a_token_is_never_printed() {
        let token = SessionToken::generate(getrandom::fill).expect("a token");
        assert_eq!(format!("{token:?}"), "SessionToken(hidden)");
    }

    #[test]
    fn two_tokens_differ() {
        let (first, second) = (
            SessionToken::generate(getrandom::fill).expect("a token"),
            SessionToken::generate(getrandom::fill).expect("a token"),
        );
        assert_ne!(first, second);
        assert_ne!(first.hash(), second.hash());
    }

    #[test]
    fn a_token_hashes_with_sha_256() {
        let token = SessionToken::parse(&"00".repeat(32)).expect("a token");
        assert_eq!(
            token.hash()[..4],
            [0x66, 0x68, 0x7a, 0xad],
            "SHA-256 of 32 zero bytes begins 66687aad"
        );
    }

    #[test]
    fn a_token_has_one_written_form() {
        let text = "0123456789abcdef".repeat(4);
        assert_eq!(
            SessionToken::parse(&text).map(|token| token.to_text()),
            Some(text.clone())
        );
        for bad in [
            text.to_uppercase(),
            text[1..].to_string(),
            format!("{text}0"),
            format!("g{}", &text[1..]),
            format!("0g{}", &text[2..]),
            format!(" {}", &text[1..]),
            String::new(),
        ] {
            assert_eq!(SessionToken::parse(&bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_failed_random_source_is_unavailable() {
        assert_eq!(
            SessionToken::generate(|_| Err(getrandom::Error::UNSUPPORTED)),
            Err(Error::Unavailable(format!(
                "no secure random source: {}",
                getrandom::Error::UNSUPPORTED
            )))
        );
    }

    #[test]
    fn a_session_lives_until_either_limit() {
        let start = SystemTime::UNIX_EPOCH + 1000 * HOUR;
        let fresh = session(start, start);
        assert!(fresh.is_live(start));
        assert!(fresh.is_live(start + 15 * MINUTE - Duration::from_nanos(1)));
        assert!(!fresh.is_live(start + 15 * MINUTE), "idle");
        let busy = session(start, start + 8 * HOUR - MINUTE);
        assert!(busy.is_live(start + 8 * HOUR - Duration::from_nanos(1)));
        assert!(!busy.is_live(start + 8 * HOUR), "absolute");
    }

    #[test]
    fn errors_say_what_failed() {
        assert_eq!(Error::Invalid("x".into()).to_string(), "invalid: x");
        assert_eq!(
            Error::Unavailable("x".into()).to_string(),
            "temporarily unavailable: x"
        );
        assert_eq!(Error::Storage("x".into()).to_string(), "storage failed: x");
    }

    proptest! {
        /// Any 32 bytes write out and read back as themselves.
        #[test]
        fn every_token_reads_back(bytes in any::<[u8; 32]>()) {
            let token = SessionToken(bytes);
            prop_assert_eq!(SessionToken::parse(&token.to_text()), Some(token));
        }

        /// Nothing but 64 lowercase hexadecimal digits is a token.
        #[test]
        fn nothing_else_is_a_token(text in "\\PC{0,80}") {
            prop_assert_eq!(SessionToken::parse(&text).is_some(), well_formed(&text));
        }

        /// Among strings of the right length, one character outside the lowercase digits is enough to refuse it.
        #[test]
        fn a_near_miss_is_not_a_token(text in "[0-9a-fA-G]{64}") {
            prop_assert_eq!(SessionToken::parse(&text).is_some(), well_formed(&text));
        }

        /// A session is live exactly while both limits hold, wherever its last request fell.
        #[test]
        fn liveness_is_both_limits(idle_for in 0u64..2_000, seen_after in 0u64..40_000) {
            let start = SystemTime::UNIX_EPOCH + 1000 * HOUR;
            let last_seen = start + Duration::from_secs(seen_after);
            let now = last_seen + Duration::from_secs(idle_for);
            let live = session(start, last_seen).is_live(now);
            let expected = now < start + 8 * HOUR && idle_for < 15 * 60;
            prop_assert_eq!(live, expected);
        }
    }

    fn well_formed(text: &str) -> bool {
        text.chars().all(|c| "0123456789abcdef".contains(c)) && text.len() == 64
    }
}
