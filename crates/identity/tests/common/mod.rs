//! What Identity's integration tests share: a migrated database, Identity's use cases over it, and one operator.

use tetsu_identity::{
    Actor, Identity, OperatorId, OperatorName, OperatorStore, Operators, SessionToken,
    postgres_store,
};
use tetsu_postgres::connect;
use tetsu_postgres::pool;
use tetsu_postgres::test_support::{TestDatabase, migrated_database};

/// Who runs the operator command in these tests.
pub fn actor() -> Actor {
    Actor::parse("kurt@workstation").expect("an actor")
}

pub fn name(name: &str) -> OperatorName {
    OperatorName::parse(name).expect("a name")
}

/// A fresh, migrated database.
pub async fn ready() -> TestDatabase {
    migrated_database(&[&Identity])
        .await
        .expect("a migrated test database")
}

/// A migrated database, with Identity's use cases over it as the application role, as `tetsu serve` signs in.
pub async fn migrated() -> (TestDatabase, Operators<impl OperatorStore>) {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    (database, operators)
}

/// [`migrated`], holding one operator, Kurt Medley.
pub async fn with_kurt() -> (TestDatabase, Operators<impl OperatorStore>, OperatorId) {
    let (database, operators) = migrated().await;
    let kurt = operators
        .create(&name("Kurt Medley"), &actor())
        .await
        .expect("creates");
    (database, operators, kurt)
}

/// Starts an operator's session as a passkey sign-in would, as the test server's administrator, and returns its
/// token: only the sign-in function may start one, so Identity's role cannot call `start_session` (ADR-0048).
pub async fn signed_in(database: &TestDatabase, operator: OperatorId) -> SessionToken {
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
            "SELECT identity.start_session(sha256(decode($1, 'hex')), 'operator', $2)",
            &[&text, &operator.get()],
        )
        .await
        .expect("starts");
    SessionToken::parse(&text).expect("a token")
}
