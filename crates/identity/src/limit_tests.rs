//! Rate limits by network (ADR-0072), against Postgres logged in as the application role. Time is moved by moving
//! a limit's row back, as the session tests move a session's.

use std::net::IpAddr;
use std::time::Duration;

use tetsu_postgres::connect;
use tetsu_postgres::test_support::TestDatabase;

use crate::adapters::postgres::{PostgresAdmins, hex};
use crate::join_tests::{as_db_admin, failure, number, with_link};
use crate::{Accounts, Error, RateLimit};
use tetsu_kernel::Requester;

fn network(address: &str) -> Requester {
    Requester::of(address.parse::<IpAddr>().expect("an address"))
}

async fn limited() -> (Accounts<PostgresAdmins>, TestDatabase) {
    let (admins, database, _, _) = with_link().await;
    (admins, database)
}

/// How long `limit` makes `from` wait now, or nothing when it is admitted.
async fn wait(
    admins: &Accounts<PostgresAdmins>,
    limit: RateLimit,
    from: &Requester,
) -> Option<Duration> {
    match admins.admit(limit, from).await {
        Ok(()) => None,
        Err(Error::Limited(wait)) => Some(wait),
        Err(other) => panic!("{other}"),
    }
}

/// Moves every limit's clock on by `seconds`.
async fn later(database: &TestDatabase, seconds: u64) {
    as_db_admin(
        database,
        &format!("UPDATE identity.limits SET due = due - interval '{seconds} seconds'"),
    )
    .await;
}

/// Thirty login begins fit in fifteen minutes, one every thirty seconds after; another network, or another limit,
/// has its own room.
#[tokio::test]
async fn a_network_begins_thirty_logins_then_one_each_thirty_seconds() {
    let (admins, database) = limited().await;
    let (one, other) = (network("192.0.2.1"), network("192.0.2.2"));
    for _ in 0..30 {
        assert_eq!(wait(&admins, RateLimit::LoginBegin, &one).await, None);
    }
    let refused = wait(&admins, RateLimit::LoginBegin, &one).await;
    assert_eq!(refused, Some(Duration::from_secs(30)));
    assert_eq!(wait(&admins, RateLimit::LoginBegin, &other).await, None);
    assert_eq!(wait(&admins, RateLimit::Join, &one).await, None);
    later(&database, 30).await;
    assert_eq!(wait(&admins, RateLimit::LoginBegin, &one).await, None);
    assert!(wait(&admins, RateLimit::LoginBegin, &one).await.is_some());
}

/// Ten join steps fit in an hour, one every six minutes after, and an IPv6 network is its /64.
#[tokio::test]
async fn a_network_takes_ten_join_steps_an_hour() {
    let (admins, _database) = limited().await;
    for n in 0..10 {
        let from = network(&format!("2001:db8:1:2::{n}"));
        assert_eq!(wait(&admins, RateLimit::Join, &from).await, None);
    }
    let from = network("2001:db8:1:2:ffff::1");
    assert_eq!(
        wait(&admins, RateLimit::Join, &from).await,
        Some(Duration::from_secs(360))
    );
}

/// Checking for room never counts against failures; ten failures do, and each further one, once let through,
/// doubles the wait, up to an hour.
#[tokio::test]
async fn failures_meet_their_limit_then_wait_twice_as_long_each_time() {
    let (admins, database) = limited().await;
    let from = network("192.0.2.1");
    for _ in 0..20 {
        assert_eq!(wait(&admins, RateLimit::LoginFailure, &from).await, None);
    }
    for _ in 0..9 {
        admins.failed(&from).await.expect("counts");
    }
    assert_eq!(wait(&admins, RateLimit::LoginFailure, &from).await, None);
    let mut waits = Vec::new();
    for _ in 0..8 {
        admins.failed(&from).await.expect("counts");
        let waited = wait(&admins, RateLimit::LoginFailure, &from)
            .await
            .expect("a wait");
        waits.push(waited.as_secs());
        later(&database, waited.as_secs()).await;
        assert_eq!(wait(&admins, RateLimit::LoginFailure, &from).await, None);
    }
    assert_eq!(waits, [60, 120, 240, 480, 960, 1920, 3600, 3600]);
    let other = network("192.0.2.2");
    assert_eq!(wait(&admins, RateLimit::LoginFailure, &other).await, None);
}

/// Once all a network's room is free again, its strikes are forgotten, and failing starts over.
#[tokio::test]
async fn strikes_are_forgotten_once_the_room_is_free() {
    let (admins, database) = limited().await;
    let from = network("192.0.2.1");
    for _ in 0..10 {
        admins.failed(&from).await.expect("counts");
    }
    later(&database, 7200).await;
    admins.failed(&from).await.expect("counts");
    let strikes = "SELECT strikes::bigint FROM identity.limits WHERE kind = 'login_failure'";
    assert_eq!(number(&database, strikes).await, 0);
    assert_eq!(wait(&admins, RateLimit::LoginFailure, &from).await, None);
}

/// A limit's row keys on its network hashed with the database's own secret, never the address or its plain hash,
/// and rows a day past their use are swept, a hundred at a time.
#[tokio::test]
async fn limits_hold_no_address_and_forget_old_networks() {
    let (admins, database) = limited().await;
    let from = network("192.0.2.1");
    admins
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
    as_db_admin(
        &database,
        "INSERT INTO identity.limits (kind, network_hash, due)
         SELECT 'join', sha256(int8send(n)), now() - interval '2 days'
         FROM generate_series(1, 150) AS n",
    )
    .await;
    admins
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
    let (_admins, database) = limited().await;
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
    let (admins, database) = limited().await;
    let from = network("192.0.2.1");
    as_db_admin(
        &database,
        "REVOKE EXECUTE ON FUNCTION identity.admit(text, bytea), identity.fail(text, bytea)
         FROM tetsu_identity",
    )
    .await;
    assert_eq!(
        failure(admins.admit(RateLimit::LoginBegin, &from).await),
        "storage failed: permission denied for function admit"
    );
    assert_eq!(
        failure(admins.failed(&from).await),
        "storage failed: permission denied for function fail"
    );
}

/// Ten refusals are recorded in fifteen minutes, one every ninety seconds after, and a network past that says so
/// once in fifteen minutes (ADR-0081).
#[tokio::test]
async fn a_network_has_ten_refusals_recorded_in_fifteen_minutes() {
    let (admins, database) = limited().await;
    let from = network("192.0.2.1");
    for _ in 0..10 {
        assert_eq!(wait(&admins, RateLimit::Refusal, &from).await, None);
    }
    assert_eq!(
        wait(&admins, RateLimit::Refusal, &from).await,
        Some(Duration::from_secs(90))
    );
    assert_eq!(wait(&admins, RateLimit::RefusalFlood, &from).await, None);
    assert_eq!(
        wait(&admins, RateLimit::RefusalFlood, &from).await,
        Some(Duration::from_mins(15))
    );
    later(&database, 90).await;
    assert_eq!(wait(&admins, RateLimit::Refusal, &from).await, None);
    later(&database, 810).await;
    assert_eq!(wait(&admins, RateLimit::RefusalFlood, &from).await, None);
}

/// Each realm's logins have their own room on a network, so staff using up theirs leave admins' alone, and the
/// reverse; failures are counted apart too.
#[tokio::test]
async fn staff_and_admins_have_their_own_room_on_a_network() {
    let (admins, database) = limited().await;
    let staff = Accounts::new(crate::adapters::postgres::PostgresStaff::new(
        crate::join_tests::app_pool(&database),
    ));
    let one = network("192.0.2.1");
    for _ in 0..30 {
        assert!(staff.admit(RateLimit::LoginBegin, &one).await.is_ok());
    }
    assert!(matches!(
        staff.admit(RateLimit::LoginBegin, &one).await,
        Err(Error::Limited(_))
    ));
    assert_eq!(wait(&admins, RateLimit::LoginBegin, &one).await, None);
    staff.failed(&one).await.expect("counted");
    assert_eq!(
        number(
            &database,
            "SELECT count(*) FROM identity.limits WHERE kind = 'staff_login_failure'"
        )
        .await,
        1
    );
    assert_eq!(
        number(
            &database,
            "SELECT count(*) FROM identity.limits WHERE kind = 'login_failure'"
        )
        .await,
        0
    );
}
