//! Fresh databases for tests. `TEST_DATABASE_URL` points at a Postgres the tests may create databases on.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::connection::{Error, Settings, connect};

static NEXT: AtomicU32 = AtomicU32::new(0);

/// Creates an empty database. Returns an error, rather than skipping, when Postgres is missing.
///
/// # Errors
/// Fails if `TEST_DATABASE_URL` is unset or Postgres cannot be reached.
pub async fn fresh_database() -> Result<Settings, Error> {
    let admin_url =
        std::env::var("TEST_DATABASE_URL").map_err(|_| "TEST_DATABASE_URL must be set")?;
    let admin = connect(&Settings::from_values(&admin_url, Some("disable"))).await?;
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.subsec_nanos();
    let name = format!(
        "t_{}_{}_{nanos}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    admin
        .batch_execute(&format!("CREATE DATABASE {name}"))
        .await?;
    let (base, _) = admin_url
        .rsplit_once('/')
        .ok_or("TEST_DATABASE_URL must name a database")?;
    Ok(Settings::from_values(
        &format!("{base}/{name}"),
        Some("disable"),
    ))
}
