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

#[tokio::test]
async fn migrate_applies_each_module_and_a_rerun_changes_nothing() {
    let settings = fresh_database().await.expect("a test database");

    let first = migrate(settings.url());
    assert!(first.status.success(), "{first:?}");
    assert_eq!(applied_tenancy_migrations(&settings).await, 1);

    let second = migrate(settings.url());
    assert!(second.status.success(), "{second:?}");
    assert_eq!(applied_tenancy_migrations(&settings).await, 1);
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
    assert!(stderr.contains("server does not support TLS"), "{stderr}");
}
