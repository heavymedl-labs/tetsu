//! Runs the real binary.

#[cfg(test)]
mod support {
    use std::process::{Command, Output};

    pub fn tetsu(args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_tetsu"))
            .args(args)
            .env_remove("DATABASE_URL")
            .env_remove("DATABASE_TLS")
            .output()
            .expect("the binary runs")
    }

    pub fn stderr(output: &Output) -> String {
        String::from_utf8(output.stderr.clone()).expect("output is UTF-8")
    }
}

use support::{stderr, tetsu};

#[test]
fn with_no_command_it_prints_its_version_and_modules() {
    let output = tetsu(&[]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("output is UTF-8");
    assert_eq!(
        stdout.trim(),
        format!("tetsu {}, modules: tenancy", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn an_unknown_command_prints_the_usage_and_exits_2() {
    let output = tetsu(&["serve-everything"]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = stderr(&output);
    assert!(
        stderr.contains("unknown command \"serve-everything\""),
        "{stderr}"
    );
    assert!(stderr.contains("usage: tetsu"), "{stderr}");
}

#[test]
fn migrate_takes_no_arguments() {
    let output = tetsu(&["migrate", "now"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("usage: tetsu"));
}

#[test]
fn migrate_needs_a_database_url() {
    let output = tetsu(&["migrate"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("DATABASE_URL must be set"));
}
