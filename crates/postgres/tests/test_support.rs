//! The test database helper must leave nothing behind (#43).

use std::sync::{Arc, Mutex};

use tetsu_kernel::{Migration, Module, Storage};
use tetsu_postgres::test_support::{
    SWEEP_LOCK, fresh_database, fresh_database_encoded, migrated_database, sweep_stale_databases,
};
use tetsu_postgres::{Settings, connect};

#[cfg(test)]
mod support {
    use super::*;

    pub fn admin() -> Settings {
        let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is set");
        Settings::from_values(&url, Some("disable"))
    }

    pub async fn exists(name: &str) -> bool {
        let client = connect(&admin()).await.expect("connects");
        let row = client
            .query_one(
                "SELECT count(*) AS found FROM pg_database WHERE datname = $1",
                &[&name],
            )
            .await
            .expect("queries");
        row.try_get::<_, i64>("found").expect("a count") == 1
    }

    /// Sweeps until `name` is gone. A sweep that finds another running leaves the work to it, so one call may
    /// return before the database is dropped. Another sweep may be clearing many expired databases, such as
    /// those a mutation run leaves, so this allows a minute.
    pub async fn swept(name: &str) -> bool {
        for _ in 0..600 {
            sweep_stale_databases().await.expect("sweeps");
            if !exists(name).await {
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        false
    }

    /// A module whose one migration makes a table named for this process, so its template is this run's own.
    pub struct Probe;

    /// The migration [`Probe`] runs, made once per process.
    fn probe_migrations() -> &'static [Migration] {
        static MIGRATIONS: std::sync::OnceLock<[Migration; 1]> = std::sync::OnceLock::new();
        MIGRATIONS.get_or_init(|| {
            let sql = format!("CREATE TABLE made_by_{} (id int)", std::process::id());
            [Migration {
                version: 1,
                sql: Box::leak(sql.into_boxed_str()),
            }]
        })
    }

    impl Module for Probe {
        fn name(&self) -> &'static str {
            "probe"
        }

        fn storage(&self) -> Option<Storage> {
            Some(Storage {
                schema: "probe",
                migrations: probe_migrations(),
            })
        }
    }

    /// How many templates the server holds.
    pub async fn templates() -> i64 {
        connect(&admin())
            .await
            .expect("connects")
            .query_one(
                "SELECT count(*) AS n FROM pg_database WHERE datname LIKE 'tetsu\\_template\\_%'",
                &[],
            )
            .await
            .expect("counts")
            .try_get("n")
            .expect("a count")
    }
}

use support::{Probe, admin, exists, swept, templates};

#[tokio::test]
async fn a_database_is_dropped_when_the_test_is_done_with_it() {
    let database = fresh_database().await.expect("a test database");
    let name = database.name().to_string();
    let client = connect(&database).await.expect("is usable");
    let row = client
        .query_one("SELECT current_database() AS name", &[])
        .await
        .expect("queries the connected database");
    assert_eq!(row.try_get::<_, String>("name").expect("a name"), name);
    assert!(exists(&name).await);
    drop(database);
    assert!(!exists(&name).await);
}

#[tokio::test]
async fn a_database_is_dropped_even_when_the_test_panics() {
    let seen = Arc::new(Mutex::new(None));
    let inside = Arc::clone(&seen);
    let outcome = tokio::spawn(async move {
        let database = fresh_database().await.expect("a test database");
        *inside.lock().expect("a lock") = Some(database.name().to_string());
        panic!("the test fails while holding its database");
    })
    .await;
    assert!(outcome.is_err());
    let name = seen.lock().expect("a lock").clone().expect("a name");
    assert!(!exists(&name).await);
}

#[tokio::test]
async fn a_database_left_by_a_killed_run_is_swept() {
    let stale = format!("t_1_{}_0", std::process::id());
    let client = connect(&admin()).await.expect("connects");
    client
        .batch_execute(&format!("CREATE DATABASE {stale}"))
        .await
        .expect("creates a stale database");
    let fresh = fresh_database().await.expect("a test database");

    assert!(swept(&stale).await);
    assert!(exists(fresh.name()).await);
}

#[tokio::test]
async fn a_database_with_another_encoding_is_cleaned_up_the_same_way() {
    let database = fresh_database_encoded("LATIN1")
        .await
        .expect("a LATIN1 database");
    let name = database.name().to_string();
    let client = connect(&database).await.expect("connects");
    let row = client
        .query_one("SELECT current_setting('server_encoding') AS encoding", &[])
        .await
        .expect("queries");
    assert_eq!(
        row.try_get::<_, String>("encoding").expect("a name"),
        "LATIN1"
    );
    drop(client);
    drop(database);
    assert!(!exists(&name).await);
}

#[tokio::test]
async fn an_encoding_name_may_hold_underscores_but_nothing_unsafe() {
    let database = fresh_database_encoded("EUC_JP")
        .await
        .expect("an EUC_JP database");
    assert!(exists(database.name()).await);
    for unsafe_name in ["UTF8; DROP DATABASE postgres", "LATIN1'", "\"UTF8\""] {
        assert!(
            fresh_database_encoded(unsafe_name).await.is_err(),
            "{unsafe_name}"
        );
    }
}

#[tokio::test]
async fn the_sweep_leaves_other_databases_and_survives_odd_names() {
    let client = connect(&admin()).await.expect("connects");
    let names = ["t_2024_backup", "t_7_x y", "t_1_2_3_4"];
    for name in names {
        client
            .batch_execute(&format!("DROP DATABASE IF EXISTS \"{name}\""))
            .await
            .expect("clears a leftover");
        client
            .batch_execute(&format!("CREATE DATABASE \"{name}\""))
            .await
            .expect("creates a database that only looks like a test one");
    }
    sweep_stale_databases()
        .await
        .expect("sweeps without failing");
    for name in names {
        assert!(exists(name).await, "{name}");
        client
            .batch_execute(&format!("DROP DATABASE \"{name}\""))
            .await
            .expect("cleans up");
    }
}

#[tokio::test]
async fn only_one_sweep_runs_at_a_time() {
    // Take the lock first, so sweeps from tests running alongside cannot drop the database early.
    let holder = connect(&admin()).await.expect("connects");
    holder
        .query_one("SELECT pg_advisory_lock($1)", &[&SWEEP_LOCK])
        .await
        .expect("holds the sweep lock");
    let stale = format!("t_2_{}_9", std::process::id());
    let client = connect(&admin()).await.expect("connects");
    client
        .batch_execute(&format!("CREATE DATABASE {stale}"))
        .await
        .expect("creates a stale database");
    sweep_stale_databases().await.expect("skips");
    assert!(
        exists(&stale).await,
        "a second sweep waits its turn instead of repeating the work"
    );
    drop(holder);
    assert!(swept(&stale).await);
}

#[tokio::test]
async fn a_database_name_that_is_not_utf8_cannot_stop_the_sweep() {
    // A LATIN1 database stores this name as the single byte 0xFF, which is not UTF-8 to anyone reading it.
    let latin1 = fresh_database_encoded("LATIN1")
        .await
        .expect("a LATIN1 database");
    let odd = format!("odd_{}_\u{FF}", std::process::id());
    let client = connect(&latin1).await.expect("connects");
    client
        .batch_execute(&format!("CREATE DATABASE \"{odd}\""))
        .await
        .expect("creates a database with a LATIN1 name");
    let swept = sweep_stale_databases().await;
    let created = fresh_database().await;
    client
        .batch_execute(&format!("DROP DATABASE \"{odd}\""))
        .await
        .expect("cleans up");
    swept.expect("the sweep skips the name");
    created.expect("tests can still get a database");
}

/// Every database migrated for the same modules is copied from one template, built by the first test that needs
/// it, and holds what migrating leaves; the copy is a test database like any other, dropped with its test.
#[tokio::test]
async fn migrated_databases_are_copies_of_one_template() {
    let before = templates().await;
    let first = migrated_database(&[&Probe])
        .await
        .expect("a migrated database");
    let second = migrated_database(&[&Probe]).await.expect("another");
    assert_eq!(templates().await, before + 1);
    let table = format!("made_by_{}", std::process::id());
    for database in [&first, &second] {
        let found: i64 = connect(database)
            .await
            .expect("connects")
            .query_one(
                "SELECT count(*) AS n FROM pg_tables WHERE schemaname = 'probe' AND tablename = $1",
                &[&table],
            )
            .await
            .expect("counts")
            .try_get("n")
            .expect("a count");
        assert_eq!(found, 1, "{}", database.name());
    }
    let name = first.name().to_string();
    drop(first);
    assert!(!exists(&name).await);
}
