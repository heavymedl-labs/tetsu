//! Fresh databases for tests. `TEST_DATABASE_URL` points at a Postgres the tests may create databases on.
//!
//! Every database is dropped when its test is done with it, including when the test fails or panics. One
//! left by a run that was killed is swept by a later run (#43), as is a runner role it made (#168). A database with modules migrated is copied from a
//! template migrated once, which the final sweep drops. Temporary files work the same way: a [`Scratch`]
//! is removed when dropped, and `scripts/clean.py` sweeps one a killed run left (#124).

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tetsu_kernel::Module;

use crate::connection::{Error, Settings, connect};
use crate::migrate::migrate;
use crate::roles::{APP_ROLE, ensure_role};

static NEXT: AtomicU32 = AtomicU32::new(0);
/// Held while sweeping, so test binaries running at once do not each repeat the same drops.
pub const SWEEP_LOCK: i64 = 7_243_002;
/// Held while building a template, so test binaries running at once build each one once.
pub const TEMPLATE_LOCK: i64 = 7_243_003;
/// What every template's name starts with; the rest is a hash of the migrations it holds.
const TEMPLATE_PREFIX: &str = "tetsu_template_";
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

    /// The same database, logged in as the application role, as `tetsu serve` is (ADR-0042). Assumes the test server
    /// trusts local roles without a password, as CI's and check.sh's do.
    #[must_use]
    pub fn as_app(&self) -> Settings {
        let url = self
            .settings
            .url()
            .replacen("postgres@", &format!("{APP_ROLE}@"), 1);
        Settings::from_values(&url, Some("disable"))
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

/// A fresh database with `modules` migrated, as [`crate::migrate`] leaves one. It is copied from a template
/// migrated once and named for those migrations, which is about ten times faster than migrating each time, so a
/// changed migration gets a template of its own. Templates stay for later tests and runs until the final sweep.
///
/// # Errors
/// As [`fresh_database`], or if the modules fail to migrate.
pub async fn migrated_database(modules: &[&dyn Module]) -> Result<TestDatabase, Error> {
    let template = template(modules).await?;
    create(&format!(" TEMPLATE {}", quoted(&template))).await
}

/// The template for `modules`, built first if no test has built it yet. It is built as a test database, so a run
/// killed while building leaves only what the sweep removes, then renamed, and closed to connections, since
/// Postgres copies only a database no one is connected to.
async fn template(modules: &[&dyn Module]) -> Result<String, Error> {
    let name = template_name(modules);
    let admin = connect(&Settings::from_values(&admin_url()?, Some("disable"))).await?;
    if !exists(&admin, &name).await? {
        // Closing the connection releases the lock.
        admin
            .query_one("SELECT pg_advisory_lock($1)", &[&TEMPLATE_LOCK])
            .await?;
        if !exists(&admin, &name).await? {
            build_template(&admin, &name, modules).await?;
        }
    }
    Ok(name)
}

async fn exists(admin: &tokio_postgres::Client, name: &str) -> Result<bool, Error> {
    let row = admin
        .query_one(
            "SELECT EXISTS (SELECT FROM pg_database WHERE datname = $1) AS found",
            &[&name],
        )
        .await?;
    Ok(row.try_get("found")?)
}

/// Migrates a test database, then makes it the template: renamed once no one is connected, and closed to
/// connections from then on.
async fn build_template(
    admin: &tokio_postgres::Client,
    name: &str,
    modules: &[&dyn Module],
) -> Result<(), Error> {
    let build = create("").await?;
    migrate(&build, modules).await?;
    admin
        .query(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = $1",
            &[&build.name()],
        )
        .await?;
    let (from, to) = (quoted(build.name()), quoted(name));
    admin
        .batch_execute(&format!("ALTER DATABASE {from} RENAME TO {to}"))
        .await?;
    admin
        .batch_execute(&format!("ALTER DATABASE {to} WITH ALLOW_CONNECTIONS false"))
        .await?;
    Ok(())
}

/// A template's name: the prefix, then a hash of every schema, version, and file the modules migrate.
fn template_name(modules: &[&dyn Module]) -> String {
    let mut hasher = DefaultHasher::new();
    for storage in modules.iter().filter_map(|module| module.storage()) {
        storage.schema.hash(&mut hasher);
        for migration in storage.migrations {
            (migration.version, migration.sql).hash(&mut hasher);
        }
    }
    format!("{TEMPLATE_PREFIX}{:016x}", hasher.finish())
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
/// Fails if `TEST_DATABASE_URL` is unset, Postgres cannot be reached, another sweep runs for 30 seconds, or a
/// test database will not drop.
pub async fn drop_all_test_databases() -> Result<(), Error> {
    sweep(Duration::ZERO, Some(FINAL_WAIT)).await
}

/// With no `wait`, a sweep that finds another running leaves the work to it, and a database it cannot drop is
/// only reported. With one, it waits that long for the lock, and fails if any database is left.
async fn sweep(older_than: Duration, wait: Option<Duration>) -> Result<(), Error> {
    let admin = connect(&Settings::from_values(&admin_url()?, Some("disable"))).await?;
    if !take_sweep_lock(&admin, wait).await? {
        return Ok(());
    }
    let mut left = sweep_databases(&admin, older_than).await?;
    left.extend(sweep_runner_roles(&admin).await?);
    // Closing the connection releases the lock. A sweep that waited is the last one, so nothing may be left.
    match (wait, left.is_empty()) {
        (Some(_), false) => {
            Err(format!("test databases or roles were left: {}", left.join(", ")).into())
        }
        _ => Ok(()),
    }
}

/// Drops each test database a sweep reaches, and returns those that would not drop.
async fn sweep_databases(
    admin: &tokio_postgres::Client,
    older_than: Duration,
) -> Result<Vec<String>, Error> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let rows = admin.query("SELECT datname FROM pg_database", &[]).await?;
    // Every name is checked in Rust, since only a test database's name parses as a creation time. A name that
    // is not UTF-8, such as one made in a LATIN1 database, is never one of ours, so it is skipped.
    let names = rows
        .iter()
        .filter_map(|row| row.try_get::<_, String>("datname").ok());
    let mut left = Vec::new();
    for name in names.filter(|name| swept(name, now, older_than)) {
        if let Err(error) = admin.batch_execute(&drop_statement(&name)).await {
            eprintln!("warning: stale test database {name} was not dropped: {error}");
            left.push(name);
        }
    }
    Ok(left)
}

/// Drops each runner role a test made, `t_runner_<process>` or `t_runner_app_<process>`, whose process has ended, so
/// a run that crashed leaves no role holding ADMIN on the module roles (#168). Returns those that would not drop,
/// such as one still holding rights in a database a later sweep drops.
async fn sweep_runner_roles(admin: &tokio_postgres::Client) -> Result<Vec<String>, Error> {
    let rows = admin
        .query(
            r"SELECT rolname::text AS name FROM pg_catalog.pg_roles WHERE rolname LIKE 't\_runner\_%'",
            &[],
        )
        .await?;
    let mut left = Vec::new();
    for row in &rows {
        let name: String = row.try_get("name")?;
        if !ended(&name) {
            continue;
        }
        if let Err(error) = admin
            .batch_execute(&format!("DROP ROLE IF EXISTS {}", quoted(&name)))
            .await
        {
            eprintln!("warning: test role {name} was not dropped: {error}");
            left.push(name);
        }
    }
    Ok(left)
}

/// Whether a runner role's name ends in the ID of a process that is no longer running. Any other name is never
/// ended, so it is kept.
fn ended(role: &str) -> bool {
    role.strip_prefix("t_runner_app_")
        .or_else(|| role.strip_prefix("t_runner_"))
        .and_then(|process| process.parse::<u32>().ok())
        .is_some_and(|process| !std::path::Path::new(&format!("/proc/{process}")).exists())
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

/// Whether a sweep drops a database: a stale test database, or a template, but only in the final sweep, which drops
/// every test database however new, since later tests copy templates while any run goes on.
fn swept(name: &str, now: u64, older_than: Duration) -> bool {
    stale(name, now, older_than) || (older_than.is_zero() && name.starts_with(TEMPLATE_PREFIX))
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
    // Roles belong to the whole server, so every test database shares this one, made by whichever test is first.
    ensure_role(&admin, APP_ROLE, &format!("CREATE ROLE {APP_ROLE} LOGIN")).await?;
    // PUBLIC may not make temporary tables, as on every Tetsu database (ADR-0060).
    let name_sql = quoted(&name);
    admin
        .batch_execute(&format!("CREATE DATABASE {name_sql}{options}"))
        .await?;
    admin
        .batch_execute(&format!(
            "REVOKE TEMPORARY ON DATABASE {name_sql} FROM PUBLIC"
        ))
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

/// Drops a test's database. The test's own connections are usually still open, since its runtime waits on this
/// drop, and a forced drop waits for the connections it ends in steps of 100 ms; ending them first and watching
/// for them in steps of 2 ms takes a tenth of that. The forced drop stays, for any connection that outlives the
/// wait.
async fn drop_database(admin_url: &str, name: &str) -> Result<(), Error> {
    let admin = connect(&Settings::from_values(admin_url, Some("disable"))).await?;
    let connected =
        "SELECT count(pg_terminate_backend(pid)) AS ended FROM pg_stat_activity WHERE datname = $1";
    for _ in 0..500 {
        let ended: i64 = admin
            .query_one(connected, &[&name])
            .await?
            .try_get("ended")?;
        if ended == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
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

/// A file or folder in the system's temporary folder, named `tetsu-<process>-<name>`, and removed when dropped,
/// including when its test fails or panics, so failing tests leave nothing behind (#124).
pub struct Scratch(std::path::PathBuf);

impl Scratch {
    /// A file holding `contents`.
    ///
    /// # Errors
    /// Fails if the file cannot be written.
    pub fn file(name: &str, contents: &str) -> std::io::Result<Self> {
        let scratch = Self::named(name);
        std::fs::write(&scratch.0, contents)?;
        Ok(scratch)
    }

    /// An empty folder.
    ///
    /// # Errors
    /// Fails if the folder cannot be made.
    pub fn folder(name: &str) -> std::io::Result<Self> {
        let scratch = Self::named(name);
        std::fs::create_dir_all(&scratch.0)?;
        Ok(scratch)
    }

    fn named(name: &str) -> Self {
        Self(std::env::temp_dir().join(format!("tetsu-{}-{name}", std::process::id())))
    }
}

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;

    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::path::Path> for Scratch {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0).or_else(|_| std::fs::remove_file(&self.0));
    }
}

#[cfg(test)]
mod tests {
    /// A runner role whose process has ended is dropped, and one whose process runs is kept; one that holds rights
    /// in a database is reported as left. No process has an ID above Linux's largest, 4,194,304.
    #[tokio::test]
    async fn runner_roles_of_ended_runs_are_dropped() {
        let database = fresh_database().await.unwrap();
        let admin = connect(&database).await.unwrap();
        // Held throughout, so no sweep in a test running alongside reaches these roles, or reports one as left.
        admin
            .query_one("SELECT pg_advisory_lock($1)", &[&SWEEP_LOCK])
            .await
            .unwrap();
        let (gone, held, running) = (
            "t_runner_4194998",
            "t_runner_app_4194999",
            format!("t_runner_app_{}", std::process::id()),
        );
        admin
            .batch_execute(&format!(
                "CREATE ROLE {gone}; CREATE ROLE {held}; CREATE ROLE {running};
                 GRANT CONNECT ON DATABASE {} TO {held}",
                database.name()
            ))
            .await
            .unwrap();
        let left = sweep_runner_roles(&admin).await;
        let kept = admin
            .query(
                "SELECT rolname::text FROM pg_catalog.pg_roles WHERE rolname = ANY($1)",
                &[&vec![gone.to_string(), held.to_string(), running.clone()]],
            )
            .await
            .unwrap();
        admin
            .batch_execute(&format!(
                "REVOKE CONNECT ON DATABASE {} FROM {held}; DROP ROLE {held}; DROP ROLE {running};
                 SELECT pg_advisory_unlock({SWEEP_LOCK})",
                database.name()
            ))
            .await
            .unwrap();
        assert_eq!(left.unwrap(), [held]);
        let mut kept: Vec<String> = kept.iter().map(|row| row.try_get(0).unwrap()).collect();
        kept.sort();
        let mut expected = vec![held.to_string(), running];
        expected.sort();
        assert_eq!(kept, expected);
        assert!(!ended("t_runner_x") && !ended("tetsu_identity"));
    }

    #[test]
    fn scratch_files_and_folders_go_when_dropped_even_by_a_panic() {
        let file = Scratch::file("scratch.txt", "x").unwrap();
        let folder = Scratch::folder("scratch-folder").unwrap();
        std::fs::write(folder.join("inside"), "y").unwrap();
        let (file_path, folder_path) = (file.to_path_buf(), folder.to_path_buf());
        assert!(file_path.is_file() && folder_path.is_dir());
        let panicked = std::panic::catch_unwind(move || {
            let _held = (file, folder);
            panic!("a failing test");
        });
        assert!(panicked.is_err());
        assert!(!file_path.exists() && !folder_path.exists());
    }

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

    /// Postgres refuses to drop a template database, so this one cannot be dropped until it is unmarked.
    #[tokio::test]
    async fn the_final_sweep_fails_when_a_database_is_left() {
        let admin = connect(&Settings::from_values(
            &admin_url().unwrap(),
            Some("disable"),
        ))
        .await
        .unwrap();
        // Created long ago by its name, so only a sweep of stale databases reaches it, never a test's own.
        let name = format!("t_1000_{}_0", std::process::id());
        // One statement per call: Postgres runs a batch as one transaction, which CREATE and DROP refuse.
        for statement in [
            format!("CREATE DATABASE {name}"),
            format!("ALTER DATABASE {name} IS_TEMPLATE true"),
        ] {
            admin.batch_execute(&statement).await.unwrap();
        }
        let swept = sweep(STALE, Some(Duration::from_secs(10))).await;
        for statement in [
            format!("ALTER DATABASE {name} IS_TEMPLATE false"),
            format!("DROP DATABASE {name}"),
        ] {
            admin.batch_execute(&statement).await.unwrap();
        }
        let error = swept.unwrap_err().to_string();
        assert!(
            error.contains(&format!("test databases or roles were left: {name}")),
            "{error}"
        );
    }

    /// Templates outlive every sweep but the final one, since tests copy them while any run goes on.
    #[test]
    fn only_the_final_sweep_drops_templates() {
        let template = format!("{TEMPLATE_PREFIX}0123456789abcdef");
        assert_eq!(
            [
                swept(&template, 5_000, STALE),
                swept(&template, 5_000, Duration::ZERO),
                swept("t_1000_1_0", 5_000, STALE),
                swept("tetsu_other", 5_000, Duration::ZERO)
            ],
            [false, true, true, false]
        );
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
