//! The contract every Tetsu module implements (ADR-0003).
//!
//! This is a contract, not a framework: modules plug in at compile time, and
//! the `tetsu` binary registers them.

/// One numbered SQL file. Versions only ever grow, and an applied file never changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Migration {
    pub version: i32,
    pub sql: &'static str,
}

/// Where a module keeps its data: the one Postgres schema it owns, and its migrations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Storage {
    pub schema: &'static str,
    pub migrations: &'static [Migration],
}

/// A module that plugs into the kernel.
pub trait Module {
    /// The module's name, unique among the modules the binary registers.
    fn name(&self) -> &'static str;

    /// The module's storage, if it stores data.
    fn storage(&self) -> Option<Storage> {
        None
    }
}

/// The color scheme a visitor chose for Tetsu's pages, kept in the `theme` cookie (ADR-0070): light or dark, or the
/// system's, which is no cookie. Every module's pages share one page shell, which renders the choice, so they read it
/// the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scheme {
    #[default]
    System,
    Light,
    Dark,
}

impl Scheme {
    /// The scheme the `theme` cookie names among a request's `Cookie` headers; none, or any other value, is the
    /// system's.
    #[must_use]
    pub fn from_cookies<'a>(headers: impl IntoIterator<Item = &'a str>) -> Self {
        headers
            .into_iter()
            .flat_map(|header| header.split(';'))
            .find_map(|pair| match pair.trim() {
                "theme=light" => Some(Self::Light),
                "theme=dark" => Some(Self::Dark),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// The radio the page checks for it, as the page shell names them.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scheme_is_read_from_the_theme_cookie() {
        for (headers, scheme) in [
            (vec!["theme=dark"], Scheme::Dark),
            (vec!["a=1; theme=light; b=2"], Scheme::Light),
            (vec!["a=1", "theme=dark"], Scheme::Dark),
            (vec![" theme=dark "], Scheme::Dark),
        ] {
            assert_eq!(Scheme::from_cookies(headers.clone()), scheme, "{headers:?}");
        }
    }

    #[test]
    fn a_missing_or_unknown_theme_is_the_system_scheme() {
        for headers in [
            vec![],
            vec!["theme="],
            vec!["theme=Dark"],
            vec!["theme=system"],
            vec!["xtheme=dark"],
            vec!["theme=dark2"],
        ] {
            assert_eq!(
                Scheme::from_cookies(headers.clone()),
                Scheme::System,
                "{headers:?}"
            );
        }
    }

    #[test]
    fn a_scheme_names_the_radio_it_checks() {
        assert_eq!(
            [Scheme::System, Scheme::Light, Scheme::Dark].map(Scheme::as_str),
            ["system", "light", "dark"]
        );
        assert_eq!(Scheme::default(), Scheme::System);
    }

    struct Plain;

    impl Module for Plain {
        fn name(&self) -> &'static str {
            "plain"
        }
    }

    #[test]
    fn a_module_stores_nothing_unless_it_says_so() {
        assert_eq!(Plain.name(), "plain");
        assert_eq!(Plain.storage(), None);
    }
}
