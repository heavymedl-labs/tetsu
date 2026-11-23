//! Formats every HTML template in place, or with `--check` lists those that are not formatted and fails
//! (ADR-0083). Run it from the repository's root.

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let check = std::env::args().skip(1).any(|arg| arg == "--check");
    let problems = tetsu_format_templates::run(Path::new("."), check);
    for problem in &problems {
        eprintln!("error: {problem}");
    }
    if problems.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
