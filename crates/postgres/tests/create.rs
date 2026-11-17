//! Creating the database a developer's settings name, as `tetsu setup` does, on the test server.

use tetsu_postgres::test_support::fresh_database;
use tetsu_postgres::{Settings, connect, create_database};

/// The server's own `postgres` database stands in for the one the settings name, so it can be created or dropped.
#[test]
fn settings_reach_another_database_on_the_same_server() {
    for (url, expected) in [
        (
            "postgres://postgres@localhost:54329/tetsu_local",
            "postgres://postgres@localhost:54329/postgres",
        ),
        (
            "postgresql://a:b@db.example.com/tetsu?sslmode=require&x=1",
            "postgresql://a:b@db.example.com/postgres?sslmode=require&x=1",
        ),
    ] {
        let found = Settings::new(url).on_database("postgres").expect("a URL");
        assert_eq!(found.url(), expected, "{url}");
    }
}

/// Only a URL with a database in its path can name another one; anything else is refused, never guessed at.
#[test]
fn settings_without_a_database_in_a_url_reach_no_other() {
    for url in [
        "host=localhost dbname=tetsu",
        "postgres://localhost:54329",
        "postgres://localhost:54329/",
        "mysql://localhost/tetsu",
    ] {
        assert!(Settings::new(url).on_database("postgres").is_err(), "{url}");
    }
}

/// It creates the database once, and finds it there after, so it is safe to run any time.
#[tokio::test]
async fn the_database_is_created_once_and_found_after() {
    let database = fresh_database().await.expect("a test database");
    assert!(
        !create_database(&database).await.expect("checks"),
        "already there"
    );
    let server = database.on_database("postgres").expect("the server");
    connect(&server)
        .await
        .expect("connects")
        .batch_execute(&format!(
            "DROP DATABASE \"{}\" WITH (FORCE)",
            database.name()
        ))
        .await
        .expect("drops");
    assert!(
        create_database(&database).await.expect("creates"),
        "created"
    );
    connect(&database)
        .await
        .expect("connects to the new database");
    assert!(
        !create_database(&database).await.expect("checks"),
        "found after"
    );
}
