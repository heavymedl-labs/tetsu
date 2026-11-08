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
/// Far longer than any one test holds its database, which is seconds, so one this old belongs to a run that is gone.
const STALE: Duration = Duration::from_mins(10);
/// How long the final sweep waits for a running one to finish before failing.
const FINAL_WAIT: Duration = Duration::from_secs(30);

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
    // Written without a character literal, which lizard misreads here and then counts the rest of the file as
    // part of this function.
    let plain_name = encoding
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "_".contains(c));
    if !plain_name {
        return Err(format!("encoding {encoding:?} must be a plain name").into());
    }
    create(&format!(
        " ENCODING {encoding} TEMPLATE template0 LC_COLLATE \"C\" LC_CTYPE \"C\""
    ))
    .await
}

/// Drops every test database over ten minutes old, which no running test can still be using. If another sweep
/// is already running, this one leaves the work to it. A database that will not drop is reported and skipped,
/// so it never blocks the tests that follow.
///
/// # Errors
/// Fails if `TEST_DATABASE_URL` is unset or Postgres cannot be reached.
pub async fn sweep_stale_databases() -> Result<(), Error> {
    sweep(STALE, None).await
}

/// Drops every test database, however new. Only for when no tests are running, such as at the end of
/// `scripts/check.sh`, since it takes databases from under any test still using them. It waits for a sweep
/// already running, so it never leaves the work undone.
///
/// # Errors
/// Fails if `TEST_DATABASE_URL` is unset, Postgres cannot be reached, or another sweep runs for 30 seconds.
pub async fn drop_all_test_databases() -> Result<(), Error> {
    sweep(Duration::ZERO, Some(FINAL_WAIT)).await
}

/// With no `wait`, a sweep that finds another running leaves the work to it. With one, it waits that long for
/// the lock, then fails.
async fn sweep(older_than: Duration, wait: Option<Duration>) -> Result<(), Error> {
    let admin = connect(&Settings::from_values(&admin_url()?, Some("disable"))).await?;
    if !take_sweep_lock(&admin, wait).await? {
        return Ok(());
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let rows = admin.query("SELECT datname FROM pg_database", &[]).await?;
    // Every name is checked in Rust, since only a test database's name parses as a creation time. A name that
    // is not UTF-8, such as one made in a LATIN1 database, is never one of ours, so it is skipped.
    let names = rows
        .iter()
        .filter_map(|row| row.try_get::<_, String>("datname").ok());
    for name in names.filter(|name| stale(name, now, older_than)) {
        if let Err(error) = admin.batch_execute(&drop_statement(&name)).await {
            eprintln!("warning: stale test database {name} was not dropped: {error}");
        }
    }
    // Closing the connection releases the lock.
    Ok(())
}

/// Whether this connection now holds the sweep lock.
async fn take_sweep_lock(
    admin: &tokio_postgres::Client,
    wait: Option<Duration>,
) -> Result<bool, Error> {
    let Some(wait) = wait else {
        let row = admin
            .query_one("SELECT pg_try_advisory_lock($1) AS taken", &[&SWEEP_LOCK])
            .await?;
        return Ok(row.try_get("taken")?);
    };
    admin
        .batch_execute(&format!("SET lock_timeout = {}", wait.as_millis()))
        .await?;
    admin
        .query_one("SELECT pg_advisory_lock($1)", &[&SWEEP_LOCK])
        .await
        .map_err(|error| format!("another sweep kept the lock: {error}"))?;
    admin.batch_execute("RESET lock_timeout").await?;
    Ok(true)
}

/// Whether a name is exactly `t_<seconds>_<process>_<counter>`, as [`create`] makes them, and was created at
/// least `older_than` before `now`. Any other name, such as `t_2024_backup`, is never a test database.
fn stale(name: &str, now: u64, older_than: Duration) -> bool {
    let parts: Vec<&str> = name
        .strip_prefix("t_")
        .unwrap_or_default()
        .split('_')
        .collect();
    let [seconds, _, _] = parts.as_slice() else {
        return false;
    };
    parts
        .iter()
        .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        && seconds
            .parse::<u64>()
            .is_ok_and(|at| at <= now && now - at >= older_than.as_secs())
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
/// and a query after it is kept, slashes and all, unless it overrides the test database.
fn database_url(admin_url: &str, name: &str) -> Result<String, Error> {
    let unusable = "TEST_DATABASE_URL must be a postgres:// URL that names a database";
    let (address, query) = admin_url
        .split_once('?')
        .map_or((admin_url, None), |(address, query)| (address, Some(query)));
    let (base, database) = address.rsplit_once('/').ok_or(unusable)?;
    if !base.contains("://") || database.is_empty() {
        return Err(unusable.into());
    }
    let url = match query {
        Some(query) => format!("{base}/{name}?{query}"),
        None => format!("{base}/{name}"),
    };
    let config: tokio_postgres::Config = url.parse().map_err(|_| unusable)?;
    if config.get_dbname() != Some(name) {
        return Err("TEST_DATABASE_URL must not override the test database in its query".into());
    }
    Ok(url)
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
    fn a_test_database_is_stale_after_ten_minutes_and_all_go_at_the_end() {
        let now = 1_800_000_000;
        assert!(!stale(&format!("t_{now}_1_0"), now, STALE));
        assert!(!stale(&format!("t_{}_1_0", now - 599), now, STALE));
        assert!(stale(&format!("t_{}_1_0", now - 600), now, STALE));
        assert!(!stale(&format!("t_{}_1_0", now + 60), now, STALE));
        assert!(stale(&format!("t_{now}_1_0"), now, Duration::ZERO));
        assert!(!stale(&format!("t_{}_1_0", now + 60), now, Duration::ZERO));
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
            assert!(!stale(name, u64::MAX, Duration::ZERO), "{name}");
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
            "postgres://h/postgres?unknown=value",
        ] {
            assert!(database_url(admin, "t_1_2_3").is_err(), "{admin}");
        }
    }

    #[test]
    fn query_parameters_cannot_override_the_test_database() {
        for query in [
            "dbname=postgres",
            "db%6eame=postgres",
            "application_name=test&dbname=postgres",
            "dbname=postgres&application_name=test",
            "dbname=",
        ] {
            let url = format!("postgres://localhost/postgres?{query}");
            assert!(database_url(&url, "t_1_2_3").is_err(), "{query}");
        }
    }

    #[tokio::test]
    async fn the_final_sweep_waits_for_one_running_and_fails_if_it_never_ends() {
        let admin = Settings::from_values(&admin_url().unwrap(), Some("disable"));
        let holder = connect(&admin).await.unwrap();
        holder
            .query_one("SELECT pg_advisory_lock($1)", &[&SWEEP_LOCK])
            .await
            .unwrap();
        // Nothing is ever this old, so these sweeps drop nothing that tests running alongside still use.
        let error = sweep(Duration::MAX, Some(Duration::from_millis(200)))
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("another sweep kept the lock"),
            "{error}"
        );
        let waiting = tokio::spawn(sweep(Duration::MAX, Some(Duration::from_secs(10))));
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!waiting.is_finished(), "waits for the running sweep");
        drop(holder);
        waiting.await.unwrap().unwrap();
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
