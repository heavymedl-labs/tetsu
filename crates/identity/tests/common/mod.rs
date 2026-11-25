//! What Identity's integration tests share: a migrated database, Identity's use cases over it, and one admin.

use tetsu_identity::{
    AccountId, Accounts, Actor, AdminStore, Identity, PersonName, SessionToken, postgres_store,
};
use tetsu_postgres::connect;
use tetsu_postgres::pool;
use tetsu_postgres::test_support::{TestDatabase, migrated_database};

/// Who runs the admin command in these tests.
pub fn actor() -> Actor {
    Actor::parse("kurt@workstation").expect("an actor")
}

pub fn name(name: &str) -> PersonName {
    PersonName::parse(name).expect("a name")
}

/// A fresh, migrated database.
pub async fn ready() -> TestDatabase {
    migrated_database(&[&Identity])
        .await
        .expect("a migrated test database")
}

/// A migrated database, with Identity's use cases over it as the application role, as `tetsu serve` logs in.
pub async fn migrated() -> (TestDatabase, Accounts<impl AdminStore>) {
    let database = ready().await;
    let admins = postgres_store(pool(&database.as_app()).expect("a pool"));
    (database, admins)
}

/// [`migrated`], holding one admin, Kurt Medley.
pub async fn with_kurt() -> (TestDatabase, Accounts<impl AdminStore>, AccountId) {
    let (database, admins) = migrated().await;
    let kurt = admins
        .create(&name("Kurt Medley"), None, &actor())
        .await
        .expect("creates");
    (database, admins, kurt)
}

/// Starts an admin's session as a passkey login would, as the test server's database administrator, and returns its
/// token: only the login function may start one, so Identity's role cannot call `start_session` (ADR-0048).
pub async fn logged_in(database: &TestDatabase, admin: AccountId) -> SessionToken {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("random bytes");
    let text = bytes.iter().fold(String::new(), |mut text, byte| {
        let _ = std::fmt::Write::write_fmt(&mut text, format_args!("{byte:02x}"));
        text
    });
    connect(database)
        .await
        .expect("connects")
        .execute(
            "SELECT identity.start_session(sha256(decode($1, 'hex')), 'admin', $2, true)",
            &[&text, &admin.get()],
        )
        .await
        .expect("starts");
    SessionToken::parse(&text).expect("a token")
}
