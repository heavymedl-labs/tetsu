//! Tenant values. Fields are private, so every value passes its checks when it is made.

const MAX_SLUG: usize = 63;

/// Names no tenant may hold, since a tenant is reached at `/<slug>` and Tetsu's own pages need these, now or
/// later (#90). Reserved generously before real tenants exist: a name reserved later hides the tenant holding it,
/// which then fails to load rather than vanishing. The list doubles as the subdomain reserve if #8 picks them.
const RESERVED: &[&str] = &[
    // Routes Tetsu serves or plans.
    "account",
    "admin",
    "api",
    "app",
    "assets",
    "auth",
    "book",
    "booking",
    "checkout",
    "dashboard",
    "embed",
    "files",
    "health",
    "join",
    "login",
    "logout",
    "mailbox",
    "media",
    "oauth",
    "ops",
    "passkeys",
    "pay",
    "register",
    "session",
    "sessions",
    "settings",
    "signin",
    "signout",
    "signup",
    "staff",
    "static",
    "webhooks",
    // Tetsu's own pages, and names a subdomain or a person would expect to be Tetsu's.
    "about",
    "blog",
    "contact",
    "docs",
    "help",
    "home",
    "legal",
    "mail",
    "pricing",
    "privacy",
    "security",
    "status",
    "support",
    "tetsu",
    "terms",
    "www",
];
const MAX_NAME: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Invalid(String),
    SlugTaken,
    /// Another tenant already holds the address, though the slug itself is free.
    AddressTaken,
    NotFound,
    /// Worth retrying: the connection dropped, or a cached statement went stale (ADR-0011).
    Unavailable(String),
    Storage(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(why) => write!(f, "invalid: {why}"),
            Self::SlugTaken => f.write_str("that slug is taken"),
            Self::AddressTaken => f.write_str("that address is taken"),
            Self::NotFound => f.write_str("no tenant at that address"),
            Self::Unavailable(why) => write!(f, "temporarily unavailable: {why}"),
            Self::Storage(why) => write!(f, "storage failed: {why}"),
        }
    }
}

impl std::error::Error for Error {}

/// Checks text as given, before trimming, so a line break at either end is refused rather than trimmed away.
/// It refuses control characters, such as a NUL that Postgres cannot store, and characters that show nothing:
/// Unicode's default ignorables (soft hyphen, fillers, tags, bidi marks and controls, zero-width spaces) and
/// line and paragraph separators. Joiners and variation selectors stay, since emoji and scripts such as Persian
/// need them, but text that is not blank must show at least one other character. The braille blank counts as
/// blank.
fn plain(what: &str, raw: &str) -> Result<(), Error> {
    if raw.chars().any(|c| c.is_control() || hidden(c)) {
        return Err(Error::Invalid(format!(
            "{what} {raw:?} must not hold control characters or invisible formatting"
        )));
    }
    let trimmed = raw.trim();
    let shows = |c: char| !c.is_whitespace() && !joiner(c) && !format(c) && c != '\u{2800}';
    if !trimmed.is_empty() && !trimmed.chars().any(shows) {
        return Err(Error::Invalid(format!(
            "{what} {raw:?} must hold a visible character"
        )));
    }
    Ok(())
}

/// Shows nothing and joins nothing: a default ignorable code point (Unicode's `DerivedCoreProperties`) other
/// than a joiner, or a line or paragraph separator.
fn hidden(c: char) -> bool {
    let ignorable = matches!(
        c,
        '\u{00AD}'
            | '\u{034F}'
            | '\u{061C}'
            | '\u{115F}'..='\u{1160}'
            | '\u{17B4}'..='\u{17B5}'
            | '\u{180B}'..='\u{180F}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{3164}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FEFF}'
            | '\u{FFA0}'
            | '\u{FFF0}'..='\u{FFF8}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0000}'..='\u{E0FFF}'
    );
    (ignorable && !joiner(c)) || matches!(c, '\u{2028}' | '\u{2029}')
}

/// Format characters that are not default ignorable yet show nothing alone: number signs that mark the digits
/// after them, the Syriac abbreviation mark, interlinear annotation marks, and Egyptian hieroglyph controls.
/// Text may hold them, beside something visible.
fn format(c: char) -> bool {
    matches!(
        c,
        '\u{0600}'..='\u{0605}'
            | '\u{06DD}'
            | '\u{070F}'
            | '\u{0890}'..='\u{0891}'
            | '\u{08E2}'
            | '\u{110BD}'
            | '\u{110CD}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{13430}'..='\u{1343F}'
    )
}

/// Zero-width joiners and variation selectors, Mongolian ones included, which shape the characters beside them.
fn joiner(c: char) -> bool {
    matches!(
        c,
        '\u{200C}'
            | '\u{200D}'
            | '\u{180B}'..='\u{180D}'
            | '\u{180F}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{E0100}'..='\u{E01EF}'
    )
}

/// Assigned by storage, never by a caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TenantId(i64);

impl TenantId {
    pub(crate) fn new(id: i64) -> Self {
        Self(id)
    }

    #[must_use]
    pub fn get(self) -> i64 {
        self.0
    }
}

/// The address of each reserved name, which no tenant may hold.
pub(crate) fn reserved_addresses() -> Vec<String> {
    RESERVED.iter().map(|name| format!("/{name}")).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slug(String);

impl Slug {
    /// # Errors
    /// Fails unless the slug is 1 to 63 lowercase letters, digits, and inner hyphens, and not a name reserved for
    /// Tetsu's own pages.
    pub fn parse(value: &str) -> Result<Self, Error> {
        if RESERVED.contains(&value) {
            return Err(Error::Invalid(format!(
                "slug {value:?} is reserved for Tetsu's own pages"
            )));
        }
        let allowed = value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        let well_formed = !value.starts_with('-') && !value.ends_with('-') && !value.contains("--");
        if (1..=MAX_SLUG).contains(&value.len()) && allowed && well_formed {
            Ok(Self(value.to_string()))
        } else {
            Err(Error::Invalid(format!(
                "slug {value:?} must be 1 to {MAX_SLUG} lowercase letters, digits, and inner hyphens"
            )))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The path that reaches this tenant. It starts with `/`, so it can never match a host name.
    #[must_use]
    pub fn address(&self) -> String {
        format!("/{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Color(String);

impl Color {
    /// # Errors
    /// Fails unless the color is `#RRGGBB`.
    pub fn parse(value: &str) -> Result<Self, Error> {
        match value.strip_prefix('#') {
            Some(hex) if hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()) => {
                Ok(Self(format!("#{}", hex.to_ascii_uppercase())))
            }
            _ => Err(Error::Invalid(format!("color {value:?} must be #RRGGBB"))),
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub primary: Color,
    pub accent: Color,
}

/// A place the business operates. Time zones are stored as given; #21 decides how they are handled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    name: String,
    time_zone: String,
}

impl Location {
    /// # Errors
    /// Fails on an empty or overlong name or time zone, or control or invisible characters in either.
    pub fn new(name: &str, time_zone: &str) -> Result<Self, Error> {
        plain("location name", name)?;
        plain("time zone", time_zone)?;
        let (name, time_zone) = (name.trim(), time_zone.trim());
        if [name, time_zone]
            .iter()
            .any(|text| text.is_empty() || text.chars().count() > MAX_NAME)
        {
            return Err(Error::Invalid(format!(
                "a location needs a name and a time zone of 1 to {MAX_NAME} characters"
            )));
        }
        Ok(Self {
            name: name.to_string(),
            time_zone: time_zone.to_string(),
        })
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn time_zone(&self) -> &str {
        &self.time_zone
    }
}

/// A tenant to create, checked before it reaches storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTenant {
    slug: Slug,
    name: String,
    theme: Theme,
    locations: Vec<Location>,
}

impl NewTenant {
    /// # Errors
    /// Fails on an empty or overlong name, or control or invisible characters in it.
    pub fn new(
        slug: Slug,
        name: &str,
        theme: Theme,
        locations: Vec<Location>,
    ) -> Result<Self, Error> {
        plain("name", name)?;
        let name = name.trim();
        if name.is_empty() || name.chars().count() > MAX_NAME {
            return Err(Error::Invalid(format!(
                "a name must be 1 to {MAX_NAME} characters"
            )));
        }
        Ok(Self {
            slug,
            name: name.to_string(),
            theme,
            locations,
        })
    }

    #[must_use]
    pub fn slug(&self) -> &Slug {
        &self.slug
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    #[must_use]
    pub fn locations(&self) -> &[Location] {
        &self.locations
    }

    pub(crate) fn stored_as(&self, id: TenantId) -> Tenant {
        Tenant {
            id,
            new: self.clone(),
        }
    }
}

/// A stored tenant: the checked values, plus the ID storage gave it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tenant {
    id: TenantId,
    new: NewTenant,
}

impl Tenant {
    #[must_use]
    pub fn id(&self) -> TenantId {
        self.id
    }

    #[must_use]
    pub fn slug(&self) -> &Slug {
        self.new.slug()
    }

    #[must_use]
    pub fn name(&self) -> &str {
        self.new.name()
    }

    #[must_use]
    pub fn theme(&self) -> &Theme {
        self.new.theme()
    }

    #[must_use]
    pub fn locations(&self) -> &[Location] {
        self.new.locations()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme {
            primary: Color::parse("#3A4250").unwrap(),
            accent: Color::parse("#E5622B").unwrap(),
        }
    }

    #[test]
    fn names_reserved_for_tetsus_own_pages_are_refused() {
        for reserved in ["assets", "api", "login", "admin", "www", "status", "embed"] {
            let error = Slug::parse(reserved).unwrap_err().to_string();
            assert!(error.contains("reserved"), "{reserved}: {error}");
        }
        for near in ["assets-co", "my-api", "apis", "login2", "wwww"] {
            assert_eq!(Slug::parse(near).unwrap().as_str(), near);
        }
    }

    #[test]
    fn slugs_are_lowercase_letters_digits_and_inner_hyphens() {
        for good in ["a", "sample-studio", "lot-42", &"a".repeat(63)] {
            assert_eq!(Slug::parse(good).unwrap().as_str(), good);
        }
        for bad in [
            "",
            "Sample",
            "sample_studio",
            "-sample",
            "sample-",
            "sample--studio",
            "sample studio",
            &"a".repeat(64),
            "é",
        ] {
            assert!(
                matches!(Slug::parse(bad), Err(Error::Invalid(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_slug_is_reached_at_its_path() {
        assert_eq!(
            Slug::parse("sample-studio").unwrap().address(),
            "/sample-studio"
        );
    }

    #[test]
    fn colors_are_six_hex_digits() {
        assert_eq!(Color::parse("#3a4250").unwrap().as_str(), "#3A4250");
        for bad in ["3A4250", "#3A425", "#3A42500", "#GGGGGG", "#3A425 ", ""] {
            assert!(
                matches!(Color::parse(bad), Err(Error::Invalid(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_tenant_needs_a_name_of_reasonable_length() {
        let slug = Slug::parse("sample-studio").unwrap();
        let denver = Location::new("Main Street", "America/Denver").unwrap();
        assert!(NewTenant::new(slug.clone(), "Sample Studio", theme(), vec![denver]).is_ok());
        assert!(NewTenant::new(slug.clone(), &"a".repeat(100), theme(), vec![]).is_ok());
        for bad in ["", "   ", &"a".repeat(101)] {
            assert!(matches!(
                NewTenant::new(slug.clone(), bad, theme(), vec![]),
                Err(Error::Invalid(_))
            ));
        }
    }

    #[test]
    fn the_name_is_trimmed() {
        let tenant = NewTenant::new(
            Slug::parse("a").unwrap(),
            "  Sample Studio  ",
            theme(),
            vec![],
        )
        .unwrap();
        assert_eq!(tenant.name(), "Sample Studio");
    }

    #[test]
    fn a_location_needs_a_name_and_a_time_zone() {
        assert!(matches!(
            Location::new("", "America/Denver"),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            Location::new("Denver", " "),
            Err(Error::Invalid(_))
        ));
        let long = "a".repeat(100);
        assert!(Location::new(&long, &long).is_ok());
        let longer = "a".repeat(101);
        for (name, zone) in [(longer.as_str(), "UTC"), ("Denver", longer.as_str())] {
            assert!(matches!(Location::new(name, zone), Err(Error::Invalid(_))));
        }
        let location = Location::new(" Denver ", " America/Denver ").unwrap();
        assert_eq!(
            (location.name(), location.time_zone()),
            ("Denver", "America/Denver")
        );
    }

    #[test]
    fn text_refuses_control_and_invisible_characters() {
        let slug = Slug::parse("a").unwrap();
        for bad in [
            "Bad\u{0}Name",
            "Two\nLines",
            "Tab\there",
            "Line\u{2028}Two",
            "Para\u{2029}Two",
            "Ab\u{202E}cd",
            "Ab\u{2067}cd\u{2069}",
            "\u{200B}",
            "Word\u{2060}Joiner",
            "\u{FEFF}Name",
            "\u{200D}",
            "\u{200D}\u{FE0F}",
            "\u{3164}",
            "\u{115F}Name",
            "So\u{AD}ft",
            "A\u{E0041}\u{E0042}",
            "A\u{200F}B",
            "A\u{61C}B",
            "Studio\u{2028}",
            "\u{2029}Studio",
            "Studio\n",
            "\tStudio",
            "\u{2800}",
            "\u{2800}\u{2800}",
        ] {
            refused_everywhere(bad);
        }
        assert!(NewTenant::new(slug, "Café Ünïcode", theme(), vec![]).is_ok());
    }

    /// A tenant name, a location name, and a time zone all refuse it.
    fn refused_everywhere(bad: &str) {
        let slug = Slug::parse("a").unwrap();
        assert!(
            matches!(
                NewTenant::new(slug, bad, theme(), vec![]),
                Err(Error::Invalid(_))
            ),
            "{bad:?}"
        );
        assert!(
            matches!(Location::new(bad, "UTC"), Err(Error::Invalid(_))),
            "{bad:?}"
        );
        assert!(
            matches!(Location::new("Main", bad), Err(Error::Invalid(_))),
            "{bad:?}"
        );
    }

    #[test]
    fn format_characters_alone_show_nothing() {
        for bad in [
            "\u{FFF9}",
            "\u{FFF9}\u{FFFA}\u{FFFB}",
            "\u{13430}",
            "\u{13437}\u{13438}",
            "\u{1343F}",
            "\u{600}",
            "\u{605}",
            "\u{6DD}",
            "\u{70F}",
            "\u{890}\u{891}",
            "\u{8E2}",
            "\u{110BD}",
            "\u{110CD}",
        ] {
            refused_everywhere(bad);
        }
        // A number sign before digits still shows the digits.
        let slug = Slug::parse("a").unwrap();
        assert!(NewTenant::new(slug, "\u{600}12", theme(), vec![]).is_ok());
    }

    #[test]
    fn joiners_that_scripts_and_emoji_need_are_kept() {
        let slug = Slug::parse("a").unwrap();
        for good in [
            "Lift \u{1F3CB}\u{FE0F}\u{200D}\u{2640}\u{FE0F}",
            "می\u{200C}خواهم",
            "\u{182E}\u{180B}\u{1823}\u{1829}",
        ] {
            assert!(
                NewTenant::new(slug.clone(), good, theme(), vec![]).is_ok(),
                "{good:?}"
            );
        }
    }

    #[test]
    fn a_stored_tenant_keeps_its_values_and_id() {
        let slug = Slug::parse("a").unwrap();
        let new = NewTenant::new(slug.clone(), "A", theme(), vec![]).unwrap();
        let stored = new.stored_as(TenantId::new(7));
        assert_eq!(
            (stored.id().get(), stored.slug(), stored.name()),
            (7, &slug, "A")
        );
        assert_eq!(
            (stored.theme(), stored.locations()),
            (new.theme(), new.locations())
        );
    }

    #[test]
    fn errors_explain_themselves() {
        assert_eq!(
            Error::Invalid("bad slug".into()).to_string(),
            "invalid: bad slug"
        );
        assert_eq!(Error::SlugTaken.to_string(), "that slug is taken");
        assert_eq!(Error::AddressTaken.to_string(), "that address is taken");
        assert_eq!(Error::NotFound.to_string(), "no tenant at that address");
        assert_eq!(
            Error::Unavailable("closed".into()).to_string(),
            "temporarily unavailable: closed"
        );
        assert_eq!(
            Error::Storage("down".into()).to_string(),
            "storage failed: down"
        );
    }
}
