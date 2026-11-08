//! Fresh databases for tests. `TEST_DATABASE_URL` points at a Postgres the tests may create databases on.
//!
//! Every database is dropped when its test is done with it, including when the test fails or panics. One
//! left by a run that was killed is swept by a later run (#43).

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::connection::{Error, Settings, connect};

static NEXT: AtomicU32 = AtomicU32::new(0);
/// Held while sweeping, so test binaries running at once do not each repeat the same drops.
pub const SWEEP_LOCK: i64 = 7_243_002;
/// Older than any test run, so a database this old belongs to a run that is gone.
const STALE: Duration = Duration::from_hours(1);

/// A database for one test. It derefs to its [`Settings`], and is dropped with it.
pub struct TestDatabase {
    settings: Settings,
    name: String,
    admin_url: String,
}

impl TestDatabase {
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl std::ops::Deref for TestDatabase {
    type Target = Settings;

    fn deref(&self) -> &Settings {
        &self.settings
    }
}

impl Drop for TestDatabase {
    /// Drop cannot wait on the runtime it runs inside, so the database is dropped from a thread with its own.
    fn drop(&mut self) {
        let (name, admin_url) = (self.name.clone(), self.admin_url.clone());
        let dropped = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            runtime.block_on(drop_database(&admin_url, &name))
        })
        .join();
        if let Some(warning) = not_dropped(&self.name, matches!(dropped, Ok(Ok(())))) {
            eprintln!("{warning}");
        }
    }
}

fn not_dropped(name: &str, dropped: bool) -> Option<String> {
    (!dropped)
        .then(|| format!("warning: test database {name} was not dropped; a later run sweeps it"))
}

/// Creates an empty database. Returns an error, rather than skipping, when Postgres is missing.
///
/// # Errors
/// Fails if `TEST_DATABASE_URL` is unset or Postgres cannot be reached.
pub async fn fresh_database() -> Result<TestDatabase, Error> {
    create("").await
}

/// [`fresh_database`], storing text in another encoding, such as `LATIN1`.
///
/// # Errors
/// As [`fresh_database`], or if the encoding is unknown.
pub async fn fresh_database_encoded(encoding: &str) -> Result<TestDatabase, Error> {
    if !encoding
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(format!("encoding {encoding:?} must be a plain name").into());
    }
    create(&format!(
        " ENCODING {encoding} TEMPLATE template0 LC_COLLATE \"C\" LC_CTYPE \"C\""
    ))
    .await
}

/// Drops every test database older than an hour, which no running test can still be using. If another sweep
/// is already running, this one leaves the work to it. A database that will not drop is reported and skipped,
/// so it never blocks the tests that follow.
///
/// # Errors
/// Fails if `TEST_DATABASE_URL` is unset or Postgres cannot be reached.
pub async fn sweep_stale_databases() -> Result<(), Error> {
    let admin = connect(&Settings::from_values(&admin_url()?, Some("disable"))).await?;
    let taken: bool = admin
        .query_one("SELECT pg_try_advisory_lock($1) AS taken", &[&SWEEP_LOCK])
        .await?
        .try_get("taken")?;
    if !taken {
        return Ok(());
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    // Every name is checked in Rust, since only a test database's name parses as a creation time.
    for row in admin.query("SELECT datname FROM pg_database", &[]).await? {
        // A name that is not UTF-8, such as one made in a LATIN1 database, is never one of ours, so skip it.
        let Ok(name) = row.try_get::<_, String>("datname") else {
            continue;
        };
        if stale(&name, now)
            && let Err(error) = admin.batch_execute(&drop_statement(&name)).await
        {
            eprintln!("warning: stale test database {name} was not dropped: {error}");
        }
    }
    // Closing the connection releases the lock.
    Ok(())
}

/// Whether a name is exactly `t_<seconds>_<process>_<counter>`, as [`create`] makes them, and was created more
/// than an hour before `now`. Any other name, such as `t_2024_backup`, is never a test database.
fn stale(name: &str, now: u64) -> bool {
    let parts: Vec<&str> = name
        .strip_prefix("t_")
        .unwrap_or_default()
        .split('_')
        .collect();
    let well_formed = parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
    let created = parts
        .first()
        .and_then(|seconds| seconds.parse::<u64>().ok());
    well_formed && created.is_some_and(|at| now.saturating_sub(at) > STALE.as_secs())
}

async fn create(options: &str) -> Result<TestDatabase, Error> {
    // Every time, not once per run: it is one quick query, and a killed run's databases go soon after.
    sweep_stale_databases().await?;
    let admin_url = admin_url()?;
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let name = format!(
        "t_{seconds}_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    // Worked out first, so a URL the helper cannot use fails before a database exists to leak.
    let settings = Settings::from_values(&database_url(&admin_url, &name)?, Some("disable"));
    let admin = connect(&Settings::from_values(&admin_url, Some("disable"))).await?;
    admin
        .batch_execute(&format!("CREATE DATABASE {}{options}", quoted(&name)))
        .await?;
    Ok(TestDatabase {
        settings,
        name,
        admin_url,
    })
}

/// The admin URL with its database swapped for `name`. Only a `postgres://` URL that names a database works,
/// and a query after it is kept, slashes and all.
fn database_url(admin_url: &str, name: &str) -> Result<String, Error> {
    let unusable = "TEST_DATABASE_URL must be a postgres:// URL that names a database";
    let (address, query) = admin_url
        .split_once('?')
        .map_or((admin_url, None), |(address, query)| (address, Some(query)));
    let (base, database) = address.rsplit_once('/').ok_or(unusable)?;
    if !base.contains("://") || database.is_empty() {
        return Err(unusable.into());
    }
    Ok(match query {
        Some(query) => format!("{base}/{name}?{query}"),
        None => format!("{base}/{name}"),
    })
}

fn admin_url() -> Result<String, Error> {
    Ok(std::env::var("TEST_DATABASE_URL").map_err(|_| "TEST_DATABASE_URL must be set")?)
}

async fn drop_database(admin_url: &str, name: &str) -> Result<(), Error> {
    let admin = connect(&Settings::from_values(admin_url, Some("disable"))).await?;
    admin.batch_execute(&drop_statement(name)).await?;
    Ok(())
}

fn drop_statement(name: &str) -> String {
    format!("DROP DATABASE IF EXISTS {} WITH (FORCE)", quoted(name))
}

/// A name as a quoted identifier, so no name can end the statement or change its case.
fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_test_database_is_stale_only_after_an_hour() {
        let now = 1_800_000_000;
        assert!(!stale(&format!("t_{now}_1_0"), now));
        assert!(!stale(&format!("t_{}_1_0", now - 3600), now));
        assert!(stale(&format!("t_{}_1_0", now - 3601), now));
        assert!(!stale(&format!("t_{}_1_0", now + 60), now));
    }

    #[test]
    fn only_test_database_names_are_ever_stale() {
        for name in [
            "postgres",
            "template1",
            "tetsu",
            "t_",
            "t_abc_1_0",
            "tx_1_1_0",
            "d2a",
            "t_2024_backup",
            "t_1_2",
            "t_1_2_3_4",
            "t_1_x_3",
            "T_1_2_3",
            "t_1_2_3x",
            "t_1_2_",
            "t_7_x y",
        ] {
            assert!(!stale(name, u64::MAX), "{name}");
        }
    }

    #[test]
    fn the_database_url_is_checked_before_anything_is_created() {
        assert_eq!(
            database_url("postgres://h:5432/postgres", "t_1_2_3").unwrap(),
            "postgres://h:5432/t_1_2_3"
        );
        assert_eq!(
            database_url("postgres://u@h/postgres?application_name=a/b", "t_1_2_3").unwrap(),
            "postgres://u@h/t_1_2_3?application_name=a/b"
        );
        for admin in [
            "host=h dbname=postgres",
            "postgres",
            "postgres://h:5432",
            "postgres://h:5432?application_name=a/b",
        ] {
            assert!(database_url(admin, "t_1_2_3").is_err(), "{admin}");
        }
    }

    #[test]
    fn a_failed_drop_says_so_and_a_good_one_is_quiet() {
        assert_eq!(not_dropped("t_1_2_3", true), None);
        assert_eq!(
            not_dropped("t_1_2_3", false).as_deref(),
            Some("warning: test database t_1_2_3 was not dropped; a later run sweeps it")
        );
    }
}
