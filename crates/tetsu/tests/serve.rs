//! `tetsu serve`, run as the real binary against a seeded test database: the journey a visitor takes (#76).

#[cfg(test)]
mod support {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::Path;
    use std::process::{Child, ChildStdout, Command, Stdio};

    use tetsu_postgres::Settings;

    /// A running `tetsu serve`, stopped when the test ends, even if it fails.
    pub struct Server {
        pub child: Child,
        pub stdout: BufReader<ChildStdout>,
        port: u16,
    }

    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    pub fn tetsu(settings: &Settings, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tetsu"));
        command
            .args(args)
            .env("DATABASE_URL", settings.url())
            .env("DATABASE_TLS", "disable")
            .env("TETSU_DEMO", "true");
        command
    }

    pub fn ready(settings: &Settings) {
        for args in [&["migrate"][..], &["seed", sample().to_str().unwrap()]] {
            let output = tetsu(settings, args).output().expect("the binary runs");
            assert!(output.status.success(), "{output:?}");
        }
    }

    pub fn sample() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.toml")
    }

    /// Starts the server on a free port, and waits until it says it is listening.
    pub fn start(settings: &Settings) -> Server {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let mut child = tetsu(settings, &["serve"])
            .env("PORT", port.to_string())
            .stdout(Stdio::piped())
            .spawn()
            .expect("the binary runs");
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        assert_eq!(line.trim(), format!("serving on http://0.0.0.0:{port}"));
        Server {
            child,
            stdout,
            port,
        }
    }

    pub fn get(server: &Server, path: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut raw = String::new();
        stream.read_to_string(&mut raw).unwrap();
        raw
    }
}

use std::io::Read;
use std::process::Command;

use support::{get, ready, start, tetsu};
use tetsu_postgres::test_support::fresh_database;

#[tokio::test]
async fn a_visitor_sees_a_tenants_page_tetsus_page_and_not_found_elsewhere() {
    let database = fresh_database().await.expect("a test database");
    ready(&database);
    let server = start(&database);

    let tenant = get(&server, "/sample-studio");
    assert!(tenant.starts_with("HTTP/1.1 200"), "{tenant}");
    assert!(tenant.contains("<h1>Sample Studio</h1>"), "{tenant}");
    assert!(tenant.contains("<li>Main Street</li>"), "{tenant}");
    assert!(tenant.contains("<li>Riverside</li>"), "{tenant}");
    assert!(tenant.contains("Demo business."), "{tenant}");
    let lower = tenant.to_ascii_lowercase();
    assert!(
        lower.contains("strict-transport-security: max-age=31536000"),
        "{tenant}"
    );
    assert!(lower.contains("x-robots-tag: noindex"), "{tenant}");

    let home = get(&server, "/");
    assert!(home.starts_with("HTTP/1.1 200"), "{home}");
    assert!(home.contains("<h1>Tetsu</h1>"), "{home}");

    let missing = get(&server, "/another-studio");
    assert!(missing.starts_with("HTTP/1.1 404"), "{missing}");
    assert!(!missing.contains("Sample Studio"), "{missing}");
}

#[cfg(unix)]
#[tokio::test]
async fn it_stops_cleanly_when_the_host_sends_sigterm() {
    let database = fresh_database().await.expect("a test database");
    ready(&database);
    let mut server = start(&database);
    let pid = server.child.id().to_string();
    let killed = Command::new("kill").args(["-TERM", &pid]).status().unwrap();
    assert!(killed.success());
    let status = server.child.wait().unwrap();
    assert!(status.success(), "{status}");
    let mut rest = String::new();
    server.stdout.read_to_string(&mut rest).unwrap();
    assert_eq!(rest.trim(), "stopped");
}

#[tokio::test]
async fn it_refuses_to_start_without_the_demo_marking() {
    let database = fresh_database().await.expect("a test database");
    let output = tetsu(&database, &["serve"])
        .env_remove("TETSU_DEMO")
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("TETSU_DEMO must be set to true or false"),
        "{stderr}"
    );
}

#[test]
fn it_refuses_to_start_without_a_database() {
    let output = Command::new(env!("CARGO_BIN_EXE_tetsu"))
        .arg("serve")
        .env("TETSU_DEMO", "true")
        .env_remove("DATABASE_URL")
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("DATABASE_URL must be set"));
}

#[tokio::test]
async fn it_refuses_a_port_that_is_not_one() {
    let database = fresh_database().await.expect("a test database");
    let output = tetsu(&database, &["serve"])
        .env("PORT", "eighty")
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("PORT must be a number"),
        "{output:?}"
    );
}
