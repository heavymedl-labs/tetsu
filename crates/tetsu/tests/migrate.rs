//! `tetsu migrate` against a real Postgres.

use std::process::Command;

use tetsu_postgres::test_support::fresh_database;

#[cfg(test)]
mod support {
    use std::process::{Command, Output};

    use tetsu_postgres::{Settings, connect};

    pub fn migrate(url: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_tetsu"))
            .arg("migrate")
            .env("DATABASE_URL", url)
            .env("DATABASE_TLS", "disable")
            .output()
            .expect("the binary runs")
    }

    pub async fn applied_tenancy_migrations(settings: &Settings) -> i64 {
        let client = connect(settings).await.expect("connects");
        let row = client
            .query_one("SELECT count(*) AS applied FROM tenancy.migrations", &[])
            .await
            .expect("the tracking table exists");
        row.try_get("applied").expect("a count")
    }
}

use support::{applied_tenancy_migrations, migrate};

/// Tenancy has five migrations: its tables, their isolation, the rename function (#135), the name lookup (#111), and
/// the slug lookup (ADR-0096).
#[tokio::test]
async fn migrate_applies_each_module_and_a_rerun_changes_nothing() {
    let settings = fresh_database().await.expect("a test database");

    let first = migrate(settings.url());
    assert!(first.status.success(), "{first:?}");
    assert_eq!(applied_tenancy_migrations(&settings).await, 5);

    let second = migrate(settings.url());
    assert!(second.status.success(), "{second:?}");
    assert_eq!(applied_tenancy_migrations(&settings).await, 5);
}

#[test]
fn a_failed_connection_never_prints_the_password() {
    for url in [
        "postgres://tetsu:s3cret-pw@127.0.0.1:1/tetsu",
        "postgres://tetsu:s3cret-pw@127.0.0.1:not-a-port/tetsu",
    ] {
        let output = migrate(url);
        assert_eq!(output.status.code(), Some(1), "{url}");
        let stderr = String::from_utf8(output.stderr).expect("output is UTF-8");
        assert!(stderr.starts_with("error: "), "{stderr}");
        assert!(!stderr.contains("s3cret-pw"), "{stderr}");
    }
}

#[test]
fn a_failure_says_why_not_only_that_it_failed() {
    let output = migrate("postgres://tetsu@127.0.0.1:1/tetsu");
    let stderr = String::from_utf8(output.stderr).expect("output is UTF-8");
    assert!(stderr.contains("Connection refused"), "{stderr}");
}

/// The test Postgres offers TLS with a self-signed certificate, so a binary that requires TLS refuses it.
#[tokio::test]
async fn tls_is_required_unless_the_environment_turns_it_off() {
    let settings = fresh_database().await.expect("a test database");
    let output = Command::new(env!("CARGO_BIN_EXE_tetsu"))
        .arg("migrate")
        .env("DATABASE_URL", settings.url())
        .env_remove("DATABASE_TLS")
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).expect("output is UTF-8");
    assert!(stderr.contains("UnknownIssuer"), "{stderr}");
}

#[test]
fn a_database_url_that_is_not_utf8_is_named_as_such() {
    use std::os::unix::ffi::OsStrExt;
    let output = Command::new(env!("CARGO_BIN_EXE_tetsu"))
        .arg("migrate")
        .env(
            "DATABASE_URL",
            std::ffi::OsStr::from_bytes(b"postgres://x\xff"),
        )
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).expect("output is UTF-8");
    assert!(stderr.contains("DATABASE_URL must be UTF-8"), "{stderr}");
}

#[tokio::test]
async fn spaces_around_the_database_url_are_ignored() {
    let settings = fresh_database().await.expect("a test database");
    let output = migrate(&format!("  {}\n", settings.url()));
    assert!(output.status.success(), "{output:?}");
}

/// ADR-0042's catalog checks over every module the binary runs.
#[tokio::test]
async fn every_module_follows_the_isolation_rules() {
    let settings = fresh_database().await.expect("a test database");
    let migrated = migrate(settings.url());
    assert!(migrated.status.success(), "{migrated:?}");
    let mut client = tetsu_postgres::connect(&settings).await.expect("connects");
    let schemas = tetsu_postgres::module_schemas(&client)
        .await
        .expect("finds the modules");
    assert!(schemas.contains(&"tenancy".to_string()), "{schemas:?}");
    assert!(schemas.contains(&"audit".to_string()), "{schemas:?}");
    let schemas: Vec<&str> = schemas.iter().map(String::as_str).collect();
    let problems = tetsu_postgres::isolation_violations(&mut client, &schemas)
        .await
        .expect("checks");
    assert_eq!(problems, Vec::<String>::new());
}

/// As on the deploy, where `tetsu_owner` migrates with CREATEROLE but is no superuser: every module migrates, twice,
/// and keeps the isolation rules, as a runner that owns the database and holds ADMIN on the module and delivery
/// roles, as their creator, without inheriting them. `tetsu_owner` did not create `tetsu_app`, so it holds no ADMIN
/// on it, and neither does this runner. A statement only a superuser may run, such as revoking on a function
/// another role owns, fails here as it would at deploy (#168).
#[tokio::test]
async fn every_module_migrates_as_a_runner_that_is_no_superuser() {
    // A migration as the test server's superuser makes the module roles, if no test has yet.
    let first = fresh_database().await.expect("a test database");
    assert!(migrate(first.url()).status.success());
    let admin = tetsu_postgres::connect(&first).await.expect("connects");
    let schemas = tetsu_postgres::module_schemas(&admin)
        .await
        .expect("finds the modules");
    let mut roles = vec![tetsu_postgres::DELIVERY_ROLE.to_string()];
    for schema in &schemas {
        roles.extend([
            tetsu_postgres::module_role(schema),
            tetsu_postgres::lookup_role(schema),
        ]);
    }
    let settings = fresh_database().await.expect("a test database");
    let runner = format!("t_runner_{}", std::process::id());
    let mut admin = tetsu_postgres::connect(&settings).await.expect("connects");
    admin
        .batch_execute(&format!(
            "CREATE ROLE {runner} LOGIN CREATEROLE;
             GRANT {} TO {runner} WITH ADMIN TRUE, INHERIT FALSE, SET FALSE;
             ALTER DATABASE {} OWNER TO {runner}",
            roles.join(", "),
            settings.name()
        ))
        .await
        .expect("makes the runner");
    // Assumes the test server trusts local roles without a password, as CI's and check.sh's do.
    let as_runner = settings
        .url()
        .replacen("postgres@", &format!("{runner}@"), 1);
    let runs = [migrate(&as_runner), migrate(&as_runner)];
    let schemas: Vec<&str> = schemas.iter().map(String::as_str).collect();
    let problems = tetsu_postgres::isolation_violations(&mut admin, &schemas).await;
    admin
        .batch_execute(&format!(
            "REASSIGN OWNED BY {runner} TO postgres; DROP OWNED BY {runner}; DROP ROLE {runner}"
        ))
        .await
        .expect("drops the runner");
    assert!(as_runner.contains(&format!("//{runner}@")), "{as_runner}");
    for run in &runs {
        assert!(run.status.success(), "{run:?}");
    }
    assert_eq!(problems.expect("checks"), Vec::<String>::new());
}

/// A local `.env` where the command runs supplies settings the environment leaves unset, and the environment wins
/// over it (ADR-0050).
#[tokio::test]
async fn a_local_env_file_supplies_settings_and_the_environment_wins() {
    let settings = fresh_database().await.expect("a test database");
    let folder = tetsu_postgres::test_support::Scratch::folder("env").expect("a folder");
    let run = |env_file: &str, url: Option<&str>| {
        std::fs::write(folder.join(".env"), env_file).expect("writes .env");
        let mut command = Command::new(env!("CARGO_BIN_EXE_tetsu"));
        command
            .arg("migrate")
            .current_dir(&*folder)
            .env_remove("DATABASE_URL")
            .env_remove("DATABASE_TLS");
        if let Some(url) = url {
            command.env("DATABASE_URL", url);
        }
        command.output().expect("the binary runs")
    };

    let from_file = run(
        &format!(
            "# local\nDATABASE_URL={}\nDATABASE_TLS=disable\n",
            settings.url()
        ),
        None,
    );
    assert!(from_file.status.success(), "{from_file:?}");
    assert_eq!(applied_tenancy_migrations(&settings).await, 5);

    let overridden = run(
        "DATABASE_URL=postgres://nobody@127.0.0.1:1/none\nDATABASE_TLS=disable\n",
        Some(settings.url()),
    );
    assert!(overridden.status.success(), "{overridden:?}");

    let broken = run("DATABASE_TLS=disable\npass word=s3cret\n", None);
    assert_eq!(broken.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&broken.stderr);
    assert!(stderr.contains(".env line 2 is not NAME=value"), "{stderr}");
    assert!(!stderr.contains("s3cret"), "{stderr}");
}
