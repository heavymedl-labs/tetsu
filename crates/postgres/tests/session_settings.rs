//! ADR-0042: nothing sets a session-level setting on a pooled connection, since the pool keeps it for the next
//! request (#53). Code sets what it needs with transaction scope through the scoped transactions instead.
//!
//! This scans SQL in every crate's `src` and `sql` folders: string literals in Rust, and whole statements in SQL
//! files. Migrations may set their own timeouts, since they run on the runner's direct connection, which resets the
//! session after each file, and so may the runner and the test support, which never touch a pooled connection.

use std::path::{Path, PathBuf};

const EXEMPT: &[&str] = &["postgres/src/migrate.rs", "postgres/src/test_support.rs"];

/// Each statement in `sql` that changes the session rather than the transaction.
fn session_settings(sql: &str) -> Vec<String> {
    sql.split(';')
        .map(|statement| {
            statement
                .lines()
                .filter(|line| !line.trim_start().starts_with("--"))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .map(|statement| statement.trim().to_string())
        .filter(|statement| {
            // Keywords only in capitals, as the SQL formatter writes them, so prose in a string is not read as SQL.
            let text = statement.as_str();
            let session_set = text.starts_with("SET ") && !text.starts_with("SET LOCAL ");
            let reset = text.starts_with("RESET ");
            let lasting_config =
                text.matches("set_config(").count() > text.matches(", true)").count();
            session_set || reset || lasting_config
        })
        .collect()
}

/// The contents of every string literal in Rust source, skipping comments and character literals.
fn strings(source: &str) -> Vec<String> {
    let (mut found, mut chars) = (Vec::new(), source.chars().peekable());
    while let Some(c) = chars.next() {
        match c {
            '/' if chars.peek() == Some(&'/') => {
                chars.by_ref().find(|&c| c == '\n');
            }
            '\'' => {
                // A character literal such as '"' or '\'', or else a lifetime, which ends at no quote.
                let literal: String = chars.clone().take(3).collect();
                if literal.starts_with("\"'") || literal.starts_with("\\'") {
                    chars.nth(1);
                }
            }
            'r' if chars.peek() == Some(&'#') || chars.peek() == Some(&'"') => {
                let hashes = chars.clone().take_while(|&c| c == '#').count();
                if chars.clone().nth(hashes) == Some('"') {
                    chars.nth(hashes);
                    let end = format!("\"{}", "#".repeat(hashes));
                    let mut text = String::new();
                    while !text.ends_with(&end) {
                        let Some(c) = chars.next() else { break };
                        text.push(c);
                    }
                    found.push(text.trim_end_matches(&end).to_string());
                }
            }
            '"' => {
                let mut text = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            chars.next();
                        }
                        '"' => break,
                        _ => text.push(c),
                    }
                }
                found.push(text);
            }
            _ => {}
        }
    }
    found
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(&path, out);
        } else if path
            .extension()
            .is_some_and(|ext| ext == "rs" || ext == "sql")
        {
            out.push(path);
        }
    }
}

#[test]
fn no_code_sets_a_session_level_setting() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&crates)
        .expect("lists the crates")
        .flatten()
    {
        for folder in ["src", "sql"] {
            files(&entry.path().join(folder), &mut paths);
        }
    }
    assert!(paths.len() > 10, "found the sources: {paths:?}");
    let mut found = Vec::new();
    for path in paths {
        let shown = path
            .strip_prefix(&crates)
            .unwrap_or(&path)
            .display()
            .to_string();
        if EXEMPT.contains(&shown.as_str()) {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("reads a source");
        let sql = if path.extension().is_some_and(|ext| ext == "sql") {
            vec![source]
        } else {
            strings(&source)
        };
        for statement in sql.iter().flat_map(|sql| session_settings(sql)) {
            found.push(format!("{shown}: {statement}"));
        }
    }
    assert!(found.is_empty(), "session-level settings: {found:#?}");
}

#[test]
fn the_scan_catches_each_kind_and_passes_transaction_scope() {
    for caught in [
        "SET statement_timeout = '1s'",
        "RESET ROLE",
        "SELECT set_config('app.tenant_id', $1, false)",
        "BEGIN; SET search_path TO x; COMMIT",
    ] {
        assert_eq!(session_settings(caught).len(), 1, "{caught}");
    }
    for passed in [
        "SET LOCAL statement_timeout = '1s'",
        "SELECT set_config('role', $1, true), set_config('lock_timeout', $2, true)",
        "UPDATE t SET a = 1",
        "GRANT x TO y WITH INHERIT FALSE, SET TRUE",
        "-- SET in a comment\nSELECT 1",
        "set PORT to choose the port",
    ] {
        assert_eq!(session_settings(passed), Vec::<String>::new(), "{passed}");
    }
    let source = r##"let a = "SET x = 1"; // "RESET y" in a comment
        let b = '"'; let c = r#"RESET ROLE"#; let d = "escaped \" SET LOCAL z = 1";"##;
    assert_eq!(
        strings(source),
        ["SET x = 1", "RESET ROLE", "escaped  SET LOCAL z = 1"]
    );
}
