//! Tenant values. Fields are private, so every value passes its checks when it is made.

const MAX_SLUG: usize = 63;
const MAX_NAME: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Invalid(String),
    SlugTaken,
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
            Self::NotFound => f.write_str("no tenant at that address"),
            Self::Unavailable(why) => write!(f, "temporarily unavailable: {why}"),
            Self::Storage(why) => write!(f, "storage failed: {why}"),
        }
    }
}

impl std::error::Error for Error {}

/// Refuses control characters, such as a NUL that Postgres cannot store or a line break in a name.
fn plain(what: &str, value: &str) -> Result<(), Error> {
    if value.chars().any(char::is_control) {
        return Err(Error::Invalid(format!(
            "{what} {value:?} must not hold control characters"
        )));
    }
    Ok(())
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slug(String);

impl Slug {
    /// # Errors
    /// Fails unless the slug is 1 to 63 lowercase letters, digits, and inner hyphens.
    pub fn parse(value: &str) -> Result<Self, Error> {
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
    /// Fails on an empty name or time zone, or control characters in either.
    pub fn new(name: &str, time_zone: &str) -> Result<Self, Error> {
        let (name, time_zone) = (name.trim(), time_zone.trim());
        if name.is_empty() || time_zone.is_empty() {
            return Err(Error::Invalid(
                "a location needs a name and a time zone".into(),
            ));
        }
        plain("location name", name)?;
        plain("time zone", time_zone)?;
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
    /// Fails on an empty or overlong name, or control characters in it.
    pub fn new(
        slug: Slug,
        name: &str,
        theme: Theme,
        locations: Vec<Location>,
    ) -> Result<Self, Error> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > MAX_NAME {
            return Err(Error::Invalid(format!(
                "a name must be 1 to {MAX_NAME} characters"
            )));
        }
        plain("name", name)?;
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
    fn slugs_are_lowercase_letters_digits_and_inner_hyphens() {
        for good in ["a", "anvil-row", "lot-42", &"a".repeat(63)] {
            assert_eq!(Slug::parse(good).unwrap().as_str(), good);
        }
        for bad in [
            "",
            "Anvil",
            "anvil_row",
            "-anvil",
            "anvil-",
            "anvil--row",
            "anvil row",
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
        assert_eq!(Slug::parse("anvil-row").unwrap().address(), "/anvil-row");
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
        let slug = Slug::parse("anvil-row").unwrap();
        let denver = Location::new("Denver, LoDo", "America/Denver").unwrap();
        assert!(NewTenant::new(slug.clone(), "Anvil Row Strength", theme(), vec![denver]).is_ok());
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
        let tenant =
            NewTenant::new(Slug::parse("a").unwrap(), "  Anvil Row  ", theme(), vec![]).unwrap();
        assert_eq!(tenant.name(), "Anvil Row");
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
        let location = Location::new(" Denver ", " America/Denver ").unwrap();
        assert_eq!(
            (location.name(), location.time_zone()),
            ("Denver", "America/Denver")
        );
    }

    #[test]
    fn text_refuses_control_characters_storage_could_not_keep() {
        let slug = Slug::parse("a").unwrap();
        for bad in ["Bad\u{0}Name", "Two\nLines", "Tab\there"] {
            assert!(
                matches!(
                    NewTenant::new(slug.clone(), bad, theme(), vec![]),
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
        assert!(NewTenant::new(slug, "Café Ünïcode", theme(), vec![]).is_ok());
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
