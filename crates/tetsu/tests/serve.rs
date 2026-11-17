//! `tetsu serve`, run as the real binary against a seeded test database: the journey a visitor takes (#76).

#[cfg(test)]
mod support {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::Path;
    use std::process::{Child, ChildStdout, Command, Stdio};

    use tetsu_postgres::Settings;
    use tetsu_postgres::test_support::fresh_database;

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
            .env("TETSU_DEMO", "true")
            .env("TETSU_ORIGIN", "http://localhost:8080")
            .env("TETSU_TRUSTED_PROXIES", "0");
        // A developer's or CI's own OpenTelemetry settings never reach a test server.
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("OTEL_") {
                command.env_remove(name);
            }
        }
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
        let (server, before) = start_with(settings, &[("TETSU_DEMO", demo)]);
        assert!(before.is_empty(), "{before:?}");
        server
    }

    /// Starts the server with extra settings, waits until it says it is listening, and returns it with the lines
    /// it wrote before that. A free port is found by binding and releasing one, and the kernel may hand that port
    /// to another socket, such as a test's database connection, before the server binds it; a server that could not
    /// start is tried again on a fresh port, a few times, so that race never fails a test.
    pub fn start_with(settings: &Settings, env: &[(&str, &str)]) -> (Server, Vec<String>) {
        for _ in 0..4 {
            match try_start(settings, env) {
                Ok(started) => return started,
                Err(mut child) => {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
        try_start(settings, env)
            .unwrap_or_else(|_| panic!("the server could not start on five ports"))
    }

    /// Starts the server once, on a port found free a moment before, and gives the child back if it says it could
    /// not start.
    fn try_start(
        settings: &Settings,
        env: &[(&str, &str)],
    ) -> Result<(Server, Vec<String>), Child> {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let mut command = tetsu(settings, &["serve"]);
        command
            .env("PORT", port.to_string())
            .envs(env.iter().copied());
        let mut child = command
            .stdout(Stdio::piped())
            .spawn()
            .expect("the binary runs");
        let (stdout, mut before) = until_listening(BufReader::new(child.stdout.take().unwrap()));
        // A server that could not bind says so, then may log more as it stops, such as a refused export, so the
        // line is looked for anywhere, not only last.
        if before
            .iter()
            .any(|line| message(line).as_deref() == Some("could not start serving"))
        {
            return Err(child);
        }
        let line = before.pop().unwrap_or_default();
        assert_eq!(
            message(&line),
            Some(format!("serving on http://0.0.0.0:{port}")),
            "{line}"
        );
        Ok((
            Server {
                child,
                stdout,
                port,
            },
            before,
        ))
    }

    /// The lines the server writes up to and including the one saying it is listening, or a failure after 10
    /// seconds, so a server that never says it is listening fails the test rather than hanging it.
    fn until_listening(
        mut stdout: BufReader<ChildStdout>,
    ) -> (BufReader<ChildStdout>, Vec<String>) {
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut lines = Vec::new();
            loop {
                let mut line = String::new();
                if stdout.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                let listening = message(&line).is_some_and(|text| text.starts_with("serving on"));
                lines.push(line);
                if listening {
                    break;
                }
            }
            let _ = send.send((stdout, lines));
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

    /// One request a stand-in OTLP backend received: its head as text, and its body.
    pub type Received = (String, Vec<u8>);

    /// A stand-in OTLP backend on a free port. It answers every request with `status`, and passes on what it
    /// received before answering, so whatever the server waited on to stop has arrived.
    pub fn collector(status: u16) -> (String, std::sync::mpsc::Receiver<Received>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming().map_while(Result::ok) {
                let send = send.clone();
                std::thread::spawn(move || answer(stream, &send, status));
            }
        });
        (address, receive)
    }

    fn answer(stream: TcpStream, send: &std::sync::mpsc::Sender<Received>, status: u16) {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut stream = stream;
        loop {
            let mut head = String::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                head.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let length = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let _ = send.send((head, body));
            write!(
                stream,
                "HTTP/1.1 {status} Status\r\nContent-Length: 0\r\n\r\n"
            )
            .unwrap();
        }
    }

    /// What one visit sends to a stand-in backend answering `status`, with the server stopped by SIGTERM so it
    /// flushes: the requests the backend received, and the lines the server wrote after it started.
    #[cfg(unix)]
    pub async fn one_visit_exported(status: u16, env: &[(&str, &str)]) -> (Vec<Received>, String) {
        let database = fresh_database().await.expect("a test database");
        ready(&database);
        let (endpoint, received) = collector(status);
        let mut env = env.to_vec();
        env.push(("OTEL_EXPORTER_OTLP_ENDPOINT", &endpoint));
        let (mut server, before) = start_with(&database, &env);
        assert!(before.is_empty(), "{before:?}");
        get(&server, "/sample-studio?email=lee.park@example.com");
        let pid = server.child.id().to_string();
        assert!(
            Command::new("kill")
                .args(["-TERM", &pid])
                .status()
                .unwrap()
                .success()
        );
        assert!(server.child.wait().unwrap().success());
        let mut rest = String::new();
        server.stdout.read_to_string(&mut rest).unwrap();
        (received.try_iter().collect(), rest)
    }

    /// Every body the stand-in backend received, once each request is checked to be OTLP logs as protobuf over HTTP,
    /// with the key the server was given.
    pub fn exported(requests: &[Received]) -> Vec<u8> {
        assert!(!requests.is_empty(), "nothing was exported");
        let mut body = Vec::new();
        for (head, part) in requests {
            let lower = head.to_ascii_lowercase();
            assert!(head.starts_with("POST /v1/logs HTTP/1.1"), "{head}");
            assert!(lower.contains("authorization: basic dgvzda=="), "{head}");
            assert!(
                lower.contains("content-type: application/x-protobuf"),
                "{head}"
            );
            body.extend_from_slice(part);
        }
        body
    }

    pub const KEY: (&str, &str) = (
        "OTEL_EXPORTER_OTLP_HEADERS",
        "Authorization=Basic%20dGVzdA==",
    );

    pub fn get(server: &Server, path: &str) -> String {
        sent(
            server,
            &format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"),
        )
    }

    /// Begins a login as a browser on the site would, from the network `forwarded` names.
    pub fn begin_login(server: &Server, forwarded: &str) -> String {
        sent(
            server,
            &format!(
                "POST /ops/login/begin HTTP/1.1\r\nHost: localhost\r\nOrigin: http://localhost:8080\r\n\
                 X-Forwarded-For: {forwarded}\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\
                 Connection: close\r\n\r\n{{}}"
            ),
        )
    }

    fn sent(server: &Server, request: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut raw = String::new();
        stream.read_to_string(&mut raw).unwrap();
        raw
    }
}

use std::io::Read;
use std::process::Command;

#[cfg(unix)]
use support::{KEY, exported, one_visit_exported};
use support::{
    begin_login, get, logged, message, ready, refused, served, start, start_marked, start_with,
    tetsu,
};
use tetsu_postgres::test_support::fresh_database;

/// The operator's login page is served, under a policy that lets it run Tetsu's own script, and the script it
/// names is served as JavaScript.
#[tokio::test]
async fn the_login_page_loads_its_script() {
    let database = fresh_database().await.expect("a test database");
    ready(&database);
    let server = start(&database);
    let page = get(&server, "/ops/login");
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert!(page.contains("script-src 'self'"), "{page}");
    let (_, after) = page
        .split_once("<script type=\"module\" src=\"")
        .expect("a script");
    let script = get(&server, after.split('"').next().unwrap_or_default());
    assert!(script.starts_with("HTTP/1.1 200"), "{script}");
    assert!(
        script
            .to_ascii_lowercase()
            .contains("content-type: text/javascript"),
        "{script}"
    );
}

#[tokio::test]
async fn a_visitor_sees_a_tenants_page_and_tetsus_page() {
    let database = fresh_database().await.expect("a test database");
    ready(&database);
    let server = start(&database);

    let tenant = get(&server, "/sample-studio");
    assert!(tenant.starts_with("HTTP/1.1 200"), "{tenant}");
    assert!(tenant.contains("<h1>Sample Studio</h1>"), "{tenant}");
    assert!(tenant.contains("<li>Main Street</li>"), "{tenant}");
    assert!(tenant.contains("<li>Riverside</li>"), "{tenant}");
    assert!(!tenant.contains("Demo business."), "{tenant}");
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
}

#[tokio::test]
async fn a_visitor_finds_nothing_at_an_address_no_one_serves() {
    let database = fresh_database().await.expect("a test database");
    ready(&database);
    let server = start(&database);

    let missing = get(&server, "/another-studio");
    assert!(missing.starts_with("HTTP/1.1 404"), "{missing}");
    assert!(!missing.contains("Sample Studio"), "{missing}");

    // A deeper path reaches the site's own fallback, and its not found page.
    let deeper = get(&server, "/sample-studio/extra");
    assert!(deeper.starts_with("HTTP/1.1 404"), "{deeper}");
    assert!(
        deeper.contains("There is nothing booked at this address."),
        "{deeper}"
    );
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
    assert_eq!(line["span"]["http.request.method"], "GET", "{text}");
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
    assert_eq!(field(0, "http.route"), "/{address}");
    assert!(field(0, "tenant_id").is_i64(), "{:?}", requests[0]);
    assert_eq!(requests[1]["status"], 404);
    assert_eq!(field(1, "http.route"), "/{address}");
    assert!(
        field(1, "tenant_id").is_null(),
        "an address no tenant holds"
    );
    assert_eq!(requests[2]["status"], 404);
    assert!(field(2, "http.route").is_null(), "no route answered");
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
    let stdout = String::from_utf8_lossy(&output.stdout);
    let failed: serde_json::Value = stdout
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .find(|line: &serde_json::Value| line["alert"] == "start_failed")
        .unwrap_or_else(|| panic!("a start_failed line in {stdout}"));
    assert_eq!(failed["level"], "ERROR", "{stdout}");
    assert_eq!(failed["message"], "could not start serving", "{stdout}");
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

/// Behind one proxy, a network that begins too many logins is refused with when to try again, keyed on the
/// entry the proxy added, whatever the client put before it; another network still begins (ADR-0072).
#[tokio::test]
async fn a_flood_of_logins_from_one_network_is_refused_and_no_other() {
    let database = fresh_database().await.expect("a test database");
    ready(&database);
    let (server, _) = start_with(&database, &[("TETSU_TRUSTED_PROXIES", "1")]);
    for n in 0..30 {
        let begun = begin_login(&server, &format!("203.0.113.{n}, 192.0.2.1"));
        assert!(begun.starts_with("HTTP/1.1 200"), "{n}: {begun}");
    }
    let refused = begin_login(&server, "198.51.100.1, 192.0.2.1");
    assert!(refused.starts_with("HTTP/1.1 429"), "{refused}");
    assert!(
        refused.to_ascii_lowercase().contains("retry-after: 30"),
        "{refused}"
    );
    assert!(refused.contains("Try again in a minute"), "{refused}");
    let other = begin_login(&server, "192.0.2.1, 192.0.2.2");
    assert!(other.starts_with("HTTP/1.1 200"), "{other}");
}

/// Rate limits key on a requester's network, so serving refuses to start without knowing how many proxies stand
/// in front, rather than guess.
#[tokio::test]
async fn it_refuses_to_start_without_its_proxy_count() {
    let database = fresh_database().await.expect("a test database");
    let output = tetsu(&database, &["serve"])
        .env_remove("TETSU_TRUSTED_PROXIES")
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("TETSU_TRUSTED_PROXIES must be set"),
        "{stderr}"
    );
}

/// The operator pages need the site's own origin, for their passkeys and to tell their requests from another
/// site's, so serving refuses to start without it.
#[tokio::test]
async fn it_refuses_to_start_without_its_origin() {
    let database = fresh_database().await.expect("a test database");
    let output = tetsu(&database, &["serve"])
        .env_remove("TETSU_ORIGIN")
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("TETSU_ORIGIN must be set"), "{stderr}");
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
        .env("TETSU_TRUSTED_PROXIES", "0")
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

/// A bad export setting is a warning, and the site serves anyway, logging to standard output alone (ADR-0049).
#[tokio::test]
async fn a_bad_export_setting_is_a_warning_and_serving_goes_on() {
    let database = fresh_database().await.expect("a test database");
    ready(&database);
    let (server, before) = start_with(
        &database,
        &[
            ("OTEL_EXPORTER_OTLP_ENDPOINT", "https://otlp.example"),
            ("OTEL_EXPORTER_OTLP_COMPRESSION", "zstd"),
        ],
    );
    assert_eq!(before.len(), 1, "{before:?}");
    let warning: serde_json::Value = serde_json::from_str(&before[0]).unwrap();
    assert_eq!(warning["level"], "WARN", "{warning}");
    assert_eq!(
        warning["message"],
        "logs are not exported: OTEL_EXPORTER_OTLP_COMPRESSION must be gzip or none"
    );
    assert!(get(&server, "/sample-studio").starts_with("HTTP/1.1 200"));
}

/// With an endpoint set, every line reaches the OTLP backend as protobuf over HTTP, with the key it was given,
/// the reviewed span fields, and the service name an attribute cannot replace, and what is batched is sent before
/// the server stops (ADR-0049).
#[cfg(unix)]
#[tokio::test]
async fn logs_reach_an_otlp_backend_and_are_flushed_at_stop() {
    let (requests, _) = one_visit_exported(
        200,
        &[KEY, ("OTEL_RESOURCE_ATTRIBUTES", "service.name=renamed")],
    )
    .await;
    let body = exported(&requests);
    let at = |text: &str| {
        body.windows(text.len())
            .position(|window| window == text.as_bytes())
    };
    for sent in [
        "served",
        "stopped",
        "request_id",
        "realm",
        "customer",
        "http.request.method",
        "http.route",
        "/{address}",
        "tenant_id",
    ] {
        assert!(at(sent).is_some(), "{sent} was not exported");
    }
    let named = at("service.name").expect("a service name");
    assert!(
        body[named..named + 24]
            .windows(5)
            .any(|window| window == b"tetsu"),
        "the service is tetsu"
    );
    for kept in ["lee.park", "sample-studio", "renamed"] {
        assert!(at(kept).is_none(), "{kept} was exported");
    }
}

/// Compression is a setting, so a backend that wants gzip gets it.
#[cfg(unix)]
#[tokio::test]
async fn logs_are_gzipped_when_set() {
    let (requests, _) =
        one_visit_exported(200, &[KEY, ("OTEL_EXPORTER_OTLP_COMPRESSION", "gzip")]).await;
    assert!(!requests.is_empty(), "nothing was exported");
    for (head, _) in &requests {
        assert!(
            head.to_ascii_lowercase().contains("content-encoding: gzip"),
            "{head}"
        );
    }
}

/// A backend that refuses the key leaves the site serving, and the refusal on standard output, where the host
/// keeps it, rather than nowhere.
#[cfg(unix)]
#[tokio::test]
async fn a_refused_export_is_reported_on_standard_output() {
    let (requests, rest) = one_visit_exported(401, &[KEY]).await;
    assert!(!requests.is_empty(), "nothing was sent");
    assert!(
        rest.lines()
            .any(|line| line.contains("\"level\":\"ERROR\"") && line.contains("ExportError")),
        "{rest}"
    );
}
