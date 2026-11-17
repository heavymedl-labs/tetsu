//! Rate limits by network (ADR-0072), against Postgres logged in as the application role. Time is moved by moving
//! a limit's row back, as the session tests move a session's.

use std::net::IpAddr;
use std::time::Duration;

use tetsu_postgres::connect;
use tetsu_postgres::test_support::TestDatabase;

use crate::adapters::postgres::{PostgresOperators, hex};
use crate::join_tests::{as_admin, failure, number, with_link};
use crate::{Error, Operators, RateLimit};
use tetsu_kernel::Requester;

fn network(address: &str) -> Requester {
    Requester::of(address.parse::<IpAddr>().expect("an address"))
}

async fn limited() -> (Operators<PostgresOperators>, TestDatabase) {
    let (operators, database, _, _) = with_link().await;
    (operators, database)
}

/// How long `limit` makes `from` wait now, or nothing when it is admitted.
async fn wait(
    operators: &Operators<PostgresOperators>,
    limit: RateLimit,
    from: &Requester,
) -> Option<Duration> {
    match operators.admit(limit, from).await {
        Ok(()) => None,
        Err(Error::Limited(wait)) => Some(wait),
        Err(other) => panic!("{other}"),
    }
}

/// Moves every limit's clock on by `seconds`.
async fn later(database: &TestDatabase, seconds: u64) {
    as_admin(
        database,
        &format!("UPDATE identity.limits SET due = due - interval '{seconds} seconds'"),
    )
    .await;
}

/// Thirty login begins fit in fifteen minutes, one every thirty seconds after; another network, or another limit,
/// has its own room.
#[tokio::test]
async fn a_network_begins_thirty_logins_then_one_each_thirty_seconds() {
    let (operators, database) = limited().await;
    let (one, other) = (network("192.0.2.1"), network("192.0.2.2"));
    for _ in 0..30 {
        assert_eq!(wait(&operators, RateLimit::LoginBegin, &one).await, None);
    }
    let refused = wait(&operators, RateLimit::LoginBegin, &one).await;
    assert_eq!(refused, Some(Duration::from_secs(30)));
    assert_eq!(wait(&operators, RateLimit::LoginBegin, &other).await, None);
    assert_eq!(wait(&operators, RateLimit::Join, &one).await, None);
    later(&database, 30).await;
    assert_eq!(wait(&operators, RateLimit::LoginBegin, &one).await, None);
    assert!(
        wait(&operators, RateLimit::LoginBegin, &one)
            .await
            .is_some()
    );
}

/// Ten join steps fit in an hour, one every six minutes after, and an IPv6 network is its /64.
#[tokio::test]
async fn a_network_takes_ten_join_steps_an_hour() {
    let (operators, _database) = limited().await;
    for n in 0..10 {
        let from = network(&format!("2001:db8:1:2::{n}"));
        assert_eq!(wait(&operators, RateLimit::Join, &from).await, None);
    }
    let from = network("2001:db8:1:2:ffff::1");
    assert_eq!(
        wait(&operators, RateLimit::Join, &from).await,
        Some(Duration::from_secs(360))
    );
}

/// Checking for room never counts against failures; ten failures do, and each further one, once let through,
/// doubles the wait, up to an hour.
#[tokio::test]
async fn failures_meet_their_limit_then_wait_twice_as_long_each_time() {
    let (operators, database) = limited().await;
    let from = network("192.0.2.1");
    for _ in 0..20 {
        assert_eq!(wait(&operators, RateLimit::LoginFailure, &from).await, None);
    }
    for _ in 0..9 {
        operators.failed(&from).await.expect("counts");
    }
    assert_eq!(wait(&operators, RateLimit::LoginFailure, &from).await, None);
    let mut waits = Vec::new();
    for _ in 0..8 {
        operators.failed(&from).await.expect("counts");
        let waited = wait(&operators, RateLimit::LoginFailure, &from)
            .await
            .expect("a wait");
        waits.push(waited.as_secs());
        later(&database, waited.as_secs()).await;
        assert_eq!(wait(&operators, RateLimit::LoginFailure, &from).await, None);
    }
    assert_eq!(waits, [60, 120, 240, 480, 960, 1920, 3600, 3600]);
    let other = network("192.0.2.2");
    assert_eq!(
        wait(&operators, RateLimit::LoginFailure, &other).await,
        None
    );
}

/// Once all a network's room is free again, its strikes are forgotten, and failing starts over.
#[tokio::test]
async fn strikes_are_forgotten_once_the_room_is_free() {
    let (operators, database) = limited().await;
    let from = network("192.0.2.1");
    for _ in 0..10 {
        operators.failed(&from).await.expect("counts");
    }
    later(&database, 7200).await;
    operators.failed(&from).await.expect("counts");
    let strikes = "SELECT strikes::bigint FROM identity.limits WHERE kind = 'login_failure'";
    assert_eq!(number(&database, strikes).await, 0);
    assert_eq!(wait(&operators, RateLimit::LoginFailure, &from).await, None);
}

/// A limit's row keys on its network hashed with the database's own secret, never the address or its plain hash,
/// and rows a day past their use are swept, a hundred at a time.
#[tokio::test]
async fn limits_hold_no_address_and_forget_old_networks() {
    let (operators, database) = limited().await;
    let from = network("192.0.2.1");
    operators
        .admit(RateLimit::LoginBegin, &from)
        .await
        .expect("admits");
    let bytes = hex(from.as_bytes());
    let keyed = format!(
        "SELECT count(*) FROM identity.limits AS l, identity.limit_secret AS s
         WHERE l.network_hash = sha256(s.secret || '\\x{bytes}'::bytea)
         AND l.network_hash <> sha256('\\x{bytes}'::bytea)"
    );
    assert_eq!(number(&database, &keyed).await, 1);
    as_admin(
        &database,
        "INSERT INTO identity.limits (kind, network_hash, due)
         SELECT 'join', sha256(int8send(n)), now() - interval '2 days'
         FROM generate_series(1, 150) AS n",
    )
    .await;
    operators
        .admit(RateLimit::LoginBegin, &from)
        .await
        .expect("admits");
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.limits").await,
        51
    );
}

/// Only Identity's two functions reach the limits, and each refuses a limit it does not keep, or a network that is
/// not one.
#[tokio::test]
async fn the_limits_refuse_what_they_do_not_keep() {
    let (_operators, database) = limited().await;
    let mut app = connect(&database.as_app()).await.expect("connects");
    let network = "'\\x00000000000000000000ffffc0000201'::bytea";
    for (call, said) in [
        (
            format!("identity.admit('nope', {network})"),
            "no limit is called nope",
        ),
        (
            format!("identity.fail('login_begin', {network})"),
            "no limit of failures is called login_begin",
        ),
        (
            "identity.admit('join', '\\x01'::bytea)".to_string(),
            "a network is 16 bytes",
        ),
    ] {
        let error = as_identity(&mut app, &format!("SELECT {call}")).await;
        assert!(format!("{error:?}").contains(said), "{call}: {error:?}");
    }
    for denied in [
        "SELECT * FROM identity.limits",
        "SELECT * FROM identity.limit_secret",
        "SELECT identity.limit_rule('join')",
    ] {
        let error = as_identity(&mut app, denied).await;
        assert!(
            format!("{error:?}").contains("permission denied"),
            "{denied}: {error:?}"
        );
    }
}

/// The error `sql` fails with, run as Identity's role in a transaction of its own.
async fn as_identity(app: &mut tokio_postgres::Client, sql: &str) -> tokio_postgres::Error {
    let transaction = app.transaction().await.expect("begins");
    transaction
        .batch_execute("SET LOCAL ROLE tetsu_identity")
        .await
        .expect("acts as Identity");
    transaction.simple_query(sql).await.expect_err("refused")
}

/// Admitting and counting fail as storage when the database refuses their functions.
#[tokio::test]
async fn limits_fail_as_storage_when_refused() {
    let (operators, database) = limited().await;
    let from = network("192.0.2.1");
    as_admin(
        &database,
        "REVOKE EXECUTE ON FUNCTION identity.admit(text, bytea), identity.fail(text, bytea)
         FROM tetsu_identity",
    )
    .await;
    assert_eq!(
        failure(operators.admit(RateLimit::LoginBegin, &from).await),
        "storage failed: permission denied for function admit"
    );
    assert_eq!(
        failure(operators.failed(&from).await),
        "storage failed: permission denied for function fail"
    );
}
