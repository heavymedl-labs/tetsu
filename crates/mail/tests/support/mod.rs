//! A local Mailpit for the SMTP tests: implicit TLS with a certificate made for the test, and a login it checks,
//! as a provider would. It stops, and its files go, when the test ends.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use serde_json::Value;
use tetsu_kernel::{Address, Message};
use tetsu_mail::{Allowlist, Mail, Smtp, SmtpSettings};
use tetsu_postgres::test_support::{Scratch, TestDatabase, migrated_database};
use tetsu_postgres::{Pool, pool};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub const USER: &str = "tetsu";
pub const PASSWORD: &str = "local-test-password";

/// A migrated Mail database and a pool on it as Tetsu's app role, as the server connects. Keep the database until the
/// test ends.
pub async fn database() -> (Pool, TestDatabase) {
    let database = migrated_database(&[&Mail])
        .await
        .expect("a migrated test database");
    (pool(&database.as_app()).expect("pool"), database)
}

/// Runs `sql` as the test database's owner, such as to take a right from Mail's role or break a column.
pub async fn as_owner(database: &TestDatabase, sql: &str) {
    tetsu_postgres::connect(database)
        .await
        .expect("connects")
        .batch_execute(sql)
        .await
        .expect("runs");
}

/// How a key stands in `mail.sends`, read as the owner.
pub async fn route(database: &TestDatabase, key: &str) -> Option<String> {
    tetsu_postgres::connect(database)
        .await
        .expect("connects")
        .query_opt(
            "SELECT route FROM mail.sends WHERE message_key = $1",
            &[&key],
        )
        .await
        .expect("reads")
        .map(|row| row.try_get("route").expect("a route"))
}

/// Ages a claim past its two minutes, as if the attempt holding it had stopped.
pub async fn lapse(database: &TestDatabase, key: &str) {
    as_owner(
        database,
        &format!(
            "UPDATE mail.sends SET claimed_at = now() - interval '3 minutes' WHERE message_key = '{key}'"
        ),
    )
    .await;
}

pub fn allow(text: &str) -> Allowlist {
    Allowlist::parse(text).expect("an allowlist")
}

pub fn address(text: &str) -> Address {
    Address::parse(text).expect("a plain address")
}

pub fn message(to: &str, key: &str) -> Message {
    Message::new(
        address(to),
        "Your invite",
        "Open the link.",
        "<p>Open the link.</p>",
        key,
    )
    .expect("a message")
}

/// A running Mailpit, its files, and the certificate it serves.
pub struct Mailpit {
    child: Child,
    pub smtp: u16,
    http: u16,
    pub certificate: Vec<u8>,
    _files: Scratch,
}

impl Mailpit {
    /// Starts Mailpit on free local ports, requiring implicit TLS and [`USER`]'s login.
    pub async fn start() -> Self {
        Self::start_allowing(None).await
    }

    /// As [`Self::start`], refusing for good every recipient `allowed` does not match.
    pub async fn start_allowing(allowed: Option<&str>) -> Self {
        static STARTED: AtomicU32 = AtomicU32::new(0);
        let name = format!("mailpit-{}", STARTED.fetch_add(1, Ordering::Relaxed));
        let files = Scratch::folder(&name).expect("a scratch folder");
        let (_, cert) = certificate(&files);
        std::fs::write(files.join("auth"), format!("{USER}:{PASSWORD}\n")).expect("an auth file");
        let (smtp, http) = (free_port(), free_port());
        let mailpit = Self {
            child: spawn(&files, smtp, http, allowed),
            smtp,
            http,
            certificate: std::fs::read(&cert).expect("the certificate"),
            _files: files,
        };
        for _ in 0..100 {
            if TcpStream::connect(("127.0.0.1", smtp)).await.is_ok()
                && mailpit.get("/api/v1/messages").await.is_some()
            {
                return mailpit;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("Mailpit did not start");
    }

    /// Settings that reach this Mailpit with `password`.
    pub fn settings(&self, password: &str) -> SmtpSettings {
        SmtpSettings {
            host: "localhost".to_string(),
            port: self.smtp,
            username: USER.to_string(),
            password: password.to_string(),
            from: address("tetsu@sandbox.example.com"),
            limit: Duration::from_secs(5),
        }
    }

    /// An adapter that reaches this Mailpit with the right login, trusting its certificate.
    pub fn adapter(&self) -> Smtp {
        Smtp::trusting(&self.settings(PASSWORD), &self.certificate).expect("an adapter")
    }

    /// Every message Mailpit holds, newest first, as its API summarizes them.
    pub async fn messages(&self) -> Vec<Value> {
        let body = self.get("/api/v1/messages").await.expect("Mailpit answers");
        body["messages"].as_array().cloned().unwrap_or_default()
    }

    /// One message in full, by its Mailpit ID.
    pub async fn message(&self, id: &str) -> Value {
        self.get(&format!("/api/v1/message/{id}"))
            .await
            .expect("Mailpit answers")
    }

    async fn get(&self, path: &str) -> Option<Value> {
        let mut stream = TcpStream::connect(("127.0.0.1", self.http)).await.ok()?;
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.ok()?;
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.ok()?;
        let text = String::from_utf8(response).ok()?;
        let (head, body) = text.split_once("\r\n\r\n")?;
        if !head.starts_with("HTTP/1.1 200") {
            return None;
        }
        let body = if head
            .to_ascii_lowercase()
            .contains("transfer-encoding: chunked")
        {
            unchunk(body)
        } else {
            body.to_string()
        };
        serde_json::from_str(&body).ok()
    }
}

impl Drop for Mailpit {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A key and a certificate for `localhost`, made for this test and never kept: an end entity, as rustls accepts.
fn certificate(files: &Path) -> (PathBuf, PathBuf) {
    let (key, cert) = (files.join("key.pem"), files.join("cert.pem"));
    let made = Command::new("openssl")
        .args([
            "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
        ])
        .args([
            "-subj",
            "/CN=localhost",
            "-addext",
            "subjectAltName=DNS:localhost",
        ])
        .args(["-addext", "basicConstraints=critical,CA:FALSE"])
        .arg("-keyout")
        .arg(&key)
        .arg("-out")
        .arg(&cert)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("openssl runs");
    assert!(made.success(), "openssl made a certificate");
    (key, cert)
}

/// Mailpit on `smtp` and `http`, requiring implicit TLS with the key, certificate, and login in `files`.
fn spawn(files: &Path, smtp: u16, http: u16, allowed: Option<&str>) -> Child {
    let mut command = Command::new(binary());
    if let Some(allowed) = allowed {
        command.arg("--smtp-allowed-recipients").arg(allowed);
    }
    command
        .arg("--smtp")
        .arg(format!("127.0.0.1:{smtp}"))
        .arg("--listen")
        .arg(format!("127.0.0.1:{http}"))
        .arg("--smtp-tls-cert")
        .arg(files.join("cert.pem"))
        .arg("--smtp-tls-key")
        .arg(files.join("key.pem"))
        .arg("--smtp-require-tls")
        .arg("--smtp-auth-file")
        .arg(files.join("auth"))
        .arg("--database")
        .arg(files.join("mailpit.db"))
        .arg("--disable-version-check")
        .arg("--quiet")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Mailpit starts; run scripts/install-tools.sh")
}

/// A port nothing listens on: one the system just handed out and took back.
pub fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("a free port")
        .local_addr()
        .expect("its address")
        .port()
}

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.tools/bin/mailpit")
}

fn unchunk(body: &str) -> String {
    let mut out = String::new();
    let mut rest = body;
    while let Some((size, after)) = rest.split_once("\r\n") {
        let size = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
        if size == 0 {
            break;
        }
        out.push_str(&after[..size]);
        rest = &after[size + 2..];
    }
    out
}
