//! Settings from the environment.

use std::env::VarError;

use tetsu_postgres::Settings;

/// Reads `DATABASE_URL` and `DATABASE_TLS`, through `var` so tests need not touch the real environment.
/// TLS stays required unless `DATABASE_TLS` is `disable`, and any other value is refused rather than guessed.
/// The URL is checked here, so a bad one fails before any connection; it may not choose TLS itself.
pub fn database(var: impl Fn(&str) -> Result<String, VarError>) -> Result<Settings, String> {
    let url = value(&var, "DATABASE_URL")?.ok_or("DATABASE_URL must be set")?;
    let tls = value(&var, "DATABASE_TLS")?;
    let settings = match tls.as_deref() {
        None | Some("require" | "disable") => Settings::from_values(&url, tls.as_deref()),
        // The value is not repeated: a URL pasted into this variable by mistake would print its password.
        Some(_) => return Err("DATABASE_TLS must be require or disable".into()),
    };
    settings.check().map_err(|error| error.to_string())?;
    Ok(settings)
}

/// A missing or blank variable is `None`, and surrounding spaces are dropped. One that is not UTF-8 is an error,
/// not a missing value.
fn value(
    var: impl Fn(&str) -> Result<String, VarError>,
    name: &str,
) -> Result<Option<String>, String> {
    match var(name) {
        Ok(value) => Ok(Some(value.trim().to_string()).filter(|value| !value.is_empty())),
        Err(VarError::NotPresent) => Ok(None),
        Err(VarError::NotUnicode(_)) => Err(format!("{name} must be UTF-8")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tetsu_postgres::Tls;

    fn read(url: Option<&str>, tls: Option<&str>) -> Result<Settings, String> {
        let found = |value: Option<&str>| value.map(str::to_string).ok_or(VarError::NotPresent);
        database(|name| match name {
            "DATABASE_URL" => found(url),
            "DATABASE_TLS" => found(tls),
            _ => Err(VarError::NotPresent),
        })
    }

    #[test]
    fn tls_is_required_unless_disabled_by_name() {
        let url = Some("postgres://localhost/tetsu");
        assert_eq!(read(url, None).unwrap().tls(), Tls::Required);
        assert_eq!(read(url, Some("require")).unwrap().tls(), Tls::Required);
        assert_eq!(read(url, Some("disable")).unwrap().tls(), Tls::Disabled);
        assert_eq!(read(url, None).unwrap().url(), "postgres://localhost/tetsu");
    }

    #[test]
    fn an_unknown_tls_value_is_refused() {
        let error = read(Some("postgres://localhost/tetsu"), Some("disabled")).unwrap_err();
        assert_eq!(error, "DATABASE_TLS must be require or disable");
        // Never repeated back, since a URL pasted into the wrong variable would print its password.
        let swapped = read(
            Some("postgres://localhost/tetsu"),
            Some("postgres://u:hunter2@h/db"),
        );
        assert!(!swapped.unwrap_err().contains("hunter2"));
    }

    #[test]
    fn tls_is_read_like_the_url() {
        let url = Some("postgres://localhost/tetsu");
        assert_eq!(read(url, Some(" disable\n")).unwrap().tls(), Tls::Disabled);
        assert_eq!(read(url, Some("")).unwrap().tls(), Tls::Required);
    }

    #[test]
    fn tls_is_set_only_through_its_own_variable() {
        for url in [
            "postgres://localhost/tetsu?sslmode=disable",
            "host=localhost sslmode=require",
            "postgres://localhost/tetsu?ss%6Cmode=require",
            "postgres://localhost/tetsu?sslmode=prefer",
            "postgres://localhost/tetsu?sslmode=disable&sslmode=prefer",
            "host=localhost sslmode=prefer",
            "host=localhost sslmode = prefer",
        ] {
            assert_eq!(
                read(Some(url), None).unwrap_err(),
                "the database URL must not set sslmode; TLS is set separately",
                "{url}"
            );
        }
        let password = read(Some("postgres://u:mysslmodepw@localhost/tetsu"), None);
        assert!(password.is_ok(), "a password may contain any text");
    }

    #[test]
    fn an_invalid_url_is_caught_before_any_connection() {
        assert_eq!(
            read(Some("postgres://localhost:99999/tetsu"), None).unwrap_err(),
            "the database URL is not a valid connection string"
        );
    }

    #[test]
    fn url_options_that_would_swallow_the_pool_timeout_are_refused() {
        assert_eq!(
            read(
                Some("postgres://localhost/tetsu?options=-c%20search_path%3Dx%5C"),
                None
            )
            .unwrap_err(),
            "the database URL's options must not end in a backslash"
        );
    }

    #[test]
    fn the_url_is_trimmed() {
        let settings = read(Some("  postgres://localhost/tetsu\n"), None).unwrap();
        assert_eq!(settings.url(), "postgres://localhost/tetsu");
    }

    #[test]
    fn a_value_that_is_not_utf8_is_named_not_treated_as_missing() {
        let error = database(|name| match name {
            "DATABASE_URL" => Err(VarError::NotUnicode("x".into())),
            _ => Err(VarError::NotPresent),
        })
        .unwrap_err();
        assert_eq!(error, "DATABASE_URL must be UTF-8");
    }

    #[test]
    fn the_url_must_be_set_and_not_blank() {
        for url in [None, Some(""), Some("  ")] {
            assert_eq!(read(url, None).unwrap_err(), "DATABASE_URL must be set");
        }
    }
}
