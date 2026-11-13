//! Settings from the environment.

use std::collections::HashMap;
use std::env::VarError;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;

use tetsu_postgres::{Settings, Tls};

/// Reads `DATABASE_URL`, `DATABASE_TLS`, and `DATABASE_CA_FILE`, through `var` so tests need not touch the real
/// environment. TLS stays required unless `DATABASE_TLS` is `disable`, and any other value is refused rather than
/// guessed. `DATABASE_CA_FILE` names a PEM file of authorities trusted instead of the public roots (ADR-0013). The
/// URL and the file are checked here, so bad settings fail before any connection; the URL may not choose TLS.
pub fn database(var: impl Fn(&str) -> Result<String, VarError>) -> Result<Settings, String> {
    let url = value(&var, "DATABASE_URL")?.ok_or("DATABASE_URL must be set")?;
    let tls = value(&var, "DATABASE_TLS")?;
    let mut settings = match tls.as_deref() {
        None | Some("require" | "disable") => Settings::from_values(&url, tls.as_deref()),
        // The value is not repeated: a URL pasted into this variable by mistake would print its password.
        Some(_) => return Err("DATABASE_TLS must be require or disable".into()),
    };
    if let Some(ca_file) = value(&var, "DATABASE_CA_FILE")? {
        if settings.tls() == Tls::Disabled {
            return Err("DATABASE_CA_FILE needs TLS, which DATABASE_TLS turns off".into());
        }
        settings = settings
            .with_ca_file(Path::new(&ca_file))
            .map_err(|error| format!("DATABASE_CA_FILE: {error}"))?;
    }
    settings.check().map_err(|error| error.to_string())?;
    Ok(settings)
}

/// Where `tetsu serve` listens. A host such as Render sets `PORT` and expects every interface, so with `PORT` it
/// listens on `0.0.0.0`. Without it, it listens on this machine only, at port 8080.
pub fn listen(var: impl Fn(&str) -> Result<String, VarError>) -> Result<SocketAddr, String> {
    match value(&var, "PORT")? {
        None => Ok(SocketAddr::from((Ipv4Addr::LOCALHOST, 8080))),
        Some(port) => match port.parse::<u16>() {
            Ok(port) if port > 0 => Ok(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port))),
            _ => Err(format!(
                "PORT must be a number from 1 to 65535, not {port:?}"
            )),
        },
    }
}

/// Whether this environment holds demo businesses, from `TETSU_DEMO`. It must be `true` or `false`, with no
/// default, so an environment never serves real businesses marked as demos or demos left unmarked (ADR-0018).
pub fn demo(var: impl Fn(&str) -> Result<String, VarError>) -> Result<bool, String> {
    match value(&var, "TETSU_DEMO")?.as_deref() {
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        _ => Err("TETSU_DEMO must be set to true or false".into()),
    }
}

/// Where logs are exported over OTLP (ADR-0049), read from OpenTelemetry's standard names.
#[derive(PartialEq, Eq)]
pub struct Export {
    /// The full address logs are sent to, ending in `/v1/logs` unless the logs endpoint was set itself.
    pub endpoint: String,
    /// Sent with every export, such as the backend's key: names in lowercase, values still percent-encoded, as
    /// the exporter decodes them itself.
    pub headers: HashMap<String, String>,
    pub gzip: bool,
    pub service_name: String,
    /// Further attributes naming this process to the backend, such as its environment, decoded.
    pub resource: Vec<(String, String)>,
}

/// Shows header names and never their values, which hold the backend's key, so printing settings leaks nothing.
impl std::fmt::Debug for Export {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut names: Vec<&String> = self.headers.keys().collect();
        names.sort();
        f.debug_struct("Export")
            .field("endpoint", &self.endpoint)
            .field("headers", &names)
            .field("gzip", &self.gzip)
            .field("service_name", &self.service_name)
            .field("resource", &self.resource)
            .finish()
    }
}

/// Reads the export settings as the exporter does, the logs-only name over the shared one: the endpoint from
/// `OTEL_EXPORTER_OTLP_LOGS_ENDPOINT`, or else `OTEL_EXPORTER_OTLP_ENDPOINT` with `/v1/logs` added; the headers
/// from `OTEL_EXPORTER_OTLP_LOGS_HEADERS` or `OTEL_EXPORTER_OTLP_HEADERS`; compression, `gzip` or `none`, from
/// `OTEL_EXPORTER_OTLP_LOGS_COMPRESSION` or `OTEL_EXPORTER_OTLP_COMPRESSION`; `OTEL_SERVICE_NAME`, default
/// `tetsu`; and `OTEL_RESOURCE_ATTRIBUTES`. With no endpoint, logs are not exported. Errors name the variable and
/// never repeat its value, since the headers hold a key.
pub fn export(var: impl Fn(&str) -> Result<String, VarError>) -> Result<Option<Export>, String> {
    let Some(endpoint) = endpoint(&var)? else {
        return Ok(None);
    };
    Ok(Some(Export {
        endpoint,
        headers: headers(&var)?,
        gzip: gzip(&var)?,
        service_name: value(&var, "OTEL_SERVICE_NAME")?.unwrap_or_else(|| "tetsu".into()),
        resource: decoded_pairs(&var, "OTEL_RESOURCE_ATTRIBUTES")?,
    }))
}

/// The logs endpoint, which must be HTTPS, since the headers carry a key, or HTTP to this machine. It may not carry
/// a credential itself, since the exporter's errors quote it on standard output.
fn endpoint(var: impl Fn(&str) -> Result<String, VarError>) -> Result<Option<String>, String> {
    let (name, endpoint) = match (
        value(&var, "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT")?,
        value(&var, "OTEL_EXPORTER_OTLP_ENDPOINT")?,
    ) {
        (Some(endpoint), _) => ("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT", endpoint),
        (None, Some(base)) => (
            "OTEL_EXPORTER_OTLP_ENDPOINT",
            format!("{}/v1/logs", base.trim_end_matches('/')),
        ),
        (None, None) => return Ok(None),
    };
    checked_endpoint(name, &endpoint)?;
    Ok(Some(endpoint))
}

fn checked_endpoint(name: &str, endpoint: &str) -> Result<(), String> {
    let wrong = || format!("{name} must be an https URL, or http to this machine");
    let uri: axum::http::Uri = endpoint.parse().map_err(|_| wrong())?;
    if uri
        .authority()
        .is_some_and(|authority| authority.as_str().contains('@'))
    {
        return Err(format!(
            "{name} must not carry a credential; send it in OTEL_EXPORTER_OTLP_HEADERS"
        ));
    }
    let local = matches!(uri.host(), Some("127.0.0.1" | "localhost" | "[::1]"));
    match uri.scheme_str() {
        Some("https") => Ok(()),
        Some("http") if local => Ok(()),
        _ => Err(wrong()),
    }
}

fn gzip(var: impl Fn(&str) -> Result<String, VarError>) -> Result<bool, String> {
    let (name, compression) = first(
        &var,
        "OTEL_EXPORTER_OTLP_LOGS_COMPRESSION",
        "OTEL_EXPORTER_OTLP_COMPRESSION",
    )?;
    match compression.as_deref() {
        None | Some("none") => Ok(false),
        Some("gzip") => Ok(true),
        Some(_) => Err(format!("{name} must be gzip or none")),
    }
}

/// Headers whose names are HTTP tokens and whose values, decoded, hold no control characters, so none is dropped
/// on the way out without a word.
fn headers(
    var: impl Fn(&str) -> Result<String, VarError>,
) -> Result<HashMap<String, String>, String> {
    let (name, _) = first(
        &var,
        "OTEL_EXPORTER_OTLP_LOGS_HEADERS",
        "OTEL_EXPORTER_OTLP_HEADERS",
    )?;
    let wrong =
        || format!("{name} must be key=value pairs, separated by commas and percent-encoded");
    pairs(&var, name)?
        .into_iter()
        .map(|(key, raw)| {
            let key = decoded(key).ok_or_else(wrong)?.to_ascii_lowercase();
            let value = decoded(&raw).ok_or_else(wrong)?;
            let token = key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte));
            let printable = !value.chars().any(|c| c.is_control() && c != '\t');
            if !token || !printable {
                return Err(wrong());
            }
            Ok((key, raw))
        })
        .collect()
}

/// The first of two variables that is set, by name, or the second's name when neither is.
fn first(
    var: impl Fn(&str) -> Result<String, VarError>,
    preferred: &'static str,
    shared: &'static str,
) -> Result<(&'static str, Option<String>), String> {
    match value(&var, preferred)? {
        Some(found) => Ok((preferred, Some(found))),
        None => Ok((shared, value(&var, shared)?)),
    }
}

/// Pairs with both parts percent-decoded.
fn decoded_pairs(
    var: impl Fn(&str) -> Result<String, VarError>,
    name: &str,
) -> Result<Vec<(String, String)>, String> {
    let wrong =
        || format!("{name} must be key=value pairs, separated by commas and percent-encoded");
    pairs(&var, name)?
        .into_iter()
        .map(|(key, value)| {
            Ok((
                decoded(key).ok_or_else(wrong)?,
                decoded(value).ok_or_else(wrong)?,
            ))
        })
        .collect()
}

/// Comma-separated `key=value` pairs, as OpenTelemetry writes headers and resource attributes, each part trimmed
/// and still encoded. Empty pairs are skipped, as other OpenTelemetry readers skip them; an empty key is refused.
fn pairs(
    var: impl Fn(&str) -> Result<String, VarError>,
    name: &str,
) -> Result<Vec<(String, String)>, String> {
    let Some(text) = value(&var, name)? else {
        return Ok(Vec::new());
    };
    text.split(',')
        .map(str::trim)
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((key, value)) if !key.trim().is_empty() => {
                Ok((key.trim().to_string(), value.trim().to_string()))
            }
            _ => Err(format!(
                "{name} must be key=value pairs, separated by commas and percent-encoded"
            )),
        })
        .collect()
}

/// Percent-decodes text, or `None` if an escape is not two hex digits or the result is not UTF-8.
fn decoded(text: impl AsRef<str>) -> Option<String> {
    let mut bytes = Vec::new();
    let mut rest = text.as_ref().as_bytes();
    while let Some((&byte, after)) = rest.split_first() {
        if byte == b'%' {
            let hex = after
                .get(..2)
                .filter(|hex| hex.iter().all(u8::is_ascii_hexdigit))?;
            bytes.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
            rest = &after[2..];
        } else {
            bytes.push(byte);
            rest = after;
        }
    }
    String::from_utf8(bytes).ok()
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

    fn listening(port: Option<&str>) -> Result<SocketAddr, String> {
        listen(|name| match (name, port) {
            ("PORT", Some(port)) => Ok(port.to_string()),
            _ => Err(VarError::NotPresent),
        })
    }

    fn marked(value: Option<&str>) -> Result<bool, String> {
        demo(|name| match (name, value) {
            ("TETSU_DEMO", Some(value)) => Ok(value.to_string()),
            _ => Err(VarError::NotPresent),
        })
    }

    #[test]
    fn demo_marking_must_be_set_to_true_or_false() {
        assert_eq!(marked(Some("true")), Ok(true));
        assert_eq!(marked(Some(" false ")), Ok(false));
        for unset in [None, Some(""), Some("yes"), Some("TRUE"), Some("1")] {
            assert_eq!(
                marked(unset),
                Err("TETSU_DEMO must be set to true or false".into()),
                "{unset:?}"
            );
        }
    }

    #[test]
    fn it_listens_on_this_machine_unless_a_host_sets_the_port() {
        assert_eq!(listening(None), Ok("127.0.0.1:8080".parse().unwrap()));
        assert_eq!(listening(Some(" ")), Ok("127.0.0.1:8080".parse().unwrap()));
        assert_eq!(
            listening(Some("10000")),
            Ok("0.0.0.0:10000".parse().unwrap())
        );
        assert_eq!(
            listening(Some("65535")),
            Ok("0.0.0.0:65535".parse().unwrap())
        );
    }

    #[test]
    fn a_port_that_is_not_one_is_refused() {
        for bad in ["0", "65536", "-1", "80a", "8080 9090"] {
            assert_eq!(
                listening(Some(bad)),
                Err(format!(
                    "PORT must be a number from 1 to 65535, not {bad:?}"
                )),
                "{bad}"
            );
        }
    }

    fn read(url: Option<&str>, tls: Option<&str>) -> Result<Settings, String> {
        let found = |value: Option<&str>| value.map(str::to_string).ok_or(VarError::NotPresent);
        database(|name| match name {
            "DATABASE_URL" => found(url),
            "DATABASE_TLS" => found(tls),
            _ => Err(VarError::NotPresent),
        })
    }

    fn read_with_ca(tls: Option<&str>, ca: Option<&str>) -> Result<Settings, String> {
        let found = |value: Option<&str>| value.map(str::to_string).ok_or(VarError::NotPresent);
        database(|name| match name {
            "DATABASE_URL" => Ok("postgres://localhost/tetsu".to_string()),
            "DATABASE_TLS" => found(tls),
            "DATABASE_CA_FILE" => found(ca),
            _ => Err(VarError::NotPresent),
        })
    }

    #[test]
    fn a_ca_file_that_cannot_be_used_names_its_variable() {
        let error = read_with_ca(None, Some("/no/such/tetsu-ca.pem")).unwrap_err();
        assert!(
            error.starts_with("DATABASE_CA_FILE: the CA file could not be read"),
            "{error}"
        );
    }

    #[test]
    fn a_ca_file_without_tls_is_refused_rather_than_ignored() {
        assert_eq!(
            read_with_ca(Some("disable"), Some("/no/such/tetsu-ca.pem")).unwrap_err(),
            "DATABASE_CA_FILE needs TLS, which DATABASE_TLS turns off"
        );
    }

    #[test]
    fn a_blank_ca_file_is_no_ca_file() {
        assert_eq!(read_with_ca(None, Some(" ")).unwrap().tls(), Tls::Required);
        assert_eq!(read_with_ca(None, None).unwrap().tls(), Tls::Required);
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

    fn exporting(set: &[(&str, &str)]) -> Result<Option<Export>, String> {
        export(|name| {
            set.iter()
                .find(|(set, _)| *set == name)
                .map(|(_, value)| (*value).to_string())
                .ok_or(VarError::NotPresent)
        })
    }

    #[test]
    fn logs_are_exported_only_once_an_endpoint_is_set() {
        assert_eq!(exporting(&[]), Ok(None));
        assert_eq!(
            exporting(&[("OTEL_EXPORTER_OTLP_HEADERS", "a=b")]),
            Ok(None)
        );
        let export = exporting(&[("OTEL_EXPORTER_OTLP_ENDPOINT", "https://otlp.example/otlp/")])
            .unwrap()
            .unwrap();
        assert_eq!(
            export,
            Export {
                endpoint: "https://otlp.example/otlp/v1/logs".into(),
                headers: HashMap::new(),
                gzip: false,
                service_name: "tetsu".into(),
                resource: Vec::new(),
            }
        );
    }

    #[test]
    fn the_logs_endpoint_is_used_as_set_over_the_shared_one() {
        let export = exporting(&[
            ("OTEL_EXPORTER_OTLP_ENDPOINT", "https://shared.example"),
            (
                "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT",
                "https://logs.example/in",
            ),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(export.endpoint, "https://logs.example/in");
    }

    #[test]
    fn headers_and_attributes_are_decoded_pairs() {
        let export = exporting(&[
            ("OTEL_EXPORTER_OTLP_ENDPOINT", "https://otlp.example"),
            (
                "OTEL_EXPORTER_OTLP_HEADERS",
                "Authorization=Basic%20MTIzOmtleQ==, X-Scope = a%2Cb",
            ),
            ("OTEL_EXPORTER_OTLP_COMPRESSION", "gzip"),
            ("OTEL_SERVICE_NAME", "tetsu-demo"),
            (
                "OTEL_RESOURCE_ATTRIBUTES",
                "deployment.environment.name=demo",
            ),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(export.headers["authorization"], "Basic%20MTIzOmtleQ==");
        assert_eq!(export.headers["x-scope"], "a%2Cb");
        assert_eq!(export.headers.len(), 2);
        assert!(export.gzip);
        assert_eq!(export.service_name, "tetsu-demo");
        assert_eq!(
            export.resource,
            vec![("deployment.environment.name".into(), "demo".into())]
        );
    }

    #[test]
    fn a_broken_pair_is_refused_without_repeating_it() {
        let wrong = "OTEL_EXPORTER_OTLP_HEADERS must be key=value pairs, separated by commas and percent-encoded";
        for bad in [
            "Basic secret",
            "=secret",
            "a=%zz",
            "a=%2",
            "a=%+1",
            "a=%ff",
            "%ff=a",
            "a b=c",
            "a=line%0Abreak",
        ] {
            assert_eq!(
                exporting(&[
                    ("OTEL_EXPORTER_OTLP_ENDPOINT", "https://otlp.example"),
                    ("OTEL_EXPORTER_OTLP_HEADERS", bad),
                ]),
                Err(wrong.into()),
                "{bad}"
            );
        }
    }

    #[test]
    fn empty_pairs_are_skipped() {
        let export = exporting(&[
            ("OTEL_EXPORTER_OTLP_ENDPOINT", "https://otlp.example"),
            ("OTEL_EXPORTER_OTLP_HEADERS", "a=b,, "),
            ("OTEL_RESOURCE_ATTRIBUTES", ",k=v%20w,"),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(export.headers.len(), 1);
        assert_eq!(export.resource, vec![("k".into(), "v w".into())]);
    }

    /// The logs-only names win over the shared ones, as they do in the exporter, so both read the same.
    #[test]
    fn logs_only_headers_and_compression_win_over_shared_ones() {
        let export = exporting(&[
            ("OTEL_EXPORTER_OTLP_ENDPOINT", "https://otlp.example"),
            ("OTEL_EXPORTER_OTLP_HEADERS", "shared=1"),
            ("OTEL_EXPORTER_OTLP_LOGS_HEADERS", "logs=1"),
            ("OTEL_EXPORTER_OTLP_COMPRESSION", "none"),
            ("OTEL_EXPORTER_OTLP_LOGS_COMPRESSION", "gzip"),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(export.headers.keys().collect::<Vec<_>>(), ["logs"]);
        assert!(export.gzip);
        assert_eq!(
            exporting(&[
                ("OTEL_EXPORTER_OTLP_ENDPOINT", "https://otlp.example"),
                ("OTEL_EXPORTER_OTLP_LOGS_HEADERS", "nothing"),
            ]),
            Err("OTEL_EXPORTER_OTLP_LOGS_HEADERS must be key=value pairs, separated by commas and percent-encoded".into())
        );
        assert_eq!(
            exporting(&[
                ("OTEL_EXPORTER_OTLP_ENDPOINT", "https://otlp.example"),
                ("OTEL_EXPORTER_OTLP_LOGS_COMPRESSION", "zstd"),
            ]),
            Err("OTEL_EXPORTER_OTLP_LOGS_COMPRESSION must be gzip or none".into())
        );
    }

    /// The headers carry a key, so the endpoint must be HTTPS, or HTTP that never leaves this machine.
    #[test]
    fn the_endpoint_must_be_https_or_local() {
        let at =
            |name, endpoint| exporting(&[(name, endpoint)]).map(|export| export.unwrap().endpoint);
        for local in [
            "http://127.0.0.1:4318",
            "http://localhost:4318",
            "http://[::1]:4318",
        ] {
            assert_eq!(
                at("OTEL_EXPORTER_OTLP_ENDPOINT", local),
                Ok(format!("{local}/v1/logs"))
            );
        }
        let wrong = Err(
            "OTEL_EXPORTER_OTLP_ENDPOINT must be an https URL, or http to this machine".to_string(),
        );
        for bad in [
            "http://otlp.example",
            "ftp://otlp.example",
            "otlp.example",
            "/v1/logs",
        ] {
            assert_eq!(at("OTEL_EXPORTER_OTLP_ENDPOINT", bad), wrong, "{bad}");
        }
        assert_eq!(
            at("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT", "https://exa mple"),
            Err(
                "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT must be an https URL, or http to this machine"
                    .into()
            )
        );
    }

    #[test]
    fn the_endpoint_may_not_carry_a_credential() {
        assert_eq!(
            exporting(&[("OTEL_EXPORTER_OTLP_ENDPOINT", "https://user:key@otlp.example")]),
            Err("OTEL_EXPORTER_OTLP_ENDPOINT must not carry a credential; send it in OTEL_EXPORTER_OTLP_HEADERS".into())
        );
    }

    /// Printed settings name each header and never its value, the backend's key.
    #[test]
    fn printed_settings_hide_header_values() {
        let export = exporting(&[
            ("OTEL_EXPORTER_OTLP_ENDPOINT", "https://otlp.example"),
            (
                "OTEL_EXPORTER_OTLP_HEADERS",
                "Authorization=Basic%20c2VjcmV0,x-scope=a",
            ),
        ])
        .unwrap()
        .unwrap();
        let printed = format!("{export:?}");
        assert!(
            printed.contains("[\"authorization\", \"x-scope\"]"),
            "{printed}"
        );
        assert!(
            printed.contains("https://otlp.example/v1/logs"),
            "{printed}"
        );
        assert!(!printed.contains("c2VjcmV0"), "{printed}");
    }

    #[test]
    fn compression_is_gzip_or_none() {
        let compressed = |value| {
            exporting(&[
                ("OTEL_EXPORTER_OTLP_ENDPOINT", "https://otlp.example"),
                ("OTEL_EXPORTER_OTLP_COMPRESSION", value),
            ])
            .map(|export| export.unwrap().gzip)
        };
        assert_eq!(compressed("none"), Ok(false));
        assert_eq!(compressed("gzip"), Ok(true));
        assert_eq!(
            compressed("zstd"),
            Err("OTEL_EXPORTER_OTLP_COMPRESSION must be gzip or none".into())
        );
    }
}
