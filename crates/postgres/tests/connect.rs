//! Connection rules from ADR-0011 and ADR-0013.

use std::time::{Duration, Instant};

use tetsu_postgres::test_support::fresh_database;
use tetsu_postgres::{Settings, Tls, pool};

/// Real timeouts are seconds long, so tests that wait them out use these instead. The defaults are pinned in
/// a unit test beside the code.
const SHORT: Duration = Duration::from_millis(300);

#[tokio::test]
async fn tls_is_the_default_and_refuses_a_server_without_it() {
    let plain = fresh_database().await.expect("a test database");
    let required = Settings::new(plain.url());
    assert_eq!(required.tls(), Tls::Required);
    let error = tetsu_postgres::connect(&required)
        .await
        .expect_err("must not fall back to plain text");
    assert!(error.to_string().contains("TLS"), "{error}");
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
    let plain = fresh_database().await.expect("a test database");
    let required = Settings::new(plain.url());
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
