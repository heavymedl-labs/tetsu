//! Connection rules from ADR-0011 and ADR-0013.

use std::time::{Duration, Instant};

use tetsu_postgres::test_support::fresh_database;
use tetsu_postgres::{Settings, Tls, pool};

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
    let pool = pool(&settings).expect("pool");
    let mut held = Vec::new();
    for _ in 0..pool.status().max_size {
        held.push(pool.get().await.expect("connects"));
    }
    let started = Instant::now();
    assert!(pool.get().await.is_err());
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[tokio::test]
async fn a_server_that_never_answers_times_out() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("binds");
    let port = listener.local_addr().expect("address").port();
    let settings =
        Settings::from_values(&format!("postgres://x@127.0.0.1:{port}/x"), Some("disable"));
    let started = Instant::now();
    assert!(pool(&settings).expect("pool").get().await.is_err());
    assert!(started.elapsed() < Duration::from_secs(6));
    drop(listener);
}

#[tokio::test]
async fn a_direct_connection_to_a_silent_server_times_out() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("binds");
    let port = listener.local_addr().expect("address").port();
    let settings =
        Settings::from_values(&format!("postgres://x@127.0.0.1:{port}/x"), Some("disable"));
    let started = Instant::now();
    assert!(tetsu_postgres::connect(&settings).await.is_err());
    assert!(started.elapsed() < Duration::from_secs(8));
    drop(listener);
}
