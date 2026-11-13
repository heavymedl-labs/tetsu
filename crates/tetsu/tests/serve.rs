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

    /// Starts the server on a free port in a demo environment, and waits until it says it is listening.
    pub fn start(settings: &Settings) -> Server {
        start_marked(settings, "true")
    }

    pub fn start_marked(settings: &Settings, demo: &str) -> Server {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let mut child = tetsu(settings, &["serve"])
            .env("PORT", port.to_string())
            .env("TETSU_DEMO", demo)
            .stdout(Stdio::piped())
            .spawn()
            .expect("the binary runs");
        let (stdout, line) = first_line(BufReader::new(child.stdout.take().unwrap()));
        assert_eq!(
            message(&line),
            Some(format!("serving on http://0.0.0.0:{port}")),
            "{line}"
        );
        Server {
            child,
            stdout,
            port,
        }
    }

    /// The first line the server writes, or a failure after 10 seconds, so a server that never says it is
    /// listening fails the test rather than hanging it.
    fn first_line(mut stdout: BufReader<ChildStdout>) -> (BufReader<ChildStdout>, String) {
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            let _ = stdout.read_line(&mut line);
            let _ = send.send((stdout, line));
        });
        receive
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the server says it is listening within 10 seconds")
    }

    /// Runs `serve` expecting it to refuse to start, and gives up after 10 seconds, so a server that starts
    /// anyway fails the test rather than hanging it.
    pub fn refused(command: &mut Command) -> std::process::Output {
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the binary runs");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while child.try_wait().expect("checks the child").is_none() {
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                panic!("serve started instead of refusing");
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        child.wait_with_output().expect("collects its output")
    }

    /// A log line's message, if the line is a JSON log line with one.
    pub fn message(line: &str) -> Option<String> {
        let line: serde_json::Value = serde_json::from_str(line).ok()?;
        line["message"].as_str().map(ToString::to_string)
    }

    /// Stops the server and returns every log line it wrote after starting, each checked to be JSON.
    pub fn logged(mut server: Server) -> Vec<serde_json::Value> {
        server.child.kill().unwrap();
        server.child.wait().unwrap();
        let mut rest = String::new();
        server.stdout.read_to_string(&mut rest).unwrap();
        rest.lines()
            .map(|line| serde_json::from_str(line).unwrap_or_else(|_| panic!("not JSON: {line}")))
            .collect()
    }

    /// The line logged for each request served, in order.
    pub fn served(lines: &[serde_json::Value]) -> Vec<&serde_json::Value> {
        lines
            .iter()
            .filter(|line| line["message"] == "served")
            .collect()
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

use support::{get, logged, message, ready, refused, served, start, start_marked, tetsu};
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
    assert!(
        home.contains("<span class=\"sr-only\">Tetsu: </span>Booking software,"),
        "{home}"
    );

    let missing = get(&server, "/another-studio");
    assert!(missing.starts_with("HTTP/1.1 404"), "{missing}");
    assert!(!missing.contains("Sample Studio"), "{missing}");

    // The tenant's colors come as a stylesheet of their own, never sniffed as anything else.
    let theme = get(&server, "/sample-studio/theme.css");
    assert!(theme.starts_with("HTTP/1.1 200"), "{theme}");
    let theme_head = theme.to_ascii_lowercase();
    assert!(
        theme_head.contains("content-type: text/css; charset=utf-8"),
        "{theme}"
    );
    assert!(
        theme_head.contains("x-content-type-options: nosniff"),
        "{theme}"
    );

    // A deeper path reaches the site's own fallback, and its not found page.
    let deeper = get(&server, "/sample-studio/extra");
    assert!(deeper.starts_with("HTTP/1.1 404"), "{deeper}");
    assert!(deeper.contains("Nothing is at this address."), "{deeper}");
    assert!(!deeper.contains("Sample Studio"), "{deeper}");
}

/// A request's line carries the request's context and timing, and nothing from its path or query.
fn names_no_path(line: &serde_json::Value) {
    let text = line.to_string();
    for private in [
        "lee.park",
        "email",
        "sample-studio",
        "another-studio",
        "secret-token",
    ] {
        assert!(!text.contains(private), "{text}");
    }
    assert_eq!(line["level"], "INFO", "{text}");
    assert_eq!(line["span"]["realm"], "customer", "{text}");
    assert_eq!(line["span"]["method"], "GET", "{text}");
    assert!(line["ms"].is_u64(), "{text}");
    assert!(line["timestamp"].is_string(), "{text}");
}

/// Each request logs one JSON line with its request ID, realm, route, and tenant, and never its path or query,
/// which could carry a token or an email (ADR-0045).
#[tokio::test]
async fn each_request_logs_one_line_that_names_no_path() {
    let database = fresh_database().await.expect("a test database");
    ready(&database);
    let server = start(&database);
    for path in [
        "/sample-studio?email=lee.park@example.com",
        "/another-studio",
        "/sample-studio/secret-token",
    ] {
        get(&server, path);
    }
    let lines = logged(server);
    let requests = served(&lines);
    assert_eq!(requests.len(), 3, "{lines:?}");
    for line in &requests {
        names_no_path(line);
    }
    let field = |index: usize, name: &str| requests[index]["span"][name].clone();
    assert_eq!(requests[0]["status"], 200);
    assert_eq!(field(0, "route"), "/{address}");
    assert!(field(0, "tenant_id").is_i64(), "{:?}", requests[0]);
    assert_eq!(requests[1]["status"], 404);
    assert_eq!(field(1, "route"), "/{address}");
    assert!(
        field(1, "tenant_id").is_null(),
        "an address no tenant holds"
    );
    assert_eq!(requests[2]["status"], 404);
    assert!(field(2, "route").is_null(), "no route answered");
    let ids: std::collections::HashSet<String> = requests
        .iter()
        .map(|line| {
            line["span"]["request_id"]
                .as_str()
                .expect("an ID")
                .to_string()
        })
        .collect();
    assert_eq!(ids.len(), 3, "each request has its own ID");
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
    assert_eq!(message(rest.trim()), Some("stopped".into()), "{rest}");
}

/// Serving checks tenants' addresses before it listens, so a database never migrated stops it at start.
#[tokio::test]
async fn it_refuses_to_start_before_migrating() {
    let database = fresh_database().await.expect("a test database");
    let output = refused(&mut tetsu(&database, &["serve"]));
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("could not check tenants' addresses"),
        "{stderr}"
    );
}

/// A site route would hide a tenant whose address a later release reserved, so serving refuses to start (#90).
#[tokio::test]
async fn it_refuses_to_start_while_a_tenant_holds_a_reserved_address() {
    let database = fresh_database().await.expect("a test database");
    ready(&database);
    tetsu_postgres::connect(&database)
        .await
        .expect("connects")
        .batch_execute("UPDATE tenancy.addresses SET address = '/login'")
        .await
        .expect("stores an address reserved since");
    let output = refused(&mut tetsu(&database, &["serve"]));
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("/login"), "{stderr}");
    assert!(stderr.contains("reserved"), "{stderr}");
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

#[tokio::test]
async fn outside_a_demo_environment_no_page_is_marked_as_a_demo() {
    let database = fresh_database().await.expect("a test database");
    ready(&database);
    let server = start_marked(&database, "false");
    for path in ["/sample-studio", "/", "/another-studio"] {
        let raw = get(&server, path);
        let lower = raw.to_ascii_lowercase();
        assert!(!raw.contains("Demo"), "{path}: {raw}");
        assert!(!lower.contains("noindex"), "{path}: {raw}");
    }
    assert!(get(&server, "/sample-studio").contains("<h1>Sample Studio</h1>"));
}

#[tokio::test]
async fn a_page_that_cannot_load_tells_the_operator_why_but_not_the_visitor() {
    // Tenancy's tables go once it is serving, so every lookup fails.
    let database = fresh_database().await.expect("a test database");
    ready(&database);
    let server = start(&database);
    tetsu_postgres::connect(&database)
        .await
        .expect("connects")
        .batch_execute("DROP SCHEMA tenancy CASCADE")
        .await
        .expect("drops Tenancy's tables");
    let page = get(&server, "/sample-studio");
    assert!(page.starts_with("HTTP/1.1 503"), "{page}");
    assert!(!page.contains("relation"), "{page}");
    let lines = logged(server);
    let reported = lines
        .iter()
        .find(|line| line["level"] == "ERROR")
        .unwrap_or_else(|| panic!("an error line in {lines:?}"));
    assert!(
        reported["message"]
            .as_str()
            .is_some_and(|text| text.starts_with("tenancy: could not load a page: ")
                && !text.contains("sample-studio")),
        "{reported}"
    );
    assert!(reported["span"]["request_id"].is_string(), "{reported}");
}
