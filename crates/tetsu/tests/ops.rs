//! The hosted chores the workflows run (#135): `check`, `reset`, `replay`, and `rename`, and the local `setup`,
//! against a real Postgres.

use std::path::Path;
use std::process::{Command, Output};

use tetsu_postgres::test_support::{TestDatabase, fresh_database};
use tetsu_postgres::{Settings, connect};

#[cfg(test)]
mod support {
    use super::*;

    /// `tetsu` with these arguments, against `settings`, with `TETSU_DEMO` set as given or unset.
    pub fn tetsu(settings: &Settings, args: &[&str], demo: Option<&str>) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tetsu"));
        command
            .args(args)
            .env("DATABASE_URL", settings.url())
            .env("DATABASE_TLS", "disable");
        match demo {
            Some(value) => command.env("TETSU_DEMO", value),
            None => command.env_remove("TETSU_DEMO"),
        };
        command.output().expect("the binary runs")
    }

    /// A database the binary migrated, with the sample tenant seeded as the application role.
    pub async fn seeded() -> TestDatabase {
        let database = fresh_database().await.expect("a test database");
        let migrated = tetsu(&database, &["migrate"], None);
        assert!(migrated.status.success(), "{migrated:?}");
        let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.toml");
        let seeded = tetsu(
            &database.as_app(),
            &["seed", sample.to_str().expect("a UTF-8 path")],
            None,
        );
        assert!(seeded.status.success(), "{seeded:?}");
        database
    }

    pub fn out(output: &Output) -> String {
        String::from_utf8(output.stdout.clone()).expect("UTF-8")
    }

    pub fn err(output: &Output) -> String {
        String::from_utf8(output.stderr.clone()).expect("UTF-8")
    }

    pub async fn schemas(settings: &Settings) -> Vec<String> {
        let client = connect(settings).await.expect("connects");
        tetsu_postgres::module_schemas(&client)
            .await
            .expect("lists")
    }
}

use support::{err, out, schemas, seeded, tetsu};

/// Every module the binary runs keeps ADR-0042's rules, as the check after each hosted migrate finds; a broken
/// rule fails the command and is named.
#[tokio::test]
async fn check_passes_a_migrated_database_and_names_a_broken_rule() {
    let database = seeded().await;
    let checked = tetsu(&database, &["check"], None);
    assert!(checked.status.success(), "{checked:?}");
    assert_eq!(out(&checked), "no isolation problems\n");
    connect(&database)
        .await
        .expect("connects")
        .batch_execute("ALTER TABLE tenancy.locations NO FORCE ROW LEVEL SECURITY")
        .await
        .expect("breaks a rule");
    let failed = tetsu(&database, &["check"], None);
    assert!(!failed.status.success());
    assert!(err(&failed).contains("tenancy.locations"), "{failed:?}");
}

/// Reset drops only where `TETSU_DEMO` is true, then migrates, which builds the schemas afresh.
#[tokio::test]
async fn reset_starts_the_demo_over_and_nowhere_else() {
    let database = seeded().await;
    for demo in [None, Some("false")] {
        let refused = tetsu(&database, &["reset", "tenancy"], demo);
        assert!(!refused.status.success(), "{demo:?}");
    }
    let refused = tetsu(&database, &["reset", "tenancy"], None);
    assert!(
        err(&refused).contains("TETSU_DEMO must be set"),
        "{refused:?}"
    );
    let refused = tetsu(&database, &["reset", "tenancy"], Some("false"));
    assert!(err(&refused).contains("reset runs only where TETSU_DEMO is true"));
    let unknown = tetsu(&database, &["reset", "tenancy", "public"], Some("true"));
    assert!(err(&unknown).contains("\"public\" is not a module's schema here"));
    assert_eq!(
        schemas(&database).await,
        ["audit", "identity", "mail", "tenancy"]
    );
    let reset = tetsu(&database, &["reset", "tenancy", "audit"], Some("true"));
    assert!(reset.status.success(), "{reset:?}");
    assert_eq!(
        out(&reset),
        "dropped: tenancy, audit\nmigrated: tenancy, identity, audit, mail\n"
    );
    assert_eq!(
        schemas(&database).await,
        ["audit", "identity", "mail", "tenancy"]
    );
    let rerun = tetsu(&database, &["migrate"], None);
    assert!(rerun.status.success(), "{rerun:?}");
    assert_eq!(
        schemas(&database).await,
        ["audit", "identity", "mail", "tenancy"]
    );
}

/// A tenant moves to a new slug as the application role, and a bad or taken slug changes nothing.
#[tokio::test]
async fn rename_moves_a_tenant_to_a_new_address() {
    let database = seeded().await;
    let app = database.as_app();
    for (slug, why) in [("login", "reserved"), ("Not A Slug", "must be 1 to 63")] {
        let refused = tetsu(&app, &["rename", "/sample-studio", slug], None);
        assert!(err(&refused).contains(why), "{refused:?}");
    }
    let missing = tetsu(&app, &["rename", "/nobody", "sample-gym"], None);
    assert!(
        err(&missing).contains("could not rename the tenant at /nobody"),
        "{missing:?}"
    );
    let moved = tetsu(&app, &["rename", "/sample-studio", "sample-gym"], None);
    assert!(moved.status.success(), "{moved:?}");
    assert_eq!(
        out(&moved),
        "moved the tenant at /sample-studio to /sample-gym\n"
    );
}

/// Replay reaches only the outboxes of the modules the binary runs that record events, as Identity does: another
/// name is refused before it reaches the database's SQL, and an event in neither of Identity's outboxes is not found.
#[tokio::test]
async fn replay_names_a_module_that_records_events() {
    let database = seeded().await;
    let app = database.as_app();
    let id = "0190a3d2-0000-7000-8000-000000000001";
    let refused = tetsu(&app, &["replay", "tenancy", id], None);
    assert!(!refused.status.success());
    assert!(
        err(&refused).contains("\"tenancy\" records no events here"),
        "{refused:?}"
    );
    let missing = tetsu(&app, &["replay", "identity", id], None);
    assert!(!missing.status.success());
    assert!(
        err(&missing).contains(&format!("no event {id} in identity's outboxes")),
        "{missing:?}"
    );
}

/// Setup creates the database the settings name when the server has none, then migrates; run again, it finds the
/// database and only migrates, so it is safe to run any time.
#[tokio::test]
async fn setup_creates_the_database_once_and_migrates() {
    let database = fresh_database().await.expect("a test database");
    let server = database.on_database("postgres").expect("the server");
    connect(&server)
        .await
        .expect("connects")
        .batch_execute(&format!(
            "DROP DATABASE \"{}\" WITH (FORCE)",
            database.name()
        ))
        .await
        .expect("drops");
    let first = tetsu(&database, &["setup"], None);
    assert!(first.status.success(), "{first:?}");
    assert_eq!(
        out(&first),
        "created the database\nmigrated: tenancy, identity, audit, mail\n"
    );
    assert_eq!(
        schemas(&database).await,
        ["audit", "identity", "mail", "tenancy"]
    );
    let again = tetsu(&database, &["setup"], None);
    assert!(again.status.success(), "{again:?}");
    assert_eq!(
        out(&again),
        "the database exists\nmigrated: tenancy, identity, audit, mail\n"
    );
    let refused = tetsu(&database, &["setup", "now"], None);
    assert!(
        err(&refused).contains("setup takes no arguments"),
        "{refused:?}"
    );
}
