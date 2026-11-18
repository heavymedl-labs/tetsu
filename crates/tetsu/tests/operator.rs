//! `tetsu operator` against a real Postgres, migrated once and copied for each test, its output piped as CI or a
//! log would take it (ADR-0040).

use std::path::Path;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use tetsu_identity::{Identity, OperatorName};
use tetsu_postgres::test_support::{Scratch, TestDatabase, migrated_database};
use tetsu_postgres::{Settings, pool};

#[cfg(test)]
mod support {
    use super::*;

    /// Runs the command as the login `tester`, with no site origin, which no command printing to a pipe needs, in
    /// an empty folder, so no `.env` a developer keeps can change what it does.
    pub fn operator(settings: &Settings, args: &[&str]) -> Output {
        static RUN: AtomicUsize = AtomicUsize::new(0);
        let folder = Scratch::folder(&format!(
            "operator-run-{}",
            RUN.fetch_add(1, Ordering::Relaxed)
        ))
        .expect("a folder");
        operator_in(&folder, settings, args)
    }

    /// Runs the command in `folder`, as [`operator`] does.
    pub fn operator_in(folder: &Path, settings: &Settings, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_tetsu"))
            .arg("operator")
            .args(args)
            .current_dir(folder)
            .env("DATABASE_URL", settings.url())
            .env("DATABASE_TLS", "disable")
            .env_remove("DATABASE_CA_FILE")
            .env_remove("TETSU_ORIGIN")
            .env_remove("SUDO_USER")
            .env("USER", "tester")
            .output()
            .expect("the binary runs")
    }

    /// A migrated database holding one operator, Kurt Medley, as operator 1.
    pub async fn with_kurt() -> TestDatabase {
        let database = migrated_database(&[&Identity])
            .await
            .expect("a migrated test database");
        tetsu_identity::postgres_store(pool(&database.as_app()).expect("a pool"))
            .create(
                &OperatorName::parse("Kurt Medley").expect("a name"),
                None,
                &tetsu_identity::Actor::parse("kurt@workstation").expect("an actor"),
            )
            .await
            .expect("creates");
        database
    }

    pub fn out(output: &Output) -> String {
        String::from_utf8(output.stdout.clone()).expect("UTF-8")
    }

    pub fn err(output: &Output) -> String {
        String::from_utf8(output.stderr.clone()).expect("UTF-8")
    }
}

use support::{err, operator, operator_in, out, with_kurt};

/// A link never reaches a pipe, so no CI log or redirect captures one, and nothing is created when it is refused.
#[tokio::test]
async fn no_invite_link_is_printed_to_a_pipe() {
    let database = with_kurt().await;
    let app = database.as_app();
    for args in [
        &["create", "Lee Park", "lee@example.com"][..],
        &["invite", "1"],
    ] {
        let refused = operator(&app, args);
        assert_eq!(refused.status.code(), Some(1), "{args:?}");
        assert!(
            err(&refused).contains("an invite link prints only to a terminal"),
            "{refused:?}"
        );
        assert_eq!(out(&refused), "");
    }
    // The refused link was never issued, so the list shows no open one.
    assert_eq!(
        out(&operator(&app, &["list"])),
        "1  Kurt Medley  enabled  0 passkeys\n"
    );
}

/// The commands that print no link run anywhere, and say what they did.
#[tokio::test]
async fn operators_are_listed_logged_out_and_disabled() {
    let database = with_kurt().await;
    let app = database.as_app();
    for (args, said) in [
        (
            &["end-sessions", "1"][..],
            "ended 0 sessions of operator 1\n",
        ),
        (&["end-sessions", "--all"], "ended 0 operator sessions\n"),
        (
            &["disable", "1"],
            "disabled operator 1, ending 0 sessions and 0 invite links\n",
        ),
        (&["enable", "1"], "enabled operator 1, who logs in afresh\n"),
        (&["list"], "1  Kurt Medley  enabled  0 passkeys\n"),
    ] {
        let output = operator(&app, args);
        assert!(output.status.success(), "{args:?}: {output:?}");
        assert_eq!(out(&output), said, "{args:?}");
    }
    // Each change names who ran it: the login, at this machine.
    let actors = tetsu_postgres::connect(&database)
        .await
        .expect("connects")
        .query(
            "SELECT DISTINCT payload->>'actor' FROM identity.outbox_global WHERE event_type <> 'identity.operator_created'",
            &[],
        )
        .await
        .expect("reads");
    assert_eq!(actors.len(), 1);
    let actor: String = actors[0].try_get(0).expect("an actor");
    assert!(actor.starts_with("tester@"), "{actor}");
    // A mistyped ID fails, rather than reading as nothing to do.
    let missing = operator(&app, &["disable", "99"]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(err(&missing).contains("no operator 99"), "{missing:?}");
}

/// Words the command does not take are a usage error.
#[test]
fn unknown_operator_commands_are_usage_errors() {
    let settings = Settings::new("postgres://unused@127.0.0.1:1/unused");
    for args in [&["remove", "1"][..], &["disable", "one"]] {
        let output = operator(&settings, args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
    }
}

/// A `.env` in the folder the command runs in cannot change how a database the environment names is reached.
#[tokio::test]
async fn a_stray_env_file_cannot_turn_off_tls_for_a_named_database() {
    let database = with_kurt().await;
    let folder = Scratch::folder("operator-stray-env").expect("a folder");
    std::fs::write(folder.join(".env"), "DATABASE_CA_FILE=/nowhere.pem\n").expect("writes");
    let refused = operator_in(&folder, &database.as_app(), &["list"]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(
        err(&refused).contains("DATABASE_CA_FILE comes from .env but DATABASE_URL does not"),
        "{refused:?}"
    );
}

/// `--env demo` reads the demo's settings from `.env.demo` in place of `.env`, so a hosted environment's operators
/// are reached with nothing typed (ADR-0073).
#[tokio::test]
async fn an_environment_s_file_names_the_database_it_reaches() {
    let database = with_kurt().await;
    let folder = Scratch::folder("operator-named-env").expect("a folder");
    let url = database.as_app().url().to_string();
    std::fs::write(
        folder.join(".env.demo"),
        format!("DATABASE_URL={url}\nDATABASE_TLS=disable\n"),
    )
    .expect("writes");
    std::fs::write(
        folder.join(".env"),
        "DATABASE_URL=postgres://unused@127.0.0.1:1/unused\n",
    )
    .expect("writes");
    let output = Command::new(env!("CARGO_BIN_EXE_tetsu"))
        .args(["--env", "demo", "operator", "list"])
        .current_dir(&folder)
        .env_remove("DATABASE_URL")
        .env_remove("DATABASE_TLS")
        .env_remove("DATABASE_CA_FILE")
        .env_remove("TETSU_ORIGIN")
        .output()
        .expect("the binary runs");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(out(&output), "1  Kurt Medley  enabled  0 passkeys\n");
}
