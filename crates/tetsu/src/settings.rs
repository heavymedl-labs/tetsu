//! Settings from the environment.

use tetsu_postgres::Settings;

/// Reads `DATABASE_URL` and `DATABASE_TLS`, through `var` so tests need not touch the real environment.
/// TLS stays required unless `DATABASE_TLS` is `disable`, and any other value is refused rather than guessed.
pub fn database(var: impl Fn(&str) -> Option<String>) -> Result<Settings, String> {
    let url = var("DATABASE_URL")
        .filter(|url| !url.trim().is_empty())
        .ok_or("DATABASE_URL must be set")?;
    let tls = var("DATABASE_TLS");
    match tls.as_deref() {
        None | Some("require" | "disable") => Ok(Settings::from_values(&url, tls.as_deref())),
        Some(other) => Err(format!(
            "DATABASE_TLS must be require or disable, not {other:?}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tetsu_postgres::Tls;

    fn read(url: Option<&str>, tls: Option<&str>) -> Result<Settings, String> {
        database(|name| match name {
            "DATABASE_URL" => url.map(str::to_string),
            "DATABASE_TLS" => tls.map(str::to_string),
            _ => None,
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
        assert_eq!(
            error,
            "DATABASE_TLS must be require or disable, not \"disabled\""
        );
    }

    #[test]
    fn the_url_must_be_set_and_not_blank() {
        for url in [None, Some(""), Some("  ")] {
            assert_eq!(read(url, None).unwrap_err(), "DATABASE_URL must be set");
        }
    }
}
