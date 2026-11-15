//! `tetsu operator` against a real Postgres, its output piped as CI or a log would take it (ADR-0040).

use std::process::{Command, Output};

use tetsu_identity::{Identity, OperatorName};
use tetsu_postgres::test_support::{TestDatabase, fresh_database};
use tetsu_postgres::{Settings, migrate, pool};

#[cfg(test)]
mod support {
    use super::*;

    pub fn operator(settings: &Settings, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_tetsu"))
            .arg("operator")
            .args(args)
            .env("DATABASE_URL", settings.url())
            .env("DATABASE_TLS", "disable")
            .env("TETSU_ORIGIN", "https://ops.example.com")
            .output()
            .expect("the binary runs")
    }

    /// A migrated database holding one operator, Kurt Medley, as operator 1.
    pub async fn with_kurt() -> TestDatabase {
        let database = fresh_database().await.expect("a test database");
        migrate(&database, &[&Identity]).await.expect("migrates");
        tetsu_identity::postgres_store(pool(&database.as_app()).expect("a pool"))
            .create(
                &OperatorName::parse("Kurt Medley").expect("a name"),
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

use support::{err, operator, out, with_kurt};

/// A link never reaches a pipe, so no CI log or redirect captures one, and nothing is created when it is refused.
#[tokio::test]
async fn no_enrollment_link_is_printed_to_a_pipe() {
    let database = with_kurt().await;
    let app = database.as_app();
    for args in [&["create", "Lee Park"][..], &["enroll", "1"]] {
        let refused = operator(&app, args);
        assert_eq!(refused.status.code(), Some(1), "{args:?}");
        assert!(
            err(&refused).contains("an enrollment link prints only to a terminal"),
            "{refused:?}"
        );
        assert_eq!(out(&refused), "");
    }
    assert_eq!(out(&operator(&app, &["list"])), "1  Kurt Medley  enabled\n");
}

/// The commands that print no link run anywhere, and say what they did.
#[tokio::test]
async fn operators_are_listed_signed_out_and_disabled() {
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
            "disabled operator 1, and ended their sessions and enrollment link\n",
        ),
        (&["list"], "1  Kurt Medley  disabled\n"),
    ] {
        let output = operator(&app, args);
        assert!(output.status.success(), "{args:?}: {output:?}");
        assert_eq!(out(&output), said, "{args:?}");
    }
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
