//! Pools and direct connections.

use std::time::Duration;

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::config::SslMode;
use tokio_postgres::tls::MakeTlsConnect;
use tokio_postgres::{Client, Config, NoTls, Socket};
use tokio_postgres_rustls::MakeRustlsConnect;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

const POOL_SIZE: usize = 8;
const POOL_WAIT: Duration = Duration::from_secs(2);
const POOL_CREATE: Duration = Duration::from_secs(5);
const STATEMENT_TIMEOUT: &str = "-c statement_timeout=5000";

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
        }
    }

    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    #[must_use]
    pub fn tls(&self) -> Tls {
        self.tls
    }

    fn config(&self) -> Result<Config, Error> {
        let mut config: Config = self.url.parse()?;
        config.ssl_mode(match self.tls {
            Tls::Required => SslMode::Require,
            Tls::Disabled => SslMode::Disable,
        });
        Ok(config)
    }
}

fn rustls_connector() -> MakeRustlsConnect {
    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    MakeRustlsConnect::new(config)
}

/// A pool for serving requests, with the timeouts ADR-0011 requires.
///
/// # Errors
/// Fails if the URL is invalid.
pub fn pool(settings: &Settings) -> Result<Pool, Error> {
    let mut config = settings.config()?;
    config.options(STATEMENT_TIMEOUT);
    let recycling = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };
    let manager = match settings.tls {
        Tls::Required => Manager::from_config(config, rustls_connector(), recycling),
        Tls::Disabled => Manager::from_config(config, NoTls, recycling),
    };
    let pool = Pool::builder(manager)
        .max_size(POOL_SIZE)
        .runtime(Runtime::Tokio1)
        .wait_timeout(Some(POOL_WAIT))
        .create_timeout(Some(POOL_CREATE))
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
        Tls::Required => connect_with(&config, rustls_connector()).await,
        Tls::Disabled => connect_with(&config, NoTls).await,
    }
}

async fn connect_with<T>(config: &Config, tls: T) -> Result<Client, Error>
where
    T: MakeTlsConnect<Socket>,
    T::Stream: Send + 'static,
{
    let (client, connection) = tokio::time::timeout(POOL_CREATE, config.connect(tls))
        .await
        .map_err(|_| "Postgres did not answer in time")??;
    tokio::spawn(connection);
    Ok(client)
}
