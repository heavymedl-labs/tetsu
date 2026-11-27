//! `tetsu staff` as a process, its output piped as CI or a log would take it (ADR-0040, ADR-0089).

use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use tetsu_postgres::test_support::Scratch;

#[cfg(test)]
mod support {
    use super::*;

    /// Runs the command with words `args`, in an empty folder, so no `.env` a developer keeps can change what it does,
    /// against a database nothing answers at, which no refusal reaches.
    pub fn staff(args: &[&str]) -> Output {
        // A folder of its own each run, since tests run at once and each removes its folder when done.
        static RUN: AtomicUsize = AtomicUsize::new(0);
        let folder = Scratch::folder(&format!(
            "staff-run-{}",
            RUN.fetch_add(1, Ordering::Relaxed)
        ))
        .expect("a folder");
        Command::new(env!("CARGO_BIN_EXE_tetsu"))
            .arg("staff")
            .args(args)
            .current_dir(&*folder)
            .env("DATABASE_URL", "postgres://unused@127.0.0.1:1/unused")
            .env("TETSU_STAFF_ORIGIN", "https://tetsu.example")
            .output()
            .expect("the binary runs")
    }
}

use support::staff;

/// An invite link never reaches a pipe, and the command is refused before it reaches the database.
#[test]
fn no_staff_invite_link_is_printed_to_a_pipe() {
    let refused = staff(&[
        "invite",
        "fern-studio",
        "Ada Park",
        "ada@example.com",
        "--role",
        "owner",
    ]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&refused.stderr)
            .contains("an invite link prints only to a terminal"),
        "{refused:?}"
    );
    assert_eq!(String::from_utf8_lossy(&refused.stdout), "");
}

/// Words the command does not take are usage errors.
#[test]
fn wrong_staff_commands_are_usage_errors() {
    for args in [
        &["invite", "fern-studio", "Ada Park", "ada@example.com"][..],
        &[
            "invite",
            "fern-studio",
            "Ada Park",
            "ada@example.com",
            "--role",
            "chef",
        ],
        &["list"],
    ] {
        let output = staff(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("staff invite <business>"),
            "{output:?}"
        );
    }
}
