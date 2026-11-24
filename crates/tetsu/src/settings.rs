//! Settings from the environment, or from a local `.env` file for any the environment leaves unset (ADR-0050), or
//! from `.env.<name>` when the command line chooses one (ADR-0073).

use std::collections::HashMap;
use std::env::VarError;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;

use tetsu_postgres::{Settings, Tls};

/// The settings file a command reads: `.env`, or `.env.<name>` when `--env <name>` comes first on its command line
/// (ADR-0073).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Local,
    Named(String),
}

impl Source {
    /// Splits a leading `--env <name>` off `args`, or chooses `.env` when there is none. A name is lowercase letters,
    /// digits, and hyphens, starting with a letter, so it cannot name a path or look like an option.
    ///
    /// # Errors
    /// Fails when `--env` has no name after it, or one that is not a name.
    pub fn chosen(args: &[String]) -> Result<(Self, &[String]), String> {
        let Some((flag, rest)) = args.split_first().filter(|(flag, _)| *flag == "--env") else {
            return Ok((Self::Local, args));
        };
        match rest.split_first() {
            Some((name, rest)) if is_environment(name) => Ok((Self::Named(name.clone()), rest)),
            _ => Err(format!(
                "{flag} takes an environment's name: lowercase letters, digits, and hyphens, starting with a letter"
            )),
        }
    }

    /// The file's name.
    fn file(&self) -> String {
        match self {
            Self::Local => ".env".into(),
            Self::Named(name) => format!(".env.{name}"),
        }
    }
}

/// An environment's name, as [`Source::chosen`] says.
fn is_environment(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Settings a local `.env` file holds (ADR-0050), or an environment's `.env.<name>` (ADR-0073). The environment wins
/// over them, except that the mail settings come all from one or all from the other (ADR-0078). The default holds no settings and never reads the environment, so a test given one cannot reach
/// whatever database its shell exports.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Local {
    file: HashMap<String, String>,
    environment: bool,
    name: String,
}

impl Local {
    /// Reads the file `source` names in `folder`. A missing `.env` holds nothing; a missing `.env.<name>` is an
    /// error, since it was asked for.
    ///
    /// # Errors
    /// Fails when the file cannot be read, or a line is not a setting.
    pub fn load(folder: &Path, source: &Source) -> Result<Self, String> {
        let name = source.file();
        let text = match std::fs::read_to_string(folder.join(&name)) {
            Ok(text) => text,
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound && *source == Source::Local =>
            {
                String::new()
            }
            Err(error) => return Err(format!("{name} could not be read: {error}")),
        };
        parse(&text, &name).map(|file| Self {
            file,
            environment: true,
            name,
        })
    }

    /// Settings from a `.env`'s text alone, never the environment, for a test.
    #[cfg(test)]
    pub(crate) fn file_only(text: &str) -> Self {
        Self {
            file: parse(text, ".env").expect("settings"),
            environment: false,
            name: ".env".into(),
        }
    }

    /// A variable from the environment, or from the file when the environment leaves it unset.
    pub fn var(&self, name: &str) -> Result<String, VarError> {
        self.or(self.environment(name), name)
    }

    /// Whether a variable's value is the file's, the environment having left it unset.
    pub fn in_file(&self, name: &str) -> bool {
        matches!(self.environment(name), Err(VarError::NotPresent)) && self.file.contains_key(name)
    }

    /// Refuses settings from the file that go with `lead` when `lead` comes from the environment: a stray `.env`
    /// in whatever folder a command runs in must not change how a database named elsewhere is reached, or where
    /// its links point.
    ///
    /// # Errors
    /// Names the first such setting.
    pub fn beside(&self, lead: &str, settings: &[&str]) -> Result<(), String> {
        if self.in_file(lead) {
            return Ok(());
        }
        match settings.iter().find(|name| self.in_file(name)) {
            Some(name) => Err(format!(
                "{name} comes from {file} but {lead} does not; set {name} in the environment too, or remove it from \
                 {file}, so a stray {file} cannot change how that database is reached",
                file = self.name
            )),
            None => Ok(()),
        }
    }

    /// Refuses `settings` that come partly from the file and partly from the environment, which set them as a
    /// whole: mixed, a password or server from one could reach the other's, such as a real login reaching a local
    /// Mailpit (ADR-0078).
    ///
    /// # Errors
    /// Names the first from each.
    pub fn together(&self, settings: &[&str]) -> Result<(), String> {
        let from_file = settings.iter().find(|name| self.in_file(name));
        let exported = settings
            .iter()
            .find(|name| !matches!(self.environment(name), Err(VarError::NotPresent)));
        match (from_file, exported) {
            (Some(file_name), Some(exported)) => Err(format!(
                "{file_name} comes from {file} but {exported} from the environment; set these settings all in one \
                 place, so a password or server from one cannot reach the other's",
                file = self.name
            )),
            _ => Ok(()),
        }
    }

    fn environment(&self, name: &str) -> Result<String, VarError> {
        if self.environment {
            std::env::var(name)
        } else {
            Err(VarError::NotPresent)
        }
    }

    fn or(&self, environment: Result<String, VarError>, name: &str) -> Result<String, VarError> {
        match environment {
            Err(VarError::NotPresent) => self.file.get(name).cloned().ok_or(VarError::NotPresent),
            found => found,
        }
    }
}

/// `NAME=value` lines, `#` comments, and blank lines. Values are taken as written, trimmed, with no quotes or
/// expansion. An error names the file and line, and never its text, since a value may be a password.
fn parse(text: &str, file: &str) -> Result<HashMap<String, String>, String> {
    let mut settings = HashMap::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let setting = line
            .split_once('=')
            .map(|(name, value)| (name.trim(), value.trim()));
        match setting {
            Some((name, value)) if is_name(name) => {
                settings.insert(name.to_string(), value.to_string());
            }
            _ => return Err(format!("{file} line {} is not NAME=value", index + 1)),
        }
    }
    Ok(settings)
}

/// A variable's name: letters, digits, and underscores, not starting with a digit.
fn is_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|first| !first.is_ascii_digit())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

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

/// How many proxies stand in front of `tetsu serve`, each adding an `X-Forwarded-For` entry, from
/// `TETSU_TRUSTED_PROXIES`: `0` locally, `3` on Render, as measured (ADR-0072). It has no default, since a wrong
/// count lets a client choose the address its rate limits key on, or puts every client under one.
///
/// # Errors
/// Fails if the setting is missing or not a whole number up to 10.
pub fn proxies(var: impl Fn(&str) -> Result<String, VarError>) -> Result<usize, String> {
    value(&var, "TETSU_TRUSTED_PROXIES")?
        .and_then(|count| count.parse().ok())
        .filter(|count| *count <= 10)
        .ok_or_else(|| {
            "TETSU_TRUSTED_PROXIES must be set to the number of proxies in front, 0 to 10".into()
        })
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

/// The site's own origin, from `TETSU_ORIGIN`, such as `https://tetsu-demo.onrender.com`: invite links point
/// there, and only it may send a state-changing request (ADR-0048). It is written as a browser sends an origin,
/// `https`, or `http` for this machine alone, with no path, so a typo fails here rather than in a link.
pub fn origin(var: impl Fn(&str) -> Result<String, VarError>) -> Result<String, String> {
    let origin = value(&var, "TETSU_ORIGIN")?.ok_or("TETSU_ORIGIN must be set")?;
    tetsu_identity::Origins::parse(&[origin.as_str()]).map_err(|_| {
        format!("TETSU_ORIGIN must be an origin such as https://example.com, not {origin:?}")
    })?;
    Ok(origin)
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

/// How log lines are laid out (ADR-0071): JSON, which a log service reads, or text, which a person reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Json,
    Text,
}

/// The log layout `TETSU_LOG_FORMAT` names, `json` or `text`. Unset, logs are text when they reach a terminal and
/// JSON anywhere else, so a host that collects standard output gets JSON without setting anything.
///
/// # Errors
/// Fails if the setting names neither.
pub fn log_format(
    var: impl Fn(&str) -> Result<String, VarError>,
    terminal: bool,
) -> Result<LogFormat, String> {
    match (value(&var, "TETSU_LOG_FORMAT")?.as_deref(), terminal) {
        (Some("text"), _) | (None, true) => Ok(LogFormat::Text),
        (Some("json"), _) | (None, false) => Ok(LogFormat::Json),
        (Some(_), _) => Err("TETSU_LOG_FORMAT must be json or text".into()),
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

/// How long one SMTP send may take. A notice sends inside a transaction Postgres ends after 10 idle seconds
/// (ADR-0060), and Mail also waits on its pool before and after, so this leaves room for both.
const SMTP_LIMIT: std::time::Duration = std::time::Duration::from_secs(5);

/// The mail settings (ADR-0074), which go with the database, since its mail is what they send.
pub const MAIL: [&str; 7] = [
    "TETSU_SMTP_HOST",
    "TETSU_SMTP_PORT",
    "TETSU_SMTP_USERNAME",
    "TETSU_SMTP_PASSWORD",
    "TETSU_SMTP_CA_FILE",
    "TETSU_MAIL_FROM",
    "TETSU_MAIL_ALLOW",
];

/// Where mail goes: an SMTP server, if one is set, the certificate authority it is also trusted by, if any, and the
/// addresses it may send to; every other message is kept for the mailbox page.
pub struct Mail {
    pub smtp: Option<tetsu_mail::SmtpSettings>,
    pub authority: Option<std::path::PathBuf>,
    pub allow: tetsu_mail::Allowlist,
}

/// Reads the mail settings: with `TETSU_SMTP_HOST`, the server's implicit-TLS port from `TETSU_SMTP_PORT`, default
/// 465, its login from `TETSU_SMTP_USERNAME` and `TETSU_SMTP_PASSWORD`, and the sender from `TETSU_MAIL_FROM`, each
/// required; a certificate authority to trust besides the public ones, such as a local Mailpit's, from
/// `TETSU_SMTP_CA_FILE`; and the addresses that get real email, separated by commas, from `TETSU_MAIL_ALLOW`.
/// Without a host, every message is kept. Errors name the variable and never repeat its value, since one is a
/// password.
pub fn mail(var: impl Fn(&str) -> Result<String, VarError>) -> Result<Mail, String> {
    let allow = value(&var, "TETSU_MAIL_ALLOW")?
        .map_or(Some(tetsu_mail::Allowlist::default()), |text| {
            tetsu_mail::Allowlist::parse(&text)
        })
        .ok_or("TETSU_MAIL_ALLOW must be plain email addresses separated by commas")?;
    let smtp = value(&var, "TETSU_SMTP_HOST")?
        .map(|host| smtp_settings(&var, host))
        .transpose()?;
    let authority = value(&var, "TETSU_SMTP_CA_FILE")?.map(std::path::PathBuf::from);
    if authority.is_some() && smtp.is_none() {
        return Err("TETSU_SMTP_CA_FILE needs TETSU_SMTP_HOST".into());
    }
    Ok(Mail {
        smtp,
        authority,
        allow,
    })
}

/// The SMTP server at `host`, with the settings it requires.
fn smtp_settings(
    var: impl Fn(&str) -> Result<String, VarError>,
    host: String,
) -> Result<tetsu_mail::SmtpSettings, String> {
    let required =
        |name: &str| value(&var, name)?.ok_or(format!("{name} must be set with TETSU_SMTP_HOST"));
    let port = value(&var, "TETSU_SMTP_PORT")?
        .map_or(Some(465), |port| port.parse().ok().filter(|port| *port > 0))
        .ok_or("TETSU_SMTP_PORT must be a port number")?;
    let from = tetsu_kernel::Address::parse(&required("TETSU_MAIL_FROM")?)
        .ok_or("TETSU_MAIL_FROM must be a plain email address")?;
    Ok(tetsu_mail::SmtpSettings {
        host,
        port,
        username: required("TETSU_SMTP_USERNAME")?,
        password: required("TETSU_SMTP_PASSWORD")?,
        from,
        limit: SMTP_LIMIT,
    })
}

/// The SMTP adapter the mail settings name, if they name a server. A host TLS cannot check, or a certificate
/// authority file that cannot be read or holds none, is refused here, at start, by the variable's name and never
/// its value.
pub fn smtp(mail: &Mail) -> Result<Option<tetsu_mail::Smtp>, String> {
    let Some(settings) = &mail.smtp else {
        return Ok(None);
    };
    let made = match &mail.authority {
        Some(path) => {
            let pem =
                std::fs::read(path).map_err(|error| format!("TETSU_SMTP_CA_FILE: {error}"))?;
            tetsu_mail::Smtp::trusting(settings, &pem)
        }
        None => tetsu_mail::Smtp::new(settings),
    };
    made.map(Some).map_err(|refused| match refused {
        tetsu_mail::Refused::Host => "TETSU_SMTP_HOST must be a host name TLS can check".into(),
        tetsu_mail::Refused::Certificate => "TETSU_SMTP_CA_FILE must hold a PEM certificate".into(),
    })
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

    /// Settings from `.env` that go with the database are refused when the database is named elsewhere, so a
    /// stray file cannot turn off TLS for it, trust another authority, or move where its links point.
    #[test]
    fn a_stray_env_file_cannot_change_how_a_database_is_reached() {
        let lead = "DATABASE_URL";
        let followers = ["DATABASE_TLS", "DATABASE_CA_FILE"];
        for stray in ["DATABASE_TLS=disable\n", "DATABASE_CA_FILE=/tmp/evil.pem\n"] {
            let refused = Local::file_only(stray)
                .beside(lead, &followers)
                .unwrap_err();
            assert!(
                refused.contains("comes from .env but DATABASE_URL does not"),
                "{refused}"
            );
        }
        let both = "DATABASE_URL=postgres://localhost/tetsu\nDATABASE_TLS=disable\n";
        assert_eq!(Local::file_only(both).beside(lead, &followers), Ok(()));
        assert_eq!(Local::file_only("").beside(lead, &followers), Ok(()));
    }

    /// The variables `set` names, as the environment would give them.
    fn vars<'a>(set: &'a [(&str, &str)]) -> impl Fn(&str) -> Result<String, VarError> + 'a {
        |name| {
            set.iter()
                .find(|(set, _)| *set == name)
                .map(|(_, value)| (*value).to_string())
                .ok_or(VarError::NotPresent)
        }
    }

    fn mail_from(set: &[(&str, &str)]) -> Result<Mail, String> {
        mail(vars(set))
    }

    const SMTP: [(&str, &str); 4] = [
        ("TETSU_SMTP_HOST", "smtp.mailgun.org"),
        ("TETSU_SMTP_USERNAME", "postmaster@sandbox.mailgun.org"),
        ("TETSU_SMTP_PASSWORD", "a-secret-password"),
        ("TETSU_MAIL_FROM", "tetsu@sandbox.mailgun.org"),
    ];

    /// With no server, every message is kept, and the allowlist is still read.
    #[test]
    fn without_a_host_mail_is_only_kept() {
        let kept = mail_from(&[]).unwrap();
        assert!(kept.smtp.is_none());
        assert_eq!(kept.allow, tetsu_mail::Allowlist::default());
        let listed =
            mail_from(&[("TETSU_MAIL_ALLOW", "ada@example.com, lee@example.org")]).unwrap();
        assert_eq!(
            listed.allow,
            tetsu_mail::Allowlist::parse("ada@example.com,lee@example.org").unwrap()
        );
    }

    #[test]
    fn a_host_brings_its_port_login_and_sender() {
        let smtp = mail_from(&SMTP).unwrap().smtp.unwrap();
        assert_eq!(
            (
                smtp.host.as_str(),
                smtp.port,
                smtp.username.as_str(),
                smtp.password.as_str()
            ),
            (
                "smtp.mailgun.org",
                465,
                "postmaster@sandbox.mailgun.org",
                "a-secret-password"
            )
        );
        assert_eq!(smtp.from.as_str(), "tetsu@sandbox.mailgun.org");
        let mut other = SMTP.to_vec();
        other.push(("TETSU_SMTP_PORT", "2465"));
        assert_eq!(mail_from(&other).unwrap().smtp.unwrap().port, 2465);
    }

    /// A server needs its login and sender, and a missing one is named.
    #[test]
    fn missing_mail_settings_say_which() {
        for (name, refused) in [
            (
                "TETSU_SMTP_USERNAME",
                "TETSU_SMTP_USERNAME must be set with TETSU_SMTP_HOST",
            ),
            (
                "TETSU_SMTP_PASSWORD",
                "TETSU_SMTP_PASSWORD must be set with TETSU_SMTP_HOST",
            ),
            (
                "TETSU_MAIL_FROM",
                "TETSU_MAIL_FROM must be set with TETSU_SMTP_HOST",
            ),
        ] {
            let without: Vec<_> = SMTP.into_iter().filter(|(set, _)| *set != name).collect();
            assert_eq!(mail_from(&without).err(), Some(refused.to_string()));
        }
    }

    /// A malformed setting is named, and the error never repeats a value, since one is the password.
    #[test]
    fn malformed_mail_settings_say_which() {
        for (name, bad, refused) in [
            (
                "TETSU_SMTP_PORT",
                "0",
                "TETSU_SMTP_PORT must be a port number",
            ),
            (
                "TETSU_SMTP_PORT",
                "smtp",
                "TETSU_SMTP_PORT must be a port number",
            ),
            (
                "TETSU_SMTP_PORT",
                "70000",
                "TETSU_SMTP_PORT must be a port number",
            ),
            (
                "TETSU_MAIL_FROM",
                "Tetsu <a@b.c>",
                "TETSU_MAIL_FROM must be a plain email address",
            ),
            (
                "TETSU_MAIL_ALLOW",
                "ada@example.com;lee@example.org",
                "TETSU_MAIL_ALLOW must be plain email addresses separated by commas",
            ),
        ] {
            let mut set: Vec<_> = SMTP.into_iter().filter(|(set, _)| *set != name).collect();
            set.push((name, bad));
            let error = mail_from(&set).err().unwrap();
            assert_eq!(error, refused);
            assert!(!error.contains("a-secret-password"));
        }
    }

    /// A certificate authority for a local Mailpit is read with the server, and is no use without one.
    #[test]
    fn a_ca_file_goes_with_the_host() {
        let mut with = SMTP.to_vec();
        with.push(("TETSU_SMTP_CA_FILE", ".tools/mailpit/cert.pem"));
        assert_eq!(
            mail_from(&with).unwrap().authority,
            Some(std::path::PathBuf::from(".tools/mailpit/cert.pem"))
        );
        assert_eq!(mail_from(&SMTP).unwrap().authority, None);
        assert_eq!(
            mail_from(&[("TETSU_SMTP_CA_FILE", "cert.pem")]).err(),
            Some("TETSU_SMTP_CA_FILE needs TETSU_SMTP_HOST".to_string())
        );
    }

    /// A certificate for `localhost`, made by `openssl` as the Mailpit tests make theirs, in `folder`.
    fn certificate(folder: &Path) -> std::path::PathBuf {
        let cert = folder.join("cert.pem");
        let made = std::process::Command::new("openssl")
            .args([
                "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
            ])
            .args([
                "-subj",
                "/CN=localhost",
                "-addext",
                "subjectAltName=DNS:localhost",
            ])
            .arg("-keyout")
            .arg(folder.join("key.pem"))
            .arg("-out")
            .arg(&cert)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(made.success());
        cert
    }

    /// The adapter trusts a readable certificate authority, and each setting that stops it is named.
    #[test]
    fn the_ca_file_is_read_and_checked_at_start() {
        let folder = tetsu_postgres::test_support::Scratch::folder("smtp-ca").unwrap();
        let cert = certificate(&folder);
        let made = |host: &str, ca: Option<&Path>| {
            let mut set: Vec<_> = SMTP
                .into_iter()
                .filter(|(name, _)| *name != "TETSU_SMTP_HOST")
                .collect();
            set.push(("TETSU_SMTP_HOST", host));
            let ca = ca.map(|path| path.to_str().unwrap().to_string());
            if let Some(ca) = &ca {
                set.push(("TETSU_SMTP_CA_FILE", ca));
            }
            smtp(&mail_from(&set).unwrap()).map(|adapter| adapter.is_some())
        };
        assert_eq!(made("localhost", Some(&cert)), Ok(true));
        assert_eq!(made("localhost", None), Ok(true));
        assert_eq!(
            smtp(&mail_from(&[]).unwrap()).map(|adapter| adapter.is_some()),
            Ok(false)
        );
        let missing = made("localhost", Some(&folder.join("none.pem"))).unwrap_err();
        assert!(missing.starts_with("TETSU_SMTP_CA_FILE: "), "{missing}");
        std::fs::write(folder.join("empty.pem"), "not a certificate").unwrap();
        assert_eq!(
            made("localhost", Some(&folder.join("empty.pem"))),
            Err("TETSU_SMTP_CA_FILE must hold a PEM certificate".to_string())
        );
        for ca in [None, Some(cert.as_path())] {
            assert_eq!(
                made("not a host", ca),
                Err("TETSU_SMTP_HOST must be a host name TLS can check".to_string())
            );
        }
    }

    fn listening(port: Option<&str>) -> Result<SocketAddr, String> {
        listen(|name| match (name, port) {
            ("PORT", Some(port)) => Ok(port.to_string()),
            _ => Err(VarError::NotPresent),
        })
    }

    fn origin_from(value: Option<&str>) -> Result<String, String> {
        origin(|name| match (name, value) {
            ("TETSU_ORIGIN", Some(value)) => Ok(value.to_string()),
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
    fn the_origin_is_written_as_a_browser_sends_it() {
        for good in ["https://tetsu-demo.onrender.com", "http://localhost:8080"] {
            assert_eq!(origin_from(Some(good)), Ok(good.to_string()));
        }
        assert_eq!(origin_from(None), Err("TETSU_ORIGIN must be set".into()));
        for bad in [
            "tetsu-demo.onrender.com",
            "https://tetsu-demo.onrender.com/",
            "http://tetsu-demo.onrender.com",
        ] {
            assert!(
                origin_from(Some(bad))
                    .is_err_and(|why| why.starts_with("TETSU_ORIGIN must be an origin")),
                "{bad}"
            );
        }
    }

    fn formatted(value: Option<&str>, terminal: bool) -> Result<LogFormat, String> {
        log_format(
            |name| match (name, value) {
                ("TETSU_LOG_FORMAT", Some(value)) => Ok(value.to_string()),
                _ => Err(VarError::NotPresent),
            },
            terminal,
        )
    }

    /// Unset, logs are text a person reads when they reach a terminal, and JSON anywhere else, so a host that
    /// collects standard output always gets JSON. The setting chooses either.
    #[test]
    fn logs_are_text_in_a_terminal_and_json_elsewhere_unless_set() {
        assert_eq!(formatted(None, true), Ok(LogFormat::Text));
        assert_eq!(formatted(None, false), Ok(LogFormat::Json));
        assert_eq!(formatted(Some(""), false), Ok(LogFormat::Json));
        for terminal in [true, false] {
            assert_eq!(formatted(Some(" json "), terminal), Ok(LogFormat::Json));
            assert_eq!(formatted(Some("text"), terminal), Ok(LogFormat::Text));
            assert_eq!(
                formatted(Some("JSON"), terminal),
                Err("TETSU_LOG_FORMAT must be json or text".into())
            );
        }
    }

    fn counted(value: Option<&str>) -> Result<usize, String> {
        proxies(|name| match (name, value) {
            ("TETSU_TRUSTED_PROXIES", Some(value)) => Ok(value.to_string()),
            _ => Err(VarError::NotPresent),
        })
    }

    #[test]
    fn the_proxy_count_must_be_set_from_0_to_10() {
        for (text, count) in [("0", 0), (" 3 ", 3), ("10", 10)] {
            assert_eq!(counted(Some(text)), Ok(count), "{text}");
        }
        for bad in [
            None,
            Some(""),
            Some("11"),
            Some("-1"),
            Some("three"),
            Some("3.0"),
        ] {
            assert_eq!(
                counted(bad),
                Err(
                    "TETSU_TRUSTED_PROXIES must be set to the number of proxies in front, 0 to 10"
                        .into()
                ),
                "{bad:?}"
            );
        }
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
        export(vars(set))
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

    #[test]
    fn a_local_file_holds_settings_comments_and_blank_lines() {
        let local = Local {
            file: parse(
                "# Local defaults\n\nDATABASE_URL = postgres://localhost/tetsu_local\nTETSU_DEMO=true\nEMPTY=\nA_1=x=y\n",
                ".env",
            )
            .unwrap(),
            environment: true,
            name: ".env".into(),
        };
        let missing = || Err(VarError::NotPresent);
        assert_eq!(
            local.or(missing(), "DATABASE_URL"),
            Ok("postgres://localhost/tetsu_local".into())
        );
        assert_eq!(local.or(missing(), "TETSU_DEMO"), Ok("true".into()));
        assert_eq!(local.or(missing(), "EMPTY"), Ok(String::new()));
        assert_eq!(local.or(missing(), "A_1"), Ok("x=y".into()));
        assert_eq!(local.or(missing(), "PORT"), Err(VarError::NotPresent));
    }

    /// The environment wins, whether it holds a value or one that is not UTF-8.
    #[test]
    fn the_environment_wins_over_the_file() {
        let local = Local {
            file: parse("PORT=9000", ".env").unwrap(),
            environment: true,
            name: ".env".into(),
        };
        assert_eq!(local.or(Ok("8080".into()), "PORT"), Ok("8080".into()));
        assert_eq!(
            local.or(Err(VarError::NotUnicode("x".into())), "PORT"),
            Err(VarError::NotUnicode("x".into()))
        );
    }

    /// A value may be a password, so an error names the line and not its text.
    #[test]
    fn a_line_that_is_not_a_setting_is_named_by_number() {
        for (text, line) in [
            ("A=1\nexport B=2", 2),
            ("pass word=s3cret", 1),
            ("=value", 1),
            ("1A=x", 1),
            ("A B=x", 1),
        ] {
            assert_eq!(
                parse(text, ".env.demo"),
                Err(format!(".env.demo line {line} is not NAME=value")),
                "{text}"
            );
        }
    }

    #[test]
    fn a_folder_without_a_file_has_no_local_settings() {
        let folder = tetsu_postgres::test_support::Scratch::folder("no-env").unwrap();
        // No file's settings, yet still the environment's, unlike a default, which reads neither.
        assert_eq!(
            Local::load(&folder, &Source::Local),
            Ok(Local {
                file: HashMap::new(),
                environment: true,
                name: ".env".into(),
            })
        );
        std::fs::write(folder.join(".env"), "TETSU_DEMO=false\n").unwrap();
        assert_eq!(
            Local::load(&folder, &Source::Local)
                .unwrap()
                .or(Err(VarError::NotPresent), "TETSU_DEMO"),
            Ok("false".into())
        );
    }

    #[test]
    fn a_file_that_cannot_be_read_is_an_error() {
        let folder = tetsu_postgres::test_support::Scratch::folder("bad-env").unwrap();
        std::fs::create_dir_all(folder.join(".env")).unwrap();
        let error = Local::load(&folder, &Source::Local).unwrap_err();
        assert!(error.starts_with(".env could not be read: "), "{error}");
    }

    fn words(words: &[&str]) -> Vec<String> {
        words.iter().map(ToString::to_string).collect()
    }

    /// `--env <name>` first chooses `.env.<name>`, and the command follows it; anything else leaves `.env` chosen.
    #[test]
    fn a_leading_env_chooses_an_environment_s_file() {
        let args = words(&["--env", "demo-2", "admin", "list"]);
        let (source, rest) = Source::chosen(&args).unwrap();
        assert_eq!(source, Source::Named("demo-2".into()));
        assert_eq!(source.file(), ".env.demo-2");
        assert_eq!(rest, &args[2..]);
        for args in [words(&[]), words(&["admin", "--env", "demo"])] {
            let (source, rest) = Source::chosen(&args).unwrap();
            assert_eq!((source.file(), rest), (".env".into(), &args[..]));
        }
    }

    /// A name cannot reach outside the folder, look like an option, or be missing.
    #[test]
    fn an_environment_s_name_is_lowercase_letters_digits_and_hyphens() {
        for bad in [
            &["--env"][..],
            &["--env", ""],
            &["--env", "../prod"],
            &["--env", "Demo"],
            &["--env", "demo.x"],
            &["--env", "2demo"],
            &["--env", "-x"],
            &["--env", "demo_x"],
        ] {
            let refused = Source::chosen(&words(bad)).unwrap_err();
            assert!(
                refused.starts_with("--env takes an environment's name"),
                "{bad:?}"
            );
        }
    }

    /// An environment's file must exist, and holds settings as `.env` does; a stray-setting error names it.
    #[test]
    fn an_environment_s_file_must_exist() {
        let folder = tetsu_postgres::test_support::Scratch::folder("named-env").unwrap();
        let demo = Source::Named("demo".into());
        let missing = Local::load(&folder, &demo).unwrap_err();
        assert!(
            missing.starts_with(".env.demo could not be read: "),
            "{missing}"
        );
        std::fs::write(folder.join(".env.demo"), "TETSU_TEST_FOLLOWER=x\n").unwrap();
        let local = Local::load(&folder, &demo).unwrap();
        assert_eq!(
            local.or(Err(VarError::NotPresent), "TETSU_TEST_FOLLOWER"),
            Ok("x".into())
        );
        let refused = local
            .beside("TETSU_TEST_LEAD", &["TETSU_TEST_FOLLOWER"])
            .unwrap_err();
        assert!(refused.contains("comes from .env.demo but"), "{refused}");
        std::fs::write(folder.join(".env.demo"), "not a setting\n").unwrap();
        assert_eq!(
            Local::load(&folder, &demo),
            Err(".env.demo line 1 is not NAME=value".into())
        );
    }
}
