//! Pools and direct connections.

use std::path::Path;
use std::time::Duration;

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod, Runtime};
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;
use tokio_postgres::config::SslMode;
use tokio_postgres::tls::MakeTlsConnect;
use tokio_postgres::{Client, Config, NoTls, Socket};
use tokio_postgres_rustls::MakeRustlsConnect;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

const POOL_SIZE: usize = 8;
/// How long a request waits for a free pooled connection before failing fast (ADR-0011).
const POOL_WAIT: Duration = Duration::from_secs(2);
/// How long opening a connection may take, which also covers a server that accepts and then stalls.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const STATEMENT_TIMEOUT: &str = "-c statement_timeout=5000";
/// How long sent data may go unacknowledged, and how soon a quiet connection is probed, so a network that stalls
/// mid-query ends the connection within about half a minute, not the two hours of the system's keepalive default
/// (ADR-0060). A closed connection is never handed out again.
const TCP_USER_TIMEOUT: Duration = Duration::from_secs(15);
const KEEPALIVE_IDLE: Duration = Duration::from_secs(10);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(5);
const KEEPALIVE_RETRIES: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tls {
    Required,
    Disabled,
}

/// How to reach Postgres. TLS is required unless a local test turns it off by name.
#[derive(Clone)]
pub struct Settings {
    url: String,
    tls: Tls,
    /// The authorities from a CA file, trusted instead of the public roots (ADR-0013, ASVS `v5.0.0-12.3.4`).
    authorities: Option<rustls::RootCertStore>,
    connect_timeout: Duration,
    pool_wait: Duration,
}

impl std::fmt::Debug for Settings {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Settings")
            .field("tls", &self.tls)
            .finish_non_exhaustive()
    }
}

impl Settings {
    #[must_use]
    pub fn new(url: &str) -> Self {
        Self::from_values(url, None)
    }

    /// Reads `tls` as set in the environment: only `disable` turns it off.
    #[must_use]
    pub fn from_values(url: &str, tls: Option<&str>) -> Self {
        let tls = if tls == Some("disable") {
            Tls::Disabled
        } else {
            Tls::Required
        };
        Self {
            url: url.to_string(),
            tls,
            authorities: None,
            connect_timeout: CONNECT_TIMEOUT,
            pool_wait: POOL_WAIT,
        }
    }

    /// Replaces the connect timeout and the pool wait, so tests that wait them out need not take seconds.
    #[must_use]
    pub fn with_timeouts(mut self, connect: Duration, pool_wait: Duration) -> Self {
        self.connect_timeout = connect;
        self.pool_wait = pool_wait;
        self
    }

    /// Trusts only the certificate authorities in a PEM file, such as a provider's own, instead of the public roots
    /// (ADR-0013, ASVS `v5.0.0-12.3.4`). Verification stays on: a certificate must chain to one of them and name
    /// the host.
    ///
    /// # Errors
    /// Fails if the file cannot be read, holds no PEM certificate, or holds one that cannot be an authority. No
    /// error quotes the file.
    pub fn with_ca_file(mut self, path: &Path) -> Result<Self, Error> {
        let pem = std::fs::read(path)
            .map_err(|error| format!("the CA file could not be read: {error}"))?;
        let authorities = CertificateDer::pem_slice_iter(&pem)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "the CA file is not valid PEM")?;
        if authorities.is_empty() {
            return Err("the CA file holds no PEM certificate".into());
        }
        let mut store = rustls::RootCertStore::empty();
        let (_, unusable) = store.add_parsable_certificates(authorities);
        if unusable > 0 {
            return Err("the CA file holds a certificate that cannot be an authority".into());
        }
        self.authorities = Some(store);
        Ok(self)
    }

    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    #[must_use]
    pub fn tls(&self) -> Tls {
        self.tls
    }

    /// Checks the URL without connecting, so bad settings fail before any work starts.
    ///
    /// # Errors
    /// Fails if the URL is not a valid connection string, or sets its own TLS mode.
    pub fn check(&self) -> Result<(), Error> {
        self.config().map(|_| ())
    }

    fn config(&self) -> Result<Config, Error> {
        // The driver splits at the first `@`, before anything else, so the rest of a password holding one would
        // reach the server, and its errors, as the host or database name.
        if let Some((_, rest)) = self.url.split_once("://")
            && rest.matches('@').count() > 1
        {
            return Err(
                "the database URL holds more than one @; write any @ in the password as %40".into(),
            );
        }
        // The parser's error can quote part of the URL, such as an option name split off a password, so it
        // is replaced rather than passed on with its cause.
        let mut config: Config = self
            .url
            .parse()
            .map_err(|_| "the database URL is not a valid connection string")?;
        // The URL's own mode would be overridden below without a word, so it is refused instead. The parsed mode
        // catches what the driver decoded; the keys catch an explicit `prefer`, which parses as the default.
        if config.get_ssl_mode() != SslMode::Prefer || sets_sslmode(&self.url) {
            return Err("the database URL must not set sslmode; TLS is set separately".into());
        }
        // A trailing backslash would escape the space before the timeout the pool adds to these options.
        if config
            .get_options()
            .is_some_and(|options| options.ends_with('\\'))
        {
            return Err("the database URL's options must not end in a backslash".into());
        }
        config.ssl_mode(match self.tls {
            Tls::Required => SslMode::Require,
            Tls::Disabled => SslMode::Disable,
        });
        config
            .tcp_user_timeout(TCP_USER_TIMEOUT)
            .keepalives(true)
            .keepalives_idle(KEEPALIVE_IDLE)
            .keepalives_interval(KEEPALIVE_INTERVAL)
            .keepalives_retries(KEEPALIVE_RETRIES);
        Ok(config)
    }
}

/// Whether the URL names `sslmode` as a key: in the query of a `postgres://` URL, where keys may be
/// percent-encoded, or as a `key=value` pair otherwise.
fn sets_sslmode(url: &str) -> bool {
    if let Some((_, rest)) = url.split_once("://") {
        let query = rest.split_once('?').map_or("", |(_, query)| query);
        query
            .split('&')
            .any(|pair| decoded(pair.split('=').next().unwrap_or_default()) == "sslmode")
    } else {
        url.split_whitespace()
            .any(|word| word.split('=').next() == Some("sslmode"))
    }
}

/// Percent-decodes text, keeping any escape that is not two hex digits as written.
fn decoded(text: &str) -> String {
    let mut out = Vec::with_capacity(text.len());
    let mut rest = text.as_bytes();
    while let Some((&first, tail)) = rest.split_first() {
        let escaped = tail
            .get(..2)
            .filter(|_| first == b'%')
            .and_then(|pair| std::str::from_utf8(pair).ok())
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        if let Some(byte) = escaped {
            out.push(byte);
            rest = tail.get(2..).unwrap_or_default();
        } else {
            out.push(first);
            rest = tail;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl Settings {
    /// The authorities from the CA file, or the public roots when there is none.
    fn roots(&self) -> rustls::RootCertStore {
        self.authorities.clone().unwrap_or_else(|| {
            rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned())
        })
    }

    fn rustls_connector(&self) -> MakeRustlsConnect {
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(self.roots())
            .with_no_client_auth();
        MakeRustlsConnect::new(config)
    }
}

/// A pool for serving requests, with the timeouts ADR-0011 requires.
///
/// # Errors
/// Fails if the URL is invalid.
pub fn pool(settings: &Settings) -> Result<Pool, Error> {
    let mut config = settings.config()?;
    // Added to the URL's own options, which setting them alone would replace.
    let options = config.get_options().map_or_else(
        || STATEMENT_TIMEOUT.to_string(),
        |own| format!("{own} {STATEMENT_TIMEOUT}"),
    );
    config.options(&options);
    let recycling = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };
    let manager = match settings.tls {
        Tls::Required => Manager::from_config(config, settings.rustls_connector(), recycling),
        Tls::Disabled => Manager::from_config(config, NoTls, recycling),
    };
    let pool = Pool::builder(manager)
        .max_size(POOL_SIZE)
        .runtime(Runtime::Tokio1)
        .wait_timeout(Some(settings.pool_wait))
        .create_timeout(Some(settings.connect_timeout))
        .build()?;
    Ok(pool)
}

/// A direct connection, for work that needs a session such as the migration lock.
///
/// # Errors
/// Fails if Postgres cannot be reached, or will not use TLS when it is required.
pub async fn connect(settings: &Settings) -> Result<Client, Error> {
    let config = settings.config()?;
    match settings.tls {
        Tls::Required => {
            connect_with(
                &config,
                settings.rustls_connector(),
                settings.connect_timeout,
            )
            .await
        }
        Tls::Disabled => connect_with(&config, NoTls, settings.connect_timeout).await,
    }
}

async fn connect_with<T>(config: &Config, tls: T, timeout: Duration) -> Result<Client, Error>
where
    T: MakeTlsConnect<Socket>,
    T::Stream: Send + 'static,
{
    let (client, connection) = tokio::time::timeout(timeout, config.connect(tls))
        .await
        .map_err(|_| "Postgres did not answer in time")??;
    tokio::spawn(connection);
    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_timeouts_are_five_seconds_to_connect_and_two_to_wait() {
        let settings = Settings::new("postgres://h/db");
        assert_eq!(settings.connect_timeout, Duration::from_secs(5));
        assert_eq!(settings.pool_wait, Duration::from_secs(2));
        let short = settings.with_timeouts(Duration::from_millis(1), Duration::from_millis(2));
        assert_eq!(
            (short.connect_timeout, short.pool_wait),
            (Duration::from_millis(1), Duration::from_millis(2))
        );
    }

    /// A stalled network ends the connection within about half a minute, whatever the URL asked (ADR-0060).
    #[test]
    fn every_connection_gives_up_on_a_stalled_network() {
        let config = Settings::new("postgres://h/db?keepalives=0&keepalives_idle=7200")
            .config()
            .expect("a config");
        assert_eq!(
            config.get_tcp_user_timeout(),
            Some(&Duration::from_secs(15))
        );
        assert!(config.get_keepalives());
        assert_eq!(config.get_keepalives_idle(), Duration::from_secs(10));
        assert_eq!(
            config.get_keepalives_interval(),
            Some(Duration::from_secs(5))
        );
        assert_eq!(config.get_keepalives_retries(), Some(3));
    }

    #[test]
    fn percent_escapes_decode_and_anything_else_stays() {
        assert_eq!(decoded("ss%6Cmode"), "sslmode");
        assert_eq!(decoded("%41%42c"), "ABc");
        for kept in ["%zz", "%4", "%", "a%", "plain"] {
            assert_eq!(decoded(kept), kept);
        }
    }

    #[test]
    fn sslmode_is_found_only_as_a_key() {
        for url in [
            "postgres://h/db?sslmode=prefer",
            "postgres://h/db?a=1&sslmode=prefer",
            "postgres://h/db?ss%6Cmode=disable",
            "host=h sslmode=prefer",
            "host=h sslmode = prefer",
        ] {
            assert!(sets_sslmode(url), "{url}");
        }
        for url in [
            "postgres://u:sslmode@h/sslmode",
            "postgres://h/db?application_name=sslmode",
            "postgres://h/db",
            "host=h password=sslmode",
        ] {
            assert!(!sets_sslmode(url), "{url}");
        }
    }

    fn written(name: &str, contents: &str) -> crate::test_support::Scratch {
        crate::test_support::Scratch::file(name, contents).unwrap()
    }

    #[test]
    fn a_ca_file_that_cannot_be_read_fails_without_quoting_it() {
        let missing = std::env::temp_dir().join("tetsu-no-such-ca.pem");
        let error = Settings::new("postgres://h/db")
            .with_ca_file(&missing)
            .unwrap_err()
            .to_string();
        assert!(
            error.starts_with("the CA file could not be read"),
            "{error}"
        );

        let text = written("not-a-certificate.pem", "fake-secret-not-a-certificate");
        let error = Settings::new("postgres://h/db")
            .with_ca_file(&text)
            .unwrap_err()
            .to_string();
        assert_eq!(error, "the CA file holds no PEM certificate");
        assert!(!error.contains("fake-secret"), "{error}");
    }

    #[test]
    fn a_ca_file_must_be_whole_pem_holding_real_certificates() {
        for (name, contents, expected) in [
            (
                "unterminated.pem",
                "-----BEGIN CERTIFICATE-----\nAAAA\n",
                "the CA file is not valid PEM",
            ),
            (
                "not-der.pem",
                "-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n",
                "the CA file holds a certificate that cannot be an authority",
            ),
        ] {
            let path = written(name, contents);
            let error = Settings::new("postgres://h/db")
                .with_ca_file(&path)
                .unwrap_err()
                .to_string();
            assert_eq!(error, expected, "{name}");
        }
    }

    #[test]
    fn without_a_ca_file_the_public_roots_are_trusted() {
        assert_eq!(
            Settings::new("postgres://h/db").roots().len(),
            webpki_roots::TLS_SERVER_ROOTS.len()
        );
    }

    /// The test Postgres's own certificate, read over SQL so none is committed, stands in for a provider's.
    #[tokio::test]
    async fn a_ca_file_replaces_the_public_roots() {
        let database = crate::test_support::fresh_database().await.unwrap();
        let client = connect(&database).await.unwrap();
        let pem: String = client
            .query_one("SELECT pg_read_file(current_setting('ssl_cert_file'))", &[])
            .await
            .unwrap()
            .try_get(0)
            .unwrap();
        let path = written("server.pem", &pem);
        let settings = Settings::new("postgres://h/db")
            .with_ca_file(&path)
            .unwrap();
        assert_eq!(settings.roots().len(), 1);
    }
}
