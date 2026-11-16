//! Identity's values. Fields are private, so every value passes its checks when it is made, and a session token is
//! never printed.

use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

const MAX_NAME: usize = 100;
/// 256 random bits (ADR-0048).
const TOKEN_BYTES: usize = 32;
const HEX: &[u8; 16] = b"0123456789abcdef";
/// Sets a form token apart from every other value made from a session token.
const FORM_TOKEN: &[u8] = b"tetsu form token\0";

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

    /// An ID as the operator command takes one: a positive whole number, written plainly, and nothing else.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let plain =
            !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) && !text.starts_with('0');
        text.parse().ok().filter(|_| plain).map(Self)
    }
}

/// What an operator is called in Tetsu's own tools and the audit record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorName(String);

impl OperatorName {
    /// A name of 1 to 100 characters, trimmed of whitespace at its ends, with no control, invisible,
    /// direction-changing, or blank-looking characters, which could hide or reorder what the name shows, and no
    /// two spaces together, which the operator command's list puts between columns.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`] for an empty or overlong name, or one holding such a character.
    pub fn parse(name: &str) -> Result<Self, Error> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > MAX_NAME {
            return Err(Error::Invalid(format!(
                "a name has 1 to {MAX_NAME} characters"
            )));
        }
        if name
            .chars()
            .any(|c| c != ' ' && (c.is_whitespace() || hidden(c)))
            || name.contains("  ")
        {
            return Err(Error::Invalid(
                "a name has no control, invisible, or direction-changing characters, and single plain spaces"
                    .to_string(),
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
#[derive(Clone)]
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
        from_hex(text).map(Self)
    }

    /// The token as a cookie holds it.
    #[must_use]
    pub fn to_text(&self) -> String {
        to_hex(&self.0)
    }

    /// The token this session's forms carry, which every state-changing request must send back (ADR-0048,
    /// `v5.0.0-3.5.1`). It comes from the session's own token, so nothing more is stored, and no page can know it
    /// without the cookie, which scripts cannot read. It differs from the stored hash, so the database never holds
    /// it either. Each call masks it with a fresh random pad, as `pad || (pad XOR token)`, so it reads differently
    /// in every page, and a compressed page never repeats it for an attacker to measure (BREACH).
    ///
    /// # Errors
    /// Fails with [`Error::Unavailable`] if the secure random source cannot be read.
    pub fn form_token(&self) -> Result<String, Error> {
        self.masked_form_token(getrandom::fill)
    }

    pub(crate) fn masked_form_token(&self, fill: Fill) -> Result<String, Error> {
        let mut pad = [0; TOKEN_BYTES];
        fill(&mut pad).map_err(no_randomness)?;
        let mut masked = [0; TOKEN_BYTES * 2];
        masked[..TOKEN_BYTES].copy_from_slice(&pad);
        for (out, (pad, secret)) in masked[TOKEN_BYTES..]
            .iter_mut()
            .zip(pad.iter().zip(self.form_bytes()))
        {
            *out = pad ^ secret;
        }
        Ok(to_hex(&masked))
    }

    /// Whether a submitted form token is this session's, however it was masked: it is unmasked, then compared in
    /// constant time, so the comparison's timing tells nothing about how much of a guess was right.
    #[must_use]
    pub fn form_token_matches(&self, submitted: &str) -> bool {
        from_hex::<{ TOKEN_BYTES * 2 }>(submitted).is_some_and(|bytes| {
            let (pad, masked) = bytes.split_at(TOKEN_BYTES);
            let mut unmasked = [0; TOKEN_BYTES];
            for (out, (pad, masked)) in unmasked.iter_mut().zip(pad.iter().zip(masked)) {
                *out = pad ^ masked;
            }
            bool::from(unmasked.ct_eq(&self.form_bytes()))
        })
    }

    fn form_bytes(&self) -> [u8; TOKEN_BYTES] {
        Sha256::new()
            .chain_update(FORM_TOKEN)
            .chain_update(self.0)
            .finalize()
            .into()
    }

    /// The SHA-256 hash that stands for the token in storage (ADR-0047).
    pub(crate) fn hash(&self) -> [u8; 32] {
        Sha256::digest(self.0).into()
    }
}

/// Equal in constant time, through `subtle`, so comparing a token tells nothing about how much of it matched.
impl PartialEq for SessionToken {
    fn eq(&self, other: &Self) -> bool {
        self.0.ct_eq(&other.0).into()
    }
}

impl Eq for SessionToken {}

impl std::fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SessionToken(hidden)")
    }
}

/// Bytes as lowercase hexadecimal digits.
fn to_hex<const N: usize>(bytes: &[u8; N]) -> String {
    let mut text = String::with_capacity(N * 2);
    for byte in bytes {
        text.push(char::from(HEX[usize::from(byte >> 4)]));
        text.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    text
}

/// The `N` bytes that `2 * N` lowercase hexadecimal digits stand for, and nothing else, so each value has one written
/// form.
fn from_hex<const N: usize>(text: &str) -> Option<[u8; N]> {
    let digits = text.as_bytes();
    if digits.len() != N * 2 {
        return None;
    }
    let mut bytes = [0; N];
    for (byte, [high, low]) in bytes.iter_mut().zip(digits.as_chunks::<2>().0) {
        *byte = digit(*high)? * 16 + digit(*low)?;
    }
    Some(bytes)
}

/// A token of 256 random bits, like a session token, of which only the hash is stored, and whose `Debug` hides it.
macro_rules! secret_token {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, PartialEq, Eq)]
        pub struct $name(SessionToken);

        impl $name {
            pub(crate) fn generate(fill: Fill) -> Result<Self, Error> {
                SessionToken::generate(fill).map(Self)
            }

            /// The token in the form [`Self::to_text`] writes, and nothing else.
            #[must_use]
            pub fn parse(text: &str) -> Option<Self> {
                SessionToken::parse(text).map(Self)
            }

            /// The token as 64 lowercase hexadecimal digits.
            #[must_use]
            pub fn to_text(&self) -> String {
                self.0.to_text()
            }

            pub(crate) fn hash(&self) -> [u8; 32] {
                self.0.hash()
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(concat!(stringify!($name), "(hidden)"))
            }
        }
    };
}

// The doc text is a literal, not attributes the macro repeats, which lizard reads as one long function.
secret_token!(
    EnrollmentToken,
    "What an enrollment link carries, good once and for an hour. The operator command prints it only to the \
     terminal that ran it (ADR-0040)."
);

secret_token!(
    CeremonyToken,
    "What a browser holds between a passkey ceremony's two halves, good once and for ten minutes. The \
     ceremony's state stays in Postgres under its hash, and never reaches the browser (ADR-0047)."
);

/// A passkey ceremony begun: the token the browser keeps until it finishes, and the options it hands its
/// authenticator, as JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ceremony {
    pub token: CeremonyToken,
    pub options: String,
}

/// An operator whose open enrollment link a browser presented: enabled, and holding no passkey yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enrollee {
    pub operator: OperatorId,
    /// The user handle `WebAuthn` names them by: random, so it says nothing about the operator (ADR-0040).
    pub user_handle: [u8; 16],
    pub name: String,
}

/// A ceremony taken from storage to finish: whose it is, the link it redeems, and the state webauthn-rs needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TakenCeremony {
    pub operator: OperatorId,
    pub link_hash: [u8; 32],
    pub state: String,
}

/// A passkey webauthn-rs has verified, ready to store: its credential ID, and the passkey as webauthn-rs keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPasskey {
    pub credential_id: Vec<u8>,
    pub passkey: String,
}

/// What redeeming an enrollment link came to for an operator who exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Redemption {
    /// The passkey is stored and the link used up.
    Redeemed,
    Disabled,
    /// The operator holds a passkey already, as when two tabs finish the same link.
    Enrolled,
    /// The link ended, or was replaced, before the ceremony finished.
    Expired,
    /// Another operator's passkey has this credential ID.
    Taken,
    /// The credential ID or passkey is larger, or the credential ID smaller, than the store keeps.
    Oversized,
}

impl Redemption {
    /// The answer as Identity's database writes it, or none for one it never writes.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "redeemed" => Some(Self::Redeemed),
            "disabled" => Some(Self::Disabled),
            "enrolled" => Some(Self::Enrolled),
            "expired" => Some(Self::Expired),
            "taken" => Some(Self::Taken),
            "oversized" => Some(Self::Oversized),
            _ => None,
        }
    }

    /// The reason a refusal records, or none for a passkey stored.
    pub(crate) fn reason(self) -> Option<&'static str> {
        match self {
            Self::Redeemed => None,
            Self::Disabled => Some("disabled"),
            Self::Enrolled => Some("enrolled"),
            Self::Expired => Some("expired"),
            Self::Taken => Some("taken"),
            Self::Oversized => Some("oversized"),
        }
    }
}

/// Who ran an operator command, as the audit record names them, such as `kurt@workstation`. It is asserted by the
/// command, not proven: every command signs in as one database login (ADR-0061), so database access is the trust
/// boundary, and this says which person used it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor(String);

/// The longest user or host in an actor.
const MAX_PART: usize = 99;

/// Whether text can be one part of an actor: 1 to [`MAX_PART`] characters, with no space, nothing hidden, and
/// nothing that reads as an `@`, so no part can pass for another.
fn actor_part(part: &str) -> bool {
    !part.is_empty()
        && part.chars().count() <= MAX_PART
        && !part
            .chars()
            .any(|c| matches!(c, '@' | '\u{FE6B}' | '\u{FF20}') || c.is_whitespace() || hidden(c))
}

/// A character that could hide or reorder text where it is shown: a control character, one Unicode says a
/// display may ignore or that only formats, one that shows as blank, or one with no agreed look. The list follows
/// Unicode 16's `Default_Ignorable_Code_Point` and `Cf` sets, plus blank-looking characters such as the Hangul
/// and Khitan fillers, the object replacement character, private use, and noncharacters. Unassigned code points
/// are not refused, since telling them apart needs Unicode's tables.
fn hidden(c: char) -> bool {
    const INVISIBLE: [(char, char); 31] = [
        ('\u{00AD}', '\u{00AD}'),
        ('\u{034F}', '\u{034F}'),
        ('\u{0600}', '\u{0605}'),
        ('\u{061C}', '\u{061C}'),
        ('\u{06DD}', '\u{06DD}'),
        ('\u{070F}', '\u{070F}'),
        ('\u{0890}', '\u{0891}'),
        ('\u{08E2}', '\u{08E2}'),
        ('\u{115F}', '\u{1160}'),
        ('\u{17B4}', '\u{17B5}'),
        ('\u{180B}', '\u{180F}'),
        ('\u{200B}', '\u{200F}'),
        ('\u{2028}', '\u{202E}'),
        ('\u{2060}', '\u{206F}'),
        ('\u{2800}', '\u{2800}'),
        ('\u{E000}', '\u{F8FF}'),
        ('\u{FDD0}', '\u{FDEF}'),
        ('\u{3164}', '\u{3164}'),
        ('\u{FE00}', '\u{FE0F}'),
        ('\u{FEFF}', '\u{FEFF}'),
        ('\u{FFA0}', '\u{FFA0}'),
        ('\u{FFF0}', '\u{FFFC}'),
        ('\u{110BD}', '\u{110BD}'),
        ('\u{110CD}', '\u{110CD}'),
        ('\u{13430}', '\u{1343F}'),
        ('\u{16FE4}', '\u{16FE4}'),
        ('\u{1BCA0}', '\u{1BCA3}'),
        ('\u{1D159}', '\u{1D159}'),
        ('\u{1D173}', '\u{1D17A}'),
        ('\u{E0000}', '\u{E0FFF}'),
        ('\u{F0000}', '\u{10FFFF}'),
    ];
    // Each plane's last two code points are noncharacters.
    c.is_control()
        || u32::from(c) & 0xFFFE == 0xFFFE
        || INVISIBLE
            .iter()
            .any(|&(first, last)| (first..=last).contains(&c))
}

impl Actor {
    /// # Errors
    /// Fails unless the text is `user@host`, each part 1 to 99 characters with no space at either end and no
    /// control, invisible, or direction-changing character, and no other `@`.
    pub fn parse(text: &str) -> Result<Self, Error> {
        match text.split_once('@') {
            Some((user, host)) if actor_part(user) && actor_part(host) => {
                Ok(Self(text.to_string()))
            }
            _ => Err(Error::Invalid(format!(
                "an actor is user@host, each part 1 to {MAX_PART} characters with no control, invisible, or \
                 direction-changing characters"
            ))),
        }
    }

    /// The actor `user@host` for a login and a machine, either of which may be missing. A part that is missing,
    /// blank, overlong, or holds a hidden character or an `@` reads `unknown`, so naming the actor never stops a
    /// command, and no part can pass for another.
    #[must_use]
    pub fn from_login(user: Option<&str>, host: Option<&str>) -> Self {
        let part = |part: Option<&str>| {
            part.map(str::trim)
                .filter(|part| actor_part(part))
                .unwrap_or("unknown")
                .to_string()
        };
        Self(format!("{}@{}", part(user), part(host)))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What disabling an operator did: whether it acted, since an operator already disabled is left as is, and how
/// many sessions and enrollment links it ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Disabling {
    pub done: bool,
    pub sessions_ended: u64,
    pub links_ended: u64,
}

/// What removing an operator's passkeys did: how many it removed, and how many live sessions it ended with them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Removal {
    pub passkeys_removed: u64,
    pub sessions_ended: u64,
}

/// An operator as the operator command lists them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorSummary {
    pub id: OperatorId,
    pub name: String,
    pub created_at: SystemTime,
    /// When the operator was disabled, if they were.
    pub disabled_at: Option<SystemTime>,
    /// How many passkeys they hold.
    pub passkeys: u64,
    /// The whole minutes, rounded up, their open enrollment link has left, if one is open.
    pub link_minutes: Option<u64>,
}

/// What issuing an enrollment link came to for an operator who exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Issue {
    Issued,
    /// The operator is disabled, and is issued no link.
    Disabled,
    /// The operator holds a passkey already, and adds another only after signing in (`v5.0.0-6.4.1`).
    Enrolled,
}

impl Issue {
    /// The answer as Identity's database writes it, or none for one it never writes.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "issued" => Some(Self::Issued),
            "disabled" => Some(Self::Disabled),
            "enrolled" => Some(Self::Enrolled),
            _ => None,
        }
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

    #[test]
    fn an_actor_is_a_user_at_a_host() {
        assert_eq!(
            Actor::parse("kurt@workstation").map(|actor| actor.as_str().to_string()),
            Ok("kurt@workstation".into())
        );
        let longest = format!("{}@{}", "é".repeat(99), "é".repeat(99));
        assert!(Actor::parse(&longest).is_ok());
        for bad in [
            String::new(),
            "kurt".into(),
            "@host".into(),
            "kurt@".into(),
            " kurt@host".into(),
            "kurt@host ".into(),
            "kurt@a@b".into(),
            "kurt\u{FF20}a@b".into(),
            "kurt\u{FE6B}a@b".into(),
            format!("{}@b", "é".repeat(100)),
            format!("a@{}", "é".repeat(100)),
            "kurt\nforged@host".into(),
            "root kurt@host".into(),
            "admin\u{2003}(approved)@host".into(),
        ] {
            assert!(Actor::parse(&bad).is_err(), "{bad:?}");
        }
    }

    /// No name or actor holds a character that could hide or reorder what the audit record shows.
    #[test]
    fn names_and_actors_hold_no_hidden_characters() {
        for hidden in [
            "\u{200B}",
            "a\u{202E}b",
            "\u{2066}a",
            "a\u{FEFF}",
            "\u{00AD}",
            "Kurt\u{E0041}",
            "\u{3164}",
            "Kurt\u{034F}",
            "a\u{FE0F}",
            "\u{0600}a",
            "a\u{1D173}",
            "a\u{2800}",
            "\u{16FE4}",
            "a\u{1D159}",
            "a\u{FFFC}",
            "a\u{E000}",
            "a\u{FDD0}",
            "a\u{1FFFE}",
            "a\u{10FFFD}",
            "a\u{3000}b",
            "a\u{00A0}b",
        ] {
            assert!(OperatorName::parse(hidden).is_err(), "{hidden:?}");
            assert!(
                Actor::parse(&format!("{hidden}@host")).is_err(),
                "{hidden:?}"
            );
        }
        // Visible letters and marks beside the ranges stay allowed.
        for shown in [
            "Zoë",
            "Kurt Medley",
            "\u{0606}",
            "\u{1161}",
            "\u{FE10}",
            "\u{FFFD}",
        ] {
            assert!(OperatorName::parse(shown).is_ok(), "{shown:?}");
        }
        // Two spaces together would read as a gap between the list's columns.
        assert!(OperatorName::parse("Kurt  disabled").is_err());
    }

    /// An actor made from a login names `unknown` for any part that is missing or cannot be shown safely, so it
    /// never stops a command, and no part can pass for the host.
    #[test]
    fn an_actor_from_a_login_falls_back_to_unknown() {
        let made = |user, host| Actor::from_login(user, host).as_str().to_string();
        assert_eq!(
            made(Some(" kurt "), Some("workstation")),
            "kurt@workstation"
        );
        assert_eq!(made(None, None), "unknown@unknown");
        let long = "a".repeat(100);
        for bad in [
            "",
            " ",
            "kurt@workstation",
            "kurt\u{FF20}workstation",
            "a\u{202E}b",
            "a\nb",
            long.as_str(),
        ] {
            assert_eq!(made(Some(bad), Some("host")), "unknown@host", "{bad:?}");
            assert_eq!(made(Some("kurt"), Some(bad)), "kurt@unknown", "{bad:?}");
        }
        assert_eq!(
            made(Some(&"a".repeat(99)), Some(&"b".repeat(99))).len(),
            199
        );
    }

    /// An enrollment token never prints, so no log line or debug output can enroll anyone.
    #[test]
    fn an_enrollment_token_is_hidden_when_printed() {
        let token = EnrollmentToken::generate(getrandom::fill).expect("a token");
        assert_eq!(format!("{token:?}"), "EnrollmentToken(hidden)");
        assert_eq!(token.to_text().len(), 64);
        assert_ne!(token.hash(), [0; 32]);
    }

    #[test]
    fn an_operator_id_is_a_plain_positive_number() {
        assert_eq!(OperatorId::parse("42"), Some(OperatorId(42)));
        assert_eq!(
            OperatorId::parse("9223372036854775807"),
            Some(OperatorId(i64::MAX))
        );
        for bad in [
            "",
            "0",
            "042",
            "-1",
            "+1",
            " 1",
            "1 ",
            "1.0",
            "x",
            "9223372036854775808",
        ] {
            assert_eq!(OperatorId::parse(bad), None, "{bad:?}");
        }
    }
    use proptest::prelude::*;

    const HOUR: Duration = Duration::from_hours(1);
    const MINUTE: Duration = Duration::from_mins(1);

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
            "a name has no control, invisible, or direction-changing characters, and single plain spaces".to_string(),
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
    fn a_form_token_is_the_sessions_own_and_not_its_stored_hash() {
        let token = SessionToken::parse(&"00".repeat(32)).expect("a token");
        let form = token.form_token().expect("a form token");
        assert!(token.form_token_matches(&form));
        assert_ne!(form, to_hex(&token.hash()), "the database never holds it");
        assert_ne!(form, token.to_text());
        assert_ne!(
            token.form_bytes(),
            token.hash(),
            "the database never holds it"
        );
        assert!(
            !token.form_token_matches(&to_hex(&token.form_bytes())),
            "only a masked token is accepted"
        );
        let other = SessionToken::generate(getrandom::fill).expect("a token");
        assert!(
            !other.form_token_matches(&form),
            "another session's form token"
        );
        for bad in ["", "not a token", &form.to_uppercase(), &form[1..]] {
            assert!(!token.form_token_matches(bad), "{bad:?}");
        }
    }

    /// Each render masks the token afresh, so no two pages repeat it, and every render still matches.
    #[test]
    fn each_render_of_a_form_token_differs_and_matches() {
        let token = SessionToken::generate(getrandom::fill).expect("a token");
        let (first, second) = (
            token.form_token().expect("a form token"),
            token.form_token().expect("a form token"),
        );
        assert_ne!(first, second);
        assert!(token.form_token_matches(&first));
        assert!(token.form_token_matches(&second));
        assert_eq!(first.len(), 128);
    }

    #[test]
    fn no_form_token_is_made_without_randomness() {
        let token = SessionToken::parse(&"00".repeat(32)).expect("a token");
        assert_eq!(
            token.masked_form_token(|_| Err(getrandom::Error::UNSUPPORTED)),
            Err(Error::Unavailable(format!(
                "no secure random source: {}",
                getrandom::Error::UNSUPPORTED
            )))
        );
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

        /// Every session's form token matches it, and a token one digit off never does.
        #[test]
        fn a_form_token_matches_only_exactly(text in "[0-9a-f]{64}", at in 0usize..128) {
            let token = SessionToken::parse(&text).expect("a token");
            let form = token.form_token().expect("a form token");
            prop_assert!(token.form_token_matches(&form));
            let digit = form.as_bytes()[at];
            let changed = if digit == b'0' { "1" } else { "0" };
            let near = format!("{}{changed}{}", &form[..at], &form[at + 1..]);
            prop_assert!(!token.form_token_matches(&near));
        }
    }

    /// Both checks run on every input, with no `&&` to skip the second: the inputs are random, so a short circuit
    /// would leave its right side covered only on runs that happen to draw a well-formed token (#138).
    fn well_formed(text: &str) -> bool {
        [
            text.len() == 64,
            text.chars().all(|c| "0123456789abcdef".contains(c)),
        ] == [true, true]
    }
}
