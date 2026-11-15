//! Sorting the driver's errors (ADR-0011), against a real server: what is taken, what is worth retrying, and what is
//! not, each keeping what Postgres said.

use std::time::Duration;

use tetsu_postgres::test_support::{TestDatabase, fresh_database};
use tetsu_postgres::{
    Failure, STALE, Settings, clear_if_stale, connect, pool, sort, sort_commit, sort_pool,
};
use tokio_postgres::error::SqlState;

#[cfg(test)]
mod support {
    use super::*;

    /// A connection, a second one to end it with, and the first one's server process id.
    pub async fn client_and_its_killer() -> (
        TestDatabase,
        tokio_postgres::Client,
        tokio_postgres::Client,
        i32,
    ) {
        let database = fresh_database().await.expect("a test database");
        let client = connect(&database).await.expect("connects");
        let admin = connect(&database).await.expect("connects");
        let pid = client
            .query_one("SELECT pg_backend_pid() AS pid", &[])
            .await
            .expect("finds its pid")
            .try_get("pid")
            .expect("maps");
        (database, client, admin, pid)
    }

    pub async fn kill(admin: &tokio_postgres::Client, pid: i32) {
        admin
            .execute("SELECT pg_terminate_backend($1)", &[&pid])
            .await
            .expect("ends the connection");
    }
}

use support::{client_and_its_killer, kill};

#[tokio::test]
async fn a_unique_row_already_held_is_taken_and_names_its_constraint() {
    let database = fresh_database().await.expect("a test database");
    let client = connect(&database).await.expect("connects");
    client
        .batch_execute("CREATE TABLE held (name text CONSTRAINT held_name UNIQUE); INSERT INTO held VALUES ('a')")
        .await
        .expect("makes a table");
    let error = client
        .execute("INSERT INTO held VALUES ('a')", &[])
        .await
        .expect_err("refused");
    assert_eq!(
        sort(&error),
        Failure::Taken {
            constraint: Some("held_name".into()),
            message: "duplicate key value violates unique constraint \"held_name\"".into(),
        }
    );
}

#[tokio::test]
async fn a_unique_row_refused_at_commit_is_still_taken() {
    let database = fresh_database().await.expect("a test database");
    let mut client = connect(&database).await.expect("connects");
    client
        .batch_execute(
            "CREATE TABLE held (name text CONSTRAINT held_name UNIQUE DEFERRABLE INITIALLY DEFERRED);
             INSERT INTO held VALUES ('a')",
        )
        .await
        .expect("makes a table");
    let transaction = client.transaction().await.expect("begins");
    transaction
        .execute("INSERT INTO held VALUES ('a')", &[])
        .await
        .expect("waits for the commit");
    let error = transaction.commit().await.expect_err("refused");
    assert_eq!(
        sort_commit(&error),
        Failure::Taken {
            constraint: Some("held_name".into()),
            message: "commit refused: duplicate key value violates unique constraint \"held_name\""
                .into(),
        }
    );
}

/// A refused commit was rolled back, so it is never sorted as worth retrying, whatever refused it.
#[tokio::test]
async fn any_other_refused_commit_is_permanent() {
    let database = fresh_database().await.expect("a test database");
    let mut client = connect(&database).await.expect("connects");
    client
        .batch_execute(
            "CREATE TABLE parent (id int PRIMARY KEY);
             CREATE TABLE child (parent int REFERENCES parent DEFERRABLE INITIALLY DEFERRED)",
        )
        .await
        .expect("makes tables");
    let transaction = client.transaction().await.expect("begins");
    transaction
        .execute("INSERT INTO child VALUES (1)", &[])
        .await
        .expect("waits for the commit");
    let error = transaction.commit().await.expect_err("refused");
    let Failure::Permanent(why) = sort_commit(&error) else {
        panic!("{error}");
    };
    assert!(why.starts_with("commit refused: insert or update"), "{why}");
}

/// A commit with no answer may have been saved, so retrying it is not safe.
#[tokio::test]
async fn a_commit_with_no_answer_is_permanent() {
    let (_database, mut client, admin, pid) = client_and_its_killer().await;
    let transaction = client.transaction().await.expect("begins");
    kill(&admin, pid).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let error = transaction.commit().await.expect_err("no answer");
    let Failure::Permanent(why) = sort_commit(&error) else {
        panic!("{error}");
    };
    assert!(why.starts_with("commit outcome unknown: "), "{why}");
}

/// The pool replaces a dead connection, so this case is a race in practice: test the sorting directly.
#[tokio::test]
async fn a_connection_the_server_closed_is_temporary() {
    let (_database, client, admin, pid) = client_and_its_killer().await;
    kill(&admin, pid).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let error = client
        .query_one("SELECT 1", &[])
        .await
        .expect_err("the connection is closed");
    assert!(matches!(sort(&error), Failure::Temporary(_)), "{error}");
}

/// A restart or failover ends sessions with a FATAL error that reaches a query still running, not a closed
/// connection, so it must be sorted as temporary too, keeping what the server said.
#[tokio::test]
async fn a_query_the_server_cuts_off_is_temporary() {
    let (_database, client, admin, pid) = client_and_its_killer().await;
    let running = tokio::spawn(async move { client.query_one("SELECT pg_sleep(10)", &[]).await });
    tokio::time::sleep(Duration::from_millis(200)).await;
    kill(&admin, pid).await;
    let error = running
        .await
        .expect("the query task ends")
        .expect_err("the query is cut off");
    assert_eq!(error.code(), Some(&SqlState::ADMIN_SHUTDOWN), "{error}");
    assert_eq!(
        sort(&error),
        Failure::Temporary("terminating connection due to administrator command".into())
    );
}

/// Every other refusal keeps the server's words, not the driver's "db error".
#[tokio::test]
async fn only_a_stale_plan_counts_among_unsupported_features() {
    let database = fresh_database().await.expect("a test database");
    let client = connect(&database).await.expect("connects");
    let error = client
        .query("SELECT count(*) FROM pg_class FOR UPDATE", &[])
        .await
        .expect_err("unsupported");
    assert_eq!(error.code(), Some(&SqlState::FEATURE_NOT_SUPPORTED));
    assert_eq!(
        sort(&error),
        Failure::Permanent("FOR UPDATE is not allowed with aggregate functions".into())
    );
}

/// A cached statement whose table changed is temporary, and clearing it reaches every pooled connection.
#[tokio::test]
async fn a_stale_statement_is_temporary_and_clears_every_connection() {
    let database = fresh_database().await.expect("a test database");
    let admin = connect(&database).await.expect("connects");
    admin
        .batch_execute("CREATE TABLE notes (body text); GRANT SELECT ON notes TO PUBLIC")
        .await
        .expect("makes a table");
    let pool = pool(&database).expect("a pool");
    let (first, second) = (
        pool.get().await.expect("a connection"),
        pool.get().await.expect("another"),
    );
    for connection in [&first, &second] {
        connection
            .prepare_cached("SELECT * FROM notes")
            .await
            .expect("caches the query");
    }
    admin
        .batch_execute("ALTER TABLE notes ALTER COLUMN body TYPE varchar(200)")
        .await
        .expect("alters");
    let statement = first
        .prepare_cached("SELECT * FROM notes")
        .await
        .expect("still cached");
    let error = first.query(&statement, &[]).await.expect_err("stale");
    let Failure::Temporary(why) = sort(&error) else {
        panic!("{error}");
    };
    assert!(why.starts_with(STALE), "{why}");
    clear_if_stale(&pool, "temporary for another reason");
    assert_eq!(
        first.statement_cache.size(),
        1,
        "only a stale failure clears"
    );
    clear_if_stale(&pool, &why);
    assert_eq!(
        (first.statement_cache.size(), second.statement_cache.size()),
        (0, 0)
    );
}

/// A pool that waits too long for a free connection is worth retrying.
#[tokio::test]
async fn a_pool_timeout_is_temporary() {
    let database = fresh_database().await.expect("a test database");
    let settings = Settings::from_values(database.url(), Some("disable"))
        .with_timeouts(Duration::from_secs(5), Duration::from_millis(50));
    let pool = pool(&settings).expect("a pool");
    let mut held = Vec::new();
    while held.len() < pool.status().max_size {
        held.push(pool.get().await.expect("a connection"));
    }
    let error = pool.get().await.expect_err("none free");
    assert!(matches!(sort_pool(error), Failure::Temporary(_)));
}

/// A refused connection keeps the server's own words, and a closed pool is not worth retrying.
#[tokio::test]
async fn a_refused_connection_and_a_closed_pool_are_permanent() {
    let database = fresh_database().await.expect("a test database");
    let url = database
        .url()
        .replacen(database.name(), "no_such_database", 1);
    let missing = pool(&Settings::from_values(&url, Some("disable"))).expect("a pool");
    let error = missing.get().await.expect_err("refused");
    assert_eq!(
        sort_pool(error),
        Failure::Permanent("database \"no_such_database\" does not exist".into())
    );
    let closed = pool(&database).expect("a pool");
    closed.close();
    let error = closed.get().await.expect_err("closed");
    assert!(matches!(sort_pool(error), Failure::Permanent(_)));
}

/// A server that cannot be reached at all, such as one restarting, is worth retrying, and the message keeps the
/// cause underneath the driver's own words.
#[tokio::test]
async fn an_unreachable_server_is_temporary() {
    let database = fresh_database().await.expect("a test database");
    let port = database
        .url()
        .split(':')
        .nth(2)
        .and_then(|rest| rest.split('/').next())
        .expect("a port");
    let url = database.url().replacen(&format!(":{port}/"), ":1/", 1);
    let unreachable = pool(
        &Settings::from_values(&url, Some("disable"))
            .with_timeouts(Duration::from_secs(5), Duration::from_secs(5)),
    )
    .expect("a pool");
    let error = unreachable.get().await.expect_err("refused");
    let Failure::Temporary(why) = sort_pool(error) else {
        panic!("not temporary");
    };
    assert!(why.contains("refused"), "{why}");
}
