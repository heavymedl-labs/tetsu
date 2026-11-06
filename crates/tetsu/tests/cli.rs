//! Runs the real binary.

use std::process::Command;

#[test]
fn prints_its_version_and_exits_cleanly() {
    let output = Command::new(env!("CARGO_BIN_EXE_tetsu"))
        .output()
        .expect("the binary runs");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("output is UTF-8");
    assert_eq!(
        stdout.trim(),
        format!("tetsu {}, modules: none", env!("CARGO_PKG_VERSION"))
    );
}
