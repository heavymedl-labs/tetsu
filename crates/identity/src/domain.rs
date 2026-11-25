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

/// Which kind of person logs in. Each realm has its own host, passkeys, cookie, and session limits (ADR-0040,
/// ADR-0048): admins, who work for Tetsu, and staff, who work for businesses (ADR-0089).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Realm {
    Admin,
    Staff,
}

impl Realm {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Staff => "staff",
        }
    }

    /// The realm a stored name names, if any.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "admin" => Some(Self::Admin),
            "staff" => Some(Self::Staff),
            _ => None,
        }
    }

    /// The most passkeys an account in this realm may hold, as `identity.passkey_cap` enforces: two for an admin,
    /// whose keys are hardware, and ten for staff, who may keep one on each device they use.
    #[must_use]
    pub const fn passkey_cap(self) -> usize {
        match self {
            Self::Admin => 2,
            Self::Staff => 10,
        }
    }

    /// How long a session lasts in this realm, within NIST SP 800-63B-4's Level 2 ceiling (ADR-0048).
    #[must_use]
    pub fn limits(self) -> Limits {
        match self {
            Self::Admin => Limits {
                absolute: Duration::from_hours(8),
                idle: Duration::from_mins(15),
            },
            Self::Staff => Limits {
                absolute: Duration::from_hours(12),
                idle: Duration::from_hours(1),
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

/// An account, as Postgres numbers them in its realm's own table (ADR-0088), so an ID means nothing without its realm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountId(i64);

impl AccountId {
    pub(crate) fn new(id: i64) -> Self {
        Self(id)
    }

    #[must_use]
    pub fn get(self) -> i64 {
        self.0
    }

    /// An ID as the admin command takes one: a positive whole number, written plainly, and nothing else.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let plain =
            !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) && !text.starts_with('0');
        text.parse().ok().filter(|_| plain).map(Self)
    }
}

/// What a person, an admin or a staff member, is called in Tetsu's own tools and pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonName(String);

impl PersonName {
    /// A name of 1 to 100 characters, trimmed of whitespace at its ends, with no control, invisible,
    /// direction-changing, or blank-looking characters, which could hide or reorder what the name shows, and no
    /// two spaces together, which the admin command's list puts between columns.
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
/// stored (ADR-0047), and it is never printed, so neither a log line nor a stored row can log anyone in.
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
pub(crate) fn to_hex<const N: usize>(bytes: &[u8; N]) -> String {
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
    InviteToken,
    "What an invite link carries, good once: for an hour for an admin, and seven days for staff. The command that \
     issues it prints it only to its own terminal (ADR-0040)."
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

/// An admin whose open invite link a browser presented: enabled, and holding no passkey yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invitee {
    pub admin: AccountId,
    /// The user handle `WebAuthn` names them by: random, so it says nothing about the admin (ADR-0040).
    pub user_handle: [u8; 16],
    pub name: String,
}

/// A ceremony taken from storage to finish: whose it is, the link it redeems, and the state webauthn-rs needs.
/// Printing it leaves out the link's hash.
#[derive(Clone, PartialEq, Eq)]
pub struct TakenCeremony {
    pub admin: AccountId,
    pub link_hash: [u8; 32],
    pub state: String,
}

impl std::fmt::Debug for TakenCeremony {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TakenCeremony")
            .field("admin", &self.admin)
            .finish_non_exhaustive()
    }
}

/// A passkey webauthn-rs has verified, ready to store: its credential ID, the passkey as webauthn-rs keeps it, and
/// the transports the browser listed that `webauthn-rs` names, once each and sorted, a hint for later ceremonies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPasskey {
    pub credential_id: Vec<u8>,
    pub passkey: String,
    pub transports: Vec<String>,
}

/// A passkey an admin holds, as their account lists it: its credential ID, how the browser said it can reach
/// it, and whether it may be, and is, synced to other devices, as the authenticator last signed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldPasskey {
    pub credential: Vec<u8>,
    pub transports: Vec<String>,
    pub backup_eligible: bool,
    pub backup_state: bool,
}

/// A passkey found by the credential ID a login names, with its admin, their user handle, the passkey as
/// `webauthn-rs` keeps it, and whether the admin is disabled, so a refused login names whose it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signer {
    pub credential: Vec<u8>,
    pub admin: AccountId,
    pub user_handle: [u8; 16],
    pub passkey: String,
    pub disabled: bool,
}

/// The session a login starts, under its token's hash, and the one the browser presented, if any, which it ends.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SessionStart {
    pub hash: [u8; 32],
    pub replacing: Option<[u8; 32]>,
}

/// Printing it leaves out both hashes, as a session's own printing does.
impl std::fmt::Debug for SessionStart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionStart")
            .field("replacing", &self.replacing.is_some())
            .finish_non_exhaustive()
    }
}

/// What logging in came to for a passkey that exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Login {
    SignedIn,
    /// The admin was disabled after the passkey was read.
    Disabled,
    /// Another login changed the passkey meanwhile, so this one's counter must not be stored.
    Raced,
}

impl Login {
    /// The answer as Identity's database writes it, or none for one it never writes.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "logged_in" => Some(Self::SignedIn),
            "disabled" => Some(Self::Disabled),
            "raced" => Some(Self::Raced),
            _ => None,
        }
    }
}

/// A rate limit on what a network may try before login (ADR-0072). Its numbers live in the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimit {
    /// Beginning a login, each counted.
    LoginBegin,
    /// Failing to log in: checked before each finish, and counted when one fails.
    LoginFailure,
    /// Beginning or finishing joining, each counted.
    Join,
    /// Recording a request a check refused, each counted, so one network's flood of them is bounded (ADR-0081).
    Refusal,
    /// Recording, once a window, that a network's later refusals go unrecorded (ADR-0081).
    RefusalFlood,
}

impl RateLimit {
    /// The limit's name in the store.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LoginBegin => "login_begin",
            Self::LoginFailure => "login_failure",
            Self::Join => "join",
            Self::Refusal => "refusal",
            Self::RefusalFlood => "refusal_flood",
        }
    }
}

/// A browser's answer to a passkey ceremony: the ceremony's token, and the response its authenticator gave, as
/// JSON. Printing it leaves out the token.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Answer<'a> {
    pub ceremony: &'a str,
    pub response: &'a str,
}

impl std::fmt::Debug for Answer<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Answer")
            .field("response", &self.response)
            .finish_non_exhaustive()
    }
}

/// The account a live session names: the admin, their user handle and name, and the passkeys they hold, as
/// the admin's page, the browser's list of their passkeys, and a registration that must make no second passkey
/// on a device that has one all need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub admin: AccountId,
    pub user_handle: [u8; 16],
    pub name: String,
    /// Where their invites and notices go, if they have an address (ADR-0074); every staff member has one.
    pub email: Option<tetsu_kernel::Address>,
    pub passkeys: Vec<HeldPasskey>,
}

/// An addition taken to be finished: its token's and session's hashes, which storing its passkey uses it up by, its
/// admin, and the state `webauthn-rs` needs. Printing it leaves out both hashes.
#[derive(Clone, PartialEq, Eq)]
pub struct TakenAddition {
    pub token: [u8; 32],
    pub session: [u8; 32],
    pub admin: AccountId,
    pub state: String,
}

impl std::fmt::Debug for TakenAddition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TakenAddition")
            .field("admin", &self.admin)
            .finish_non_exhaustive()
    }
}

/// What adding a passkey came to for an admin who exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Addition {
    Added,
    Disabled,
    /// The session ended, or names another admin.
    SignedOut,
    /// The addition was not taken by this session, or has ended.
    Ended,
    /// The admin holds two passkeys already (ADR-0057).
    Full,
    Oversized,
    Taken,
}

impl Addition {
    /// The answer as Identity's database writes it, or none for one it never writes.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "added" => Some(Self::Added),
            "disabled" => Some(Self::Disabled),
            "logged_out" => Some(Self::SignedOut),
            "ended" => Some(Self::Ended),
            "full" => Some(Self::Full),
            "oversized" => Some(Self::Oversized),
            "taken" => Some(Self::Taken),
            _ => None,
        }
    }

    /// The reason a refusal records, or none for a passkey added.
    pub(crate) fn reason(self) -> Option<&'static str> {
        match self {
            Self::Added => None,
            Self::Disabled => Some("disabled"),
            Self::SignedOut => Some("logged_out"),
            Self::Ended => Some("ended"),
            Self::Full => Some("full"),
            Self::Oversized => Some("oversized"),
            Self::Taken => Some("taken"),
        }
    }
}

/// What redeeming an invite link came to for an admin who exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Redemption {
    /// The passkey is stored and the link used up.
    Redeemed,
    Disabled,
    /// The admin holds a passkey already, as when two tabs finish the same link.
    Joined,
    /// The link ended, or was replaced, before the ceremony finished.
    Expired,
    /// Another admin's passkey has this credential ID.
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
            "joined" => Some(Self::Joined),
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
            Self::Joined => Some("joined"),
            Self::Expired => Some("expired"),
            Self::Taken => Some("taken"),
            Self::Oversized => Some("oversized"),
        }
    }
}

/// A coarse name for the device a request came from, such as `Firefox on Linux`: the browser's family and the system
/// it runs on, read from the `User-Agent`, never a version, so the audit record and a list of sessions can say which
/// device without keeping a fingerprint (ADR-0059). It holds only fixed names, so it always shows safely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Device {
    browser: &'static str,
    system: &'static str,
}

impl Device {
    /// The device a `User-Agent` names, if the request sent one. Either part it cannot tell reads `another
    /// browser` or `another system`.
    #[must_use]
    pub fn from_user_agent(user_agent: Option<&str>) -> Option<Self> {
        // Order matters: Edge and Opera name Chrome too, Chrome names Safari, and Android names Linux.
        const BROWSERS: [(&str, &str); 5] = [
            ("Edg/", "Edge"),
            ("OPR/", "Opera"),
            ("Firefox/", "Firefox"),
            ("Chrome/", "Chrome"),
            ("Safari/", "Safari"),
        ];
        const SYSTEMS: [(&str, &str); 7] = [
            ("iPhone", "iOS"),
            ("iPad", "iPadOS"),
            ("Android", "Android"),
            ("CrOS", "ChromeOS"),
            ("Windows", "Windows"),
            ("Mac OS X", "macOS"),
            ("Linux", "Linux"),
        ];
        let agent = user_agent?;
        let first = |names: &[(&str, &'static str)], other: &'static str| {
            names
                .iter()
                .find(|(mark, _)| agent.contains(mark))
                .map_or(other, |(_, name)| name)
        };
        Some(Self {
            browser: first(&BROWSERS, "another browser"),
            system: first(&SYSTEMS, "another system"),
        })
    }
}

impl std::fmt::Display for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} on {}", self.browser, self.system)
    }
}

/// The kind of ceremony a refusal ends, which names the event that records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CeremonyKind {
    Join,
    Login,
    Addition,
}

/// A ceremony refused after it was taken but before storage was asked to store its outcome: its kind, its reason as
/// the audit record names it, and, where known, the admin and credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refusal<'a> {
    pub kind: CeremonyKind,
    pub admin: Option<AccountId>,
    pub credential: Option<&'a [u8]>,
    pub reason: &'a str,
}

/// Who ran an admin command, as the audit record names them, such as `kurt@workstation`. It is asserted by the
/// command, not proven: every command logs in as one database login (ADR-0061), so database access is the trust
/// boundary, and this says which person used it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    text: String,
    /// The device a browser acted from, kept whole, so a list of sessions names it no finer than it is.
    device: Option<Device>,
}

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
            Some((user, host)) if actor_part(user) && actor_part(host) => Ok(Self {
                text: text.to_string(),
                device: None,
            }),
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
        Self {
            text: format!("{}@{}", part(user), part(host)),
            device: None,
        }
    }

    /// Who acted from a browser, as the audit record names them: `<realm>-<id>@<device>` once logged in, such as
    /// `admin-7@Firefox on Linux`, or `browser@<device>` before, where a device the request did not name reads
    /// `unknown`, as for a command.
    #[must_use]
    pub fn browser(realm: Realm, account: Option<AccountId>, device: Option<Device>) -> Self {
        let user = account.map_or_else(
            || "browser".to_string(),
            |id| format!("{}-{}", realm.as_str(), id.get()),
        );
        let named = device.map_or_else(|| "unknown".to_string(), |device| device.to_string());
        Self {
            text: format!("{user}@{named}"),
            device,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The device a browser's actor acted from, as [`Self::browser`] was given it: none for a command, or a request
    /// that named none, whatever its text says.
    #[must_use]
    pub fn device(&self) -> Option<Device> {
        self.device
    }
}

/// What ending an admin's other sessions did (ADR-0059, `v5.0.0-7.5.2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    /// So many of the admin's other sessions were live when they ended.
    Ended(u64),
    /// The session had not been proven by a passkey within ten minutes, so it may end no other (ADR-0048).
    NotFresh,
    /// The request passed the checks, but its session had already ended or never was, or its admin was disabled.
    NoSession,
    /// The request failed a check before reaching storage.
    Refused(FailedCheck),
}

/// The check a state-changing request failed before it reached storage, as the audit record names it
/// (`v5.0.0-16.3.3`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailedCheck {
    /// It carried no session cookie for its realm, one that is not a token, or the cookie twice.
    Cookie,
    /// It came from an origin the realm does not serve, or a browser said it came from another site.
    Origin,
    /// It carried its session's cookie but not that session's form token.
    FormToken,
    /// A field it sent is in no form the field takes.
    Field,
}

impl FailedCheck {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cookie => "cookie",
            Self::Origin => "origin",
            Self::FormToken => "form_token",
            Self::Field => "field",
        }
    }
}

/// A session's random public id, as the list of sessions shows it and ending one names it (ADR-0059): a UUID in its
/// lowercase form, never anything of its token. One that is not is refused before it reaches storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionId(pub(crate) String);

impl SessionId {
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let groups = text.split('-').map(str::len);
        let hex = text
            .bytes()
            .all(|byte| byte == b'-' || matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
        (hex && groups.eq([8, 4, 4, 4, 12])).then(|| Self(text.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One of an admin's live sessions, as their page lists it (ADR-0059, `v5.0.0-7.5.2`): its random id, when it
/// started and was last used, the device it started on, and whether it is the session asking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedSession {
    pub id: SessionId,
    pub started_at: SystemTime,
    pub last_seen_at: SystemTime,
    pub device: Option<String>,
    pub presented: bool,
}

/// What disabling an admin did: whether it acted, since an admin already disabled is left as is, and how
/// many sessions and invite links it ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Disabling {
    pub done: bool,
    pub sessions_ended: u64,
    pub links_ended: u64,
}

/// What removing an admin's passkeys did: how many it removed, and how many live sessions it ended with them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Removal {
    pub passkeys_removed: u64,
    pub sessions_ended: u64,
}

/// An admin as the admin command lists them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminSummary {
    pub id: AccountId,
    pub name: String,
    pub created_at: SystemTime,
    /// When the admin was disabled, if they were.
    pub disabled_at: Option<SystemTime>,
    /// How many passkeys they hold.
    pub passkeys: u64,
    /// The whole minutes, rounded up, their open invite link has left, if one is open.
    pub link_minutes: Option<u64>,
}

/// What issuing an invite link came to for an admin who exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Issue {
    Issued,
    /// The admin is disabled, and is issued no link.
    Disabled,
    /// The admin holds a passkey already, and adds another only after logging in (`v5.0.0-6.4.1`).
    Joined,
}

impl Issue {
    /// The answer as Identity's database writes it, or none for one it never writes.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "issued" => Some(Self::Issued),
            "disabled" => Some(Self::Disabled),
            "joined" => Some(Self::Joined),
            _ => None,
        }
    }
}

/// A business, by the ID the composition root resolves from its address; Identity keeps its staff's roles and
/// invites under it (ADR-0042).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusinessId(i64);

impl BusinessId {
    #[must_use]
    pub const fn new(id: i64) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

/// What a staff member may do at a business (ADR-0048). Only owners exist until sub-slice 4 adds the other roles
/// and their checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Owner,
}

impl Role {
    /// The role by the name the staff command and Identity's database write.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "owner" => Some(Self::Owner),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
        }
    }
}

/// What issuing a staff invite came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaffIssue {
    /// Issued to this staff member, made now if `created`; `renamed` when they keep a stored name other than the one
    /// given, which only they change (ADR-0040).
    Issued {
        staff: AccountId,
        created: bool,
        renamed: bool,
    },
    /// The staff member is disabled, and is invited nowhere.
    Disabled,
    /// The staff member holds a role at the business already.
    Member,
}

/// A staff invite to issue: to which business, whom, by email, and as what. A name is used only for someone new,
/// since only a staff member changes their own (ADR-0040).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewStaffInvite {
    pub business: BusinessId,
    pub name: PersonName,
    pub email: tetsu_kernel::Address,
    pub role: Role,
}

/// A staff invite issued: its link's token, and who it invites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaffInvite {
    pub token: InviteToken,
    pub staff: AccountId,
    /// The staff member was made by this invite.
    pub created: bool,
    /// They keep a stored name other than the one given.
    pub renamed: bool,
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

/// How long a live session has left before each of its realm's limits, in whole seconds by the store's clock: the
/// idle limit, which a request renews, and the absolute one, which nothing does (ADR-0048).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeLeft {
    pub idle: i64,
    pub absolute: i64,
}

/// A logged-in admin's session, as stored. Printing it leaves out the hash of its token, which finds it.
#[derive(Clone, PartialEq, Eq)]
pub struct Session {
    /// The hash of the token that found it, so a change made in its name can be checked against it again.
    pub(crate) hash: [u8; 32],
    pub(crate) realm: Realm,
    pub(crate) admin: AccountId,
    pub(crate) started_at: SystemTime,
    /// When a passkey last proved the person in it: at the start of a login, and never for a session joining began,
    /// which rests on the invite link (ADR-0069).
    pub(crate) proven_at: Option<SystemTime>,
    pub(crate) last_seen_at: SystemTime,
    pub(crate) ends_at: SystemTime,
}

impl Session {
    #[must_use]
    pub fn realm(&self) -> Realm {
        self.realm
    }

    #[must_use]
    pub fn admin(&self) -> AccountId {
        self.admin
    }

    #[must_use]
    pub fn started_at(&self) -> SystemTime {
        self.started_at
    }

    #[must_use]
    pub fn proven_at(&self) -> Option<SystemTime> {
        self.proven_at
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

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("realm", &self.realm)
            .field("admin", &self.admin)
            .field("started_at", &self.started_at)
            .field("proven_at", &self.proven_at)
            .field("last_seen_at", &self.last_seen_at)
            .field("ends_at", &self.ends_at)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Invalid(String),
    /// Refused by a rate limit (ADR-0072), worth trying again after this long.
    Limited(std::time::Duration),
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
            Self::Limited(wait) => return write!(f, "limited: try again in {}s", wait.as_secs()),
        };
        write!(f, "{what}: {why}")
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    /// A role reads back from the name it writes, and no other name is a role yet, in any case.
    #[test]
    fn a_role_is_read_only_by_its_own_name() {
        assert_eq!(Role::parse(Role::Owner.as_str()), Some(Role::Owner));
        for other in ["Owner", "manager", "front_desk", "instructor", ""] {
            assert_eq!(Role::parse(other), None, "{other:?}");
        }
    }

    /// Printing a session leaves out the hash of its token, which finds it.
    #[test]
    fn a_printed_session_hides_its_hash() {
        let session = Session {
            hash: [0xab; 32],
            realm: Realm::Admin,
            admin: AccountId::new(1),
            started_at: SystemTime::UNIX_EPOCH,
            proven_at: None,
            last_seen_at: SystemTime::UNIX_EPOCH,
            ends_at: SystemTime::UNIX_EPOCH,
        };
        let printed = format!("{session:?}");
        assert!(
            printed.starts_with("Session { realm: Admin, admin: "),
            "{printed}"
        );
        assert_eq!(
            (printed.contains("hash"), printed.contains("171")),
            (false, false),
            "{printed}"
        );
    }

    /// Printing a session to start leaves out both hashes, saying only whether it replaces one.
    #[test]
    fn a_printed_session_start_hides_its_hashes() {
        let start = SessionStart {
            hash: [0xab; 32],
            replacing: Some([0xcd; 32]),
        };
        assert_eq!(format!("{start:?}"), "SessionStart { replacing: true, .. }");
    }

    /// Printing an answer leaves out its ceremony's token.
    #[test]
    fn a_printed_answer_hides_its_token() {
        let answer = Answer {
            ceremony: "secret",
            response: "{}",
        };
        assert_eq!(format!("{answer:?}"), r#"Answer { response: "{}", .. }"#);
    }

    /// Printing a taken ceremony or addition leaves out the hashes of the tokens that name it.
    #[test]
    fn printed_ceremonies_hide_their_hashes() {
        let printed = [
            format!(
                "{:?}",
                TakenCeremony {
                    admin: AccountId::new(1),
                    link_hash: [0xab; 32],
                    state: String::new(),
                }
            ),
            format!(
                "{:?}",
                TakenAddition {
                    token: [0xab; 32],
                    session: [0xab; 32],
                    admin: AccountId::new(1),
                    state: String::new(),
                }
            ),
        ];
        assert_eq!(
            printed.each_ref().map(|printed| printed.contains("171")),
            [false, false],
            "{printed:?}"
        );
        assert_eq!(
            printed,
            [
                "TakenCeremony { admin: AccountId(1), .. }",
                "TakenAddition { admin: AccountId(1), .. }"
            ]
        );
    }

    /// A browser's actor names the admin once logged in, or a browser before, at its device, and a device the
    /// request did not name reads `unknown`.
    #[test]
    fn a_browsers_actor_names_the_admin_and_device() {
        let linux = Device::from_user_agent(Some("Mozilla/5.0 (X11; Linux x86_64) Firefox/140.0"));
        let named = [
            Actor::browser(Realm::Admin, Some(AccountId::new(3)), linux),
            Actor::browser(Realm::Admin, None, linux),
            Actor::browser(Realm::Admin, Some(AccountId::new(3)), None),
        ];
        assert_eq!(
            named.each_ref().map(Actor::as_str),
            [
                "admin-3@Firefox on Linux",
                "browser@Firefox on Linux",
                "admin-3@unknown"
            ]
        );
    }

    /// A device is named by its browser's family and its system, never a version, and what cannot be told is said
    /// plainly.
    #[test]
    fn a_device_is_named_coarsely() {
        let agents = [
            "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15",
            "Mozilla/5.0 (iPhone; CPU iPhone OS 26_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Mobile/15E148 Safari/604.1",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36 Edg/141.0.0.0",
            "Mozilla/5.0 (Linux; Android 16) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Mobile Safari/537.36",
            "Mozilla/5.0 (X11; CrOS x86_64 16000.0.0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36 OPR/120.0.0.0",
            "Mozilla/5.0 (iPad; CPU OS 26_0 like Mac OS X)",
            "curl/8.15.0",
        ];
        assert_eq!(
            agents.map(|agent| Device::from_user_agent(Some(agent))
                .map(|device| device.to_string())
                .unwrap_or_default()),
            [
                "Firefox on Linux",
                "Safari on macOS",
                "Safari on iOS",
                "Edge on Windows",
                "Chrome on Android",
                "Opera on ChromeOS",
                "another browser on iPadOS",
                "another browser on another system"
            ]
        );
        assert_eq!(Device::from_user_agent(None), None);
    }

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
            assert!(PersonName::parse(hidden).is_err(), "{hidden:?}");
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
            assert!(PersonName::parse(shown).is_ok(), "{shown:?}");
        }
        // Two spaces together would read as a gap between the list's columns.
        assert!(PersonName::parse("Kurt  disabled").is_err());
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

    /// An invite token never prints, so no log line or debug output can join anyone.
    #[test]
    fn an_invite_token_is_hidden_when_printed() {
        let token = InviteToken::generate(getrandom::fill).expect("a token");
        assert_eq!(format!("{token:?}"), "InviteToken(hidden)");
        assert_eq!(token.to_text().len(), 64);
        assert_ne!(token.hash(), [0; 32]);
    }

    #[test]
    fn an_admin_id_is_a_plain_positive_number() {
        assert_eq!(AccountId::parse("42"), Some(AccountId(42)));
        assert_eq!(
            AccountId::parse("9223372036854775807"),
            Some(AccountId(i64::MAX))
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
            assert_eq!(AccountId::parse(bad), None, "{bad:?}");
        }
    }
    use proptest::prelude::*;

    const HOUR: Duration = Duration::from_hours(1);
    const MINUTE: Duration = Duration::from_mins(1);

    #[test]
    fn admins_get_eight_hours_and_fifteen_idle_minutes() {
        assert_eq!(
            Realm::Admin.limits(),
            Limits {
                absolute: 8 * HOUR,
                idle: 15 * MINUTE
            }
        );
    }

    #[test]
    fn staff_get_twelve_hours_and_one_idle_hour() {
        assert_eq!(
            Realm::Staff.limits(),
            Limits {
                absolute: 12 * HOUR,
                idle: HOUR
            }
        );
    }

    #[test]
    fn a_realm_reads_back_its_own_name_and_no_other() {
        for realm in [Realm::Admin, Realm::Staff] {
            assert_eq!(Realm::parse(realm.as_str()), Some(realm));
        }
        assert_eq!(
            [Realm::Admin.as_str(), Realm::Staff.as_str()],
            ["admin", "staff"]
        );
        for other in ["Admin", "Staff", "customer", ""] {
            assert_eq!(Realm::parse(other), None, "{other}");
        }
    }

    #[test]
    fn a_name_is_trimmed_and_bounded() {
        assert_eq!(
            PersonName::parse("  Kurt Medley ").map(|name| name.as_str().to_string()),
            Ok("Kurt Medley".to_string())
        );
        assert!(PersonName::parse(&"é".repeat(100)).is_ok());
        let length = Err(Error::Invalid("a name has 1 to 100 characters".to_string()));
        let control = Err(Error::Invalid(
            "a name has no control, invisible, or direction-changing characters, and single plain spaces".to_string(),
        ));
        assert_eq!(PersonName::parse(""), length);
        assert_eq!(PersonName::parse("   "), length);
        assert_eq!(PersonName::parse(&"a".repeat(101)), length);
        assert_eq!(PersonName::parse("Kurt\nMedley"), control);
        assert_eq!(PersonName::parse("Kurt\u{7}"), control);
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
        assert_eq!(
            Error::Limited(std::time::Duration::from_secs(90)).to_string(),
            "limited: try again in 90s"
        );
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
