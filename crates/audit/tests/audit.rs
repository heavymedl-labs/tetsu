//! The audit record, built from events the dispatcher delivers (ADR-0043, ADR-0045). Events arrive twice and in
//! shuffled order, as delivery allows, and each is recorded once, for its own tenant alone. Each test copies a
//! database migrated once for these modules.

use tetsu_audit::{Audit, consumer};
use tetsu_kernel::{Migration, Module, Storage};
use tetsu_postgres::test_support::{TestDatabase, migrated_database};
use tetsu_postgres::{
    Dispatcher, ModuleTransaction, NewEvent, OUTBOX, Pool, TenantTransaction, connect, pool,
};

#[cfg(test)]
mod support {
    use super::*;

    /// Stands in for Identity: a module with outboxes and nothing else.
    pub struct Logins;

    const LOGINS: &[Migration] = &[Migration {
        version: 1,
        sql: OUTBOX,
    }];

    impl Module for Logins {
        fn name(&self) -> &'static str {
            "logins"
        }
        fn storage(&self) -> Option<Storage> {
            Some(Storage {
                schema: "logins",
                migrations: LOGINS,
            })
        }
    }

    /// Another producer, whose events Audit does not record.
    pub struct Bookings;

    impl Module for Bookings {
        fn name(&self) -> &'static str {
            "bookings"
        }
        fn storage(&self) -> Option<Storage> {
            Some(Storage {
                schema: "bookings",
                migrations: LOGINS,
            })
        }
    }

    pub async fn ready() -> (Pool, Dispatcher, TestDatabase) {
        let database = migrated_database(&[&Logins, &Bookings, &Audit])
            .await
            .expect("a migrated test database");
        let pool = pool(&database.as_app()).expect("pool");
        let dispatcher = Dispatcher::new(
            pool.clone(),
            &["logins", "bookings", "audit"],
            vec![consumer(&["logins"])],
        )
        .await
        .expect("a dispatcher");
        (pool, dispatcher, database)
    }

    pub fn logged_in(account: u32) -> String {
        format!(r#"{{"account_id": {account}}}"#)
    }

    pub async fn record(pool: &Pool, schema: &str, tenant: Option<i64>, payload: &str) {
        let event = NewEvent {
            kind: "logins.logged_in",
            version: 1,
            payload,
            keep: false,
        };
        let mut client = pool.get().await.expect("a connection");
        if let Some(tenant) = tenant {
            let transaction = TenantTransaction::begin(&mut client, schema, tenant)
                .await
                .expect("begins");
            transaction.record(&event).await.expect("records");
            transaction.commit().await.expect("commits");
        } else {
            let transaction = ModuleTransaction::begin(&mut client, schema)
                .await
                .expect("begins");
            transaction.record(&event).await.expect("records");
            transaction.commit().await.expect("commits");
        }
    }

    /// Delivers everything due, then again as if each delivery had been lost, each time in a shuffled order.
    pub async fn deliver_twice_shuffled(dispatcher: &Dispatcher, database: &TestDatabase) {
        for _ in 0..2 {
            run_as_owner(
                database,
                "UPDATE logins.outbox SET delivered_at = NULL, due_at = now() - random() * interval '1 minute';
                 UPDATE logins.outbox_global SET delivered_at = NULL, due_at = now() - random() * interval '1 minute';
                 UPDATE bookings.outbox SET delivered_at = NULL",
            )
            .await;
            dispatcher.deliver_due().await.expect("delivers");
        }
    }

    pub async fn run_as_owner(database: &TestDatabase, sql: &str) {
        connect(database)
            .await
            .expect("connects")
            .batch_execute(sql)
            .await
            .expect("runs");
    }

    /// The payloads of the records one tenant sees, as Audit's own role, in order.
    pub async fn seen_by(pool: &Pool, tenant: i64) -> Vec<String> {
        let mut client = pool.get().await.expect("a connection");
        let transaction = TenantTransaction::begin(&mut client, "audit", tenant)
            .await
            .expect("begins");
        transaction
            .query(
                "SELECT payload::text FROM audit.records ORDER BY payload::text",
                &[],
            )
            .await
            .expect("reads")
            .iter()
            .map(|row| row.try_get(0).expect("text"))
            .collect()
    }
}

use support::{deliver_twice_shuffled, logged_in, ready, record, run_as_owner, seen_by};

#[tokio::test]
async fn each_event_is_recorded_once_for_its_own_tenant() {
    let (pool, dispatcher, database) = ready().await;
    for (tenant, account) in [(1, 1), (2, 2), (1, 3), (2, 4), (1, 5)] {
        record(&pool, "logins", Some(tenant), &logged_in(account)).await;
    }
    deliver_twice_shuffled(&dispatcher, &database).await;
    let shown = |accounts: &[u32]| -> Vec<String> {
        accounts
            .iter()
            .map(|account| format!(r#"{{"account_id": {account}}}"#))
            .collect()
    };
    assert_eq!(seen_by(&pool, 1).await, shown(&[1, 3, 5]));
    assert_eq!(seen_by(&pool, 2).await, shown(&[2, 4]));
    assert_eq!(seen_by(&pool, 3).await, Vec::<String>::new());
}

#[tokio::test]
async fn a_record_keeps_where_the_event_came_from_and_when() {
    let (pool, dispatcher, database) = ready().await;
    record(&pool, "logins", Some(1), &logged_in(1)).await;
    deliver_twice_shuffled(&dispatcher, &database).await;
    let row = connect(&database)
        .await
        .expect("connects")
        .query_one(
            "SELECT r.producer, r.event_type, r.event_version, r.occurred_at = o.occurred_at, r.event_id = o.id
             FROM audit.records r, logins.outbox o",
            &[],
        )
        .await
        .expect("reads");
    let fields: (String, String, i64, bool, bool) = (
        row.try_get(0).expect("a field"),
        row.try_get(1).expect("a field"),
        row.try_get(2).expect("a field"),
        row.try_get(3).expect("a field"),
        row.try_get(4).expect("a field"),
    );
    assert_eq!(
        fields,
        ("logins".into(), "logins.logged_in".into(), 1, true, true)
    );
}

#[tokio::test]
async fn an_event_for_no_business_goes_to_the_global_record() {
    let (pool, dispatcher, database) = ready().await;
    record(&pool, "logins", None, &logged_in(9)).await;
    deliver_twice_shuffled(&dispatcher, &database).await;
    let kept: Vec<String> = connect(&database)
        .await
        .expect("connects")
        .query("SELECT payload::text FROM audit.records_global", &[])
        .await
        .expect("reads")
        .iter()
        .map(|row| row.try_get(0).expect("a field"))
        .collect();
    assert_eq!(kept, [logged_in(9)]);
    assert_eq!(seen_by(&pool, 1).await, Vec::<String>::new());
}

#[tokio::test]
async fn events_from_other_modules_are_not_recorded() {
    let (pool, dispatcher, database) = ready().await;
    record(&pool, "bookings", Some(1), &logged_in(1)).await;
    deliver_twice_shuffled(&dispatcher, &database).await;
    assert_eq!(seen_by(&pool, 1).await, Vec::<String>::new());
}

/// The record is append-only, even to Audit's own role.
#[tokio::test]
async fn audit_s_role_cannot_change_or_remove_a_record() {
    let (pool, dispatcher, database) = ready().await;
    record(&pool, "logins", Some(1), &logged_in(1)).await;
    deliver_twice_shuffled(&dispatcher, &database).await;
    for statement in [
        "UPDATE audit.records SET payload = '{}'",
        "DELETE FROM audit.records",
        "UPDATE audit.records_global SET payload = '{}'",
        "DELETE FROM audit.records_global",
        "TRUNCATE audit.records",
        "TRUNCATE audit.records_global",
    ] {
        let mut client = pool.get().await.expect("a connection");
        let transaction = TenantTransaction::begin(&mut client, "audit", 1)
            .await
            .expect("begins");
        let refused = transaction
            .execute(statement, &[])
            .await
            .expect_err("append-only");
        assert_eq!(
            refused.code(),
            Some(&tokio_postgres::error::SqlState::INSUFFICIENT_PRIVILEGE),
            "{statement}"
        );
    }
    assert_eq!(seen_by(&pool, 1).await, [logged_in(1)]);
}

/// A record that cannot be written leaves the event to be tried again, never lost.
#[tokio::test]
async fn a_record_that_cannot_be_written_is_tried_again() {
    let (pool, dispatcher, database) = ready().await;
    record(&pool, "logins", Some(1), &logged_in(1)).await;
    run_as_owner(&database, "REVOKE INSERT ON audit.records FROM tetsu_audit").await;
    assert_eq!(dispatcher.deliver_due().await.expect("tries"), 0);
    run_as_owner(
        &database,
        "GRANT INSERT ON audit.records TO tetsu_audit; UPDATE logins.outbox SET due_at = now()",
    )
    .await;
    assert_eq!(dispatcher.deliver_due().await.expect("delivers"), 1);
    assert_eq!(seen_by(&pool, 1).await, [logged_in(1)]);
}

#[tokio::test]
async fn audit_s_tables_follow_the_isolation_rules() {
    let (_pool, _dispatcher, database) = ready().await;
    let mut client = connect(&database).await.expect("connects");
    assert_eq!(
        tetsu_postgres::isolation_violations(&mut client, &["logins", "bookings", "audit"])
            .await
            .expect("checks"),
        Vec::<String>::new()
    );
}

#[test]
fn audit_registers_its_schema_with_an_inbox() {
    let storage = Audit.storage().expect("stores data");
    assert_eq!((Audit.name(), storage.schema), ("audit", "audit"));
    assert!(
        storage
            .migrations
            .iter()
            .any(|migration| migration.sql == tetsu_postgres::INBOX)
    );
}

/// The audit record may not be lost (ADR-0045), so an event Audit cannot record keeps retrying past the limit
/// other events stop at.
#[tokio::test]
async fn an_event_audit_cannot_record_is_never_given_up() {
    let (pool, dispatcher, database) = ready().await;
    record(&pool, "logins", None, &logged_in(1)).await;
    run_as_owner(
        &database,
        "REVOKE INSERT ON audit.records_global FROM tetsu_audit",
    )
    .await;
    for _ in 0..tetsu_postgres::MAX_ATTEMPTS + 2 {
        run_as_owner(&database, "UPDATE logins.outbox_global SET due_at = now()").await;
        assert_eq!(dispatcher.deliver_due().await.expect("tries"), 0);
    }
    run_as_owner(
        &database,
        "GRANT INSERT ON audit.records_global TO tetsu_audit; UPDATE logins.outbox_global SET due_at = now()",
    )
    .await;
    assert_eq!(dispatcher.deliver_due().await.expect("delivers"), 1);
}

/// A record names the event's own tenant, so a transaction opened for another tenant cannot take it.
#[tokio::test]
async fn an_event_is_recorded_only_for_its_own_tenant() {
    let (pool, _dispatcher, _database) = ready().await;
    let event = tetsu_postgres::Event {
        id: "0190a3d2-0000-7000-8000-000000000009".into(),
        producer: "logins".into(),
        tenant: Some(1),
        kind: "logins.logged_in".into(),
        version: 1,
        occurred_at: std::time::SystemTime::now(),
        payload: logged_in(1),
    };
    let mut client = pool.get().await.expect("a connection");
    let transaction = TenantTransaction::begin(&mut client, "audit", 2)
        .await
        .expect("begins");
    let refused = consumer(&["logins"])
        .apply(&transaction, &event)
        .await
        .expect_err("another tenant's event");
    assert_eq!(
        refused
            .downcast_ref::<tokio_postgres::Error>()
            .and_then(tokio_postgres::Error::code),
        Some(&tokio_postgres::error::SqlState::INSUFFICIENT_PRIVILEGE),
        "the tenant policy refuses it"
    );
}
