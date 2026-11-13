//! Connection rules from ADR-0011 and ADR-0013.

use std::time::{Duration, Instant};

use tetsu_postgres::test_support::{Scratch, fresh_database};
use tetsu_postgres::{Settings, Tls, pool};

/// Real timeouts are seconds long, so tests that wait them out use these instead. The defaults are pinned in
/// a unit test beside the code.
const SHORT: Duration = Duration::from_millis(300);

/// A server that answers Postgres's TLS request with "no", as one without TLS does. The test Postgres offers TLS,
/// so it cannot play this part.
async fn server_without_tls() -> std::io::Result<String> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            tokio::spawn(refuse_tls(socket));
        }
    });
    Ok(format!("postgres://x@127.0.0.1:{port}/x"))
}

async fn refuse_tls(socket: tokio::net::TcpStream) -> std::io::Result<()> {
    let (mut request, mut read) = ([0; 8], 0);
    while read < request.len() {
        socket.readable().await?;
        match socket.try_read(&mut request[read..]) {
            Ok(0) => return Ok(()),
            Ok(n) => read += n,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error),
        }
    }
    socket.writable().await?;
    socket.try_write(b"N").map(|_| ())
}

/// The error and every cause beneath it, where the TLS library says why it refused.
fn chain(error: &tetsu_postgres::Error) -> String {
    std::iter::successors(Some(&**error as &dyn std::error::Error), |e| e.source())
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ")
}

#[tokio::test]
async fn tls_is_the_default_and_refuses_a_server_without_it() {
    let required = Settings::new(&server_without_tls().await.expect("a server"));
    assert_eq!(required.tls(), Tls::Required);
    let error = tetsu_postgres::connect(&required)
        .await
        .expect_err("must not fall back to plain text");
    assert!(error.to_string().contains("TLS"), "{error}");
}

/// The test Postgres presents its image's self-signed certificate for localhost, so only the authority is wrong.
/// A connector that skipped verification would connect.
#[tokio::test]
async fn tls_refuses_a_certificate_no_trusted_authority_signed() {
    let plain = fresh_database().await.expect("a test database");
    let required = Settings::new(plain.url());
    let direct = tetsu_postgres::connect(&required)
        .await
        .expect_err("must not trust the certificate");
    assert!(
        chain(&direct).contains("UnknownIssuer"),
        "{}",
        chain(&direct)
    );
    let pooled = pool(&required)
        .expect("pool")
        .get()
        .await
        .expect_err("must not trust the certificate");
    let pooled = chain(&pooled.into());
    assert!(pooled.contains("UnknownIssuer"), "{pooled}");
}

/// The test Postgres's own certificate, read over SQL when the test runs, so no certificate is committed. It is
/// self-signed for localhost, so trusting it makes it the authority.
async fn server_certificate(
    database: &tetsu_postgres::test_support::TestDatabase,
) -> Result<Scratch, tetsu_postgres::Error> {
    let client = tetsu_postgres::connect(database).await?;
    let pem: String = client
        .query_one("SELECT pg_read_file(current_setting('ssl_cert_file'))", &[])
        .await?
        .try_get(0)?;
    Ok(Scratch::file(&format!("{}.pem", database.name()), &pem)?)
}

/// What rustls says when a trusted certificate names another host.
const NOT_LOCALHOST: &str = r#"certificate not valid for name "127.0.0.1""#;

#[tokio::test]
async fn a_ca_file_is_trusted_but_only_for_the_hostname_its_certificate_names() {
    let database = fresh_database().await.expect("a test database");
    assert!(
        database.url().contains("@localhost:"),
        "the test needs a localhost URL"
    );
    let ca = server_certificate(&database)
        .await
        .expect("the server's certificate");

    let trusted = Settings::new(database.url())
        .with_ca_file(&ca)
        .expect("the file holds a certificate");
    let client = tetsu_postgres::connect(&trusted)
        .await
        .expect("trusts the authority in the file");
    client
        .simple_query("SELECT 1")
        .await
        .expect("a query over TLS");
    let pooled = pool(&trusted)
        .expect("pool")
        .get()
        .await
        .expect("the pool trusts it too");
    drop(pooled);

    let elsewhere = Settings::new(&database.url().replacen("@localhost:", "@127.0.0.1:", 1))
        .with_ca_file(&ca)
        .expect("the file holds a certificate");
    let direct = tetsu_postgres::connect(&elsewhere)
        .await
        .expect_err("the certificate names localhost, not 127.0.0.1");
    assert!(chain(&direct).contains(NOT_LOCALHOST), "{}", chain(&direct));
    let pooled = pool(&elsewhere)
        .expect("pool")
        .get()
        .await
        .expect_err("the pool checks the name too");
    let pooled = chain(&pooled.into());
    assert!(pooled.contains(NOT_LOCALHOST), "{pooled}");
    std::fs::remove_file(ca).expect("cleanup");
}

/// A DNS name is checked by a different path than an IP address, and providers use names. `hostaddr` sends the
/// connection to the test Postgres while TLS checks the certificate against the name in `host`.
#[tokio::test]
async fn a_ca_file_still_refuses_a_certificate_for_another_dns_name() {
    let database = fresh_database().await.expect("a test database");
    assert!(
        database.url().contains("@localhost:"),
        "the test needs a localhost URL"
    );
    let ca = server_certificate(&database)
        .await
        .expect("the server's certificate");
    let wrong_name = format!(
        "{}?hostaddr=127.0.0.1",
        database.url().replacen("@localhost:", "@wrong.example:", 1)
    );
    let wrong_name = Settings::new(&wrong_name)
        .with_ca_file(&ca)
        .expect("the file holds a certificate");
    let refused = tetsu_postgres::connect(&wrong_name)
        .await
        .expect_err("the certificate names localhost, not wrong.example");
    assert!(
        chain(&refused).contains(r#"certificate not valid for name "wrong.example""#),
        "{}",
        chain(&refused)
    );
    std::fs::remove_file(ca).expect("cleanup");
}

#[test]
fn only_an_explicit_setting_turns_tls_off() {
    assert_eq!(
        Settings::from_values("postgres://x", None).tls(),
        Tls::Required
    );
    assert_eq!(
        Settings::from_values("postgres://x", Some("require")).tls(),
        Tls::Required
    );
    assert_eq!(
        Settings::from_values("postgres://x", Some("disable")).tls(),
        Tls::Disabled
    );
}

#[test]
fn settings_never_print_the_url() {
    let settings = Settings::new("postgres://user:secret@host/db");
    let shown = format!("{settings:?}");
    assert!(!shown.contains("secret"));
    assert!(shown.contains("Required"), "{shown}");
}

#[tokio::test]
async fn a_pool_requiring_tls_refuses_a_server_without_it() {
    let required = Settings::new(&server_without_tls().await.expect("a server"));
    assert!(pool(&required).expect("pool").get().await.is_err());
}

#[tokio::test]
async fn every_connection_has_a_statement_timeout() {
    let settings = fresh_database().await.expect("a test database");
    let client = pool(&settings)
        .expect("pool")
        .get()
        .await
        .expect("connects");
    let row = client
        .query_one("SHOW statement_timeout", &[])
        .await
        .expect("query");
    assert_eq!(row.try_get::<_, String>(0).expect("maps"), "5s");
}

#[tokio::test]
async fn the_direct_connection_has_no_statement_timeout_for_migrations() {
    let settings = fresh_database().await.expect("a test database");
    let client = tetsu_postgres::connect(&settings).await.expect("connects");
    let row = client
        .query_one("SHOW statement_timeout", &[])
        .await
        .expect("query");
    assert_eq!(row.try_get::<_, String>(0).expect("maps"), "0");
}

#[tokio::test]
async fn a_full_pool_fails_fast() {
    let settings = fresh_database().await.expect("a test database");
    let short = (*settings).clone().with_timeouts(SHORT, SHORT);
    let pool = pool(&short).expect("pool");
    let mut held = Vec::new();
    for _ in 0..pool.status().max_size {
        held.push(pool.get().await.expect("connects"));
    }
    let started = Instant::now();
    assert!(pool.get().await.is_err());
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn a_server_that_never_answers_times_out() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("binds");
    let port = listener.local_addr().expect("address").port();
    let settings =
        Settings::from_values(&format!("postgres://x@127.0.0.1:{port}/x"), Some("disable"))
            .with_timeouts(SHORT, SHORT);
    let started = Instant::now();
    assert!(pool(&settings).expect("pool").get().await.is_err());
    assert!(started.elapsed() < Duration::from_secs(2));
    drop(listener);
}

#[tokio::test]
async fn a_direct_connection_to_a_silent_server_times_out() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("binds");
    let port = listener.local_addr().expect("address").port();
    let settings =
        Settings::from_values(&format!("postgres://x@127.0.0.1:{port}/x"), Some("disable"))
            .with_timeouts(SHORT, SHORT);
    let started = Instant::now();
    assert!(tetsu_postgres::connect(&settings).await.is_err());
    assert!(started.elapsed() < Duration::from_secs(2));
    drop(listener);
}

#[tokio::test]
async fn a_malformed_url_never_echoes_any_part_of_it() {
    for url in [
        "host=localhost port=54329 user=postgres password=my s3cret=x",
        "postgres://localhost/db?password=x&s3cret=1",
        "postgres://u:s3cret@localhost:no-port/db",
    ] {
        let settings = tetsu_postgres::Settings::from_values(url, Some("disable"));
        let errors = [
            tetsu_postgres::connect(&settings)
                .await
                .err()
                .map(|e| e.to_string()),
            tetsu_postgres::pool(&settings).err().map(|e| e.to_string()),
        ];
        for error in errors {
            let error = error.expect("a malformed URL fails");
            assert_eq!(
                error, "the database URL is not a valid connection string",
                "{url}"
            );
        }
    }
}

#[tokio::test]
async fn the_pool_keeps_options_from_the_url_and_adds_its_timeout() {
    let database = fresh_database().await.expect("a test database");
    let url = format!("{}?options=-c%20application_name%3Dkept", database.url());
    let pool = tetsu_postgres::pool(&tetsu_postgres::Settings::from_values(
        &url,
        Some("disable"),
    ))
    .expect("a pool");
    let client = pool.get().await.expect("a connection");
    let name: String = client
        .query_one("SHOW application_name", &[])
        .await
        .expect("shows")
        .try_get(0)
        .expect("a name");
    let timeout: String = client
        .query_one("SHOW statement_timeout", &[])
        .await
        .expect("shows")
        .try_get(0)
        .expect("a timeout");
    assert_eq!((name.as_str(), timeout.as_str()), ("kept", "5s"));
}

#[test]
fn check_refuses_a_url_that_sets_tls_or_breaks_the_pool_options() {
    let check = |url: &str| tetsu_postgres::Settings::from_values(url, None).check();
    for url in [
        "postgres://h/db?sslmode=prefer",
        "postgres://h/db?sslmode=disable&sslmode=prefer",
        "postgres://h/db?sslmode=require",
        "postgres://h/db?ss%6Cmode=disable",
    ] {
        assert_eq!(
            check(url).unwrap_err().to_string(),
            "the database URL must not set sslmode; TLS is set separately",
            "{url}"
        );
    }
    assert_eq!(
        check("postgres://h/db?options=-c%20search_path%3Dx%5C")
            .unwrap_err()
            .to_string(),
        "the database URL's options must not end in a backslash"
    );
    assert!(check("postgres://u:mysslmodepw@h/db?options=-c%20search_path%3Dx").is_ok());
    assert_eq!(
        check("postgres://h:99999/db").unwrap_err().to_string(),
        "the database URL is not a valid connection string"
    );
}

#[tokio::test]
async fn an_at_sign_in_the_password_never_reaches_an_error() {
    let plain = fresh_database().await.expect("a test database");
    let (base, _) = plain.url().rsplit_once('/').expect("a database name");
    let with_password =
        |password: &str| base.replacen("postgres@", &format!("postgres:{password}@"), 1);
    // The driver splits at the first `@`, so the rest of these passwords would reach the server as a database name.
    for url in [
        format!("{}/SECRET@h/db", with_password("x")),
        format!("{}/SECRET@h/db", with_password("x?a")),
    ] {
        let settings = Settings::from_values(&url, Some("disable"));
        let errors = [
            settings.check().err(),
            tetsu_postgres::connect(&settings).await.err(),
            pool(&settings).err(),
        ];
        for error in errors {
            assert_eq!(
                error.expect("refused").to_string(),
                "the database URL holds more than one @; write any @ in the password as %40",
                "{url}"
            );
        }
    }
    // Written as %40, the URL passes, and the server's error names only the database.
    let url = format!("{}/no_such_database", with_password("x%40SECRET"));
    let error = tetsu_postgres::connect(&Settings::from_values(&url, Some("disable")))
        .await
        .expect_err("the database does not exist");
    let chain = chain(&error);
    assert!(chain.contains("no_such_database"), "{chain}");
    assert!(!chain.contains("SECRET"), "{chain}");
}
