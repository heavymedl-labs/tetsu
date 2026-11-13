//! Domain events through the outbox (ADR-0043): recorded with the change, delivered at least once, applied once,
//! retried with backoff, and removed once delivered for a week. Everything runs as the application role.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use tetsu_kernel::{Migration, Module, Storage};
use tetsu_postgres::test_support::{TestDatabase, fresh_database};
use tetsu_postgres::{
    Applied, Consumer, DeliveryTransaction, Dispatcher, Event, INBOX, MAX_ATTEMPTS,
    ModuleTransaction, NewEvent, OUTBOX, Pool, TenantTransaction, Transaction, connect, migrate,
    pool,
};

#[cfg(test)]
mod support {
    use super::*;

    /// A module with a schema and its files.
    pub struct Named(pub &'static str, pub &'static [Migration]);

    impl Module for Named {
        fn name(&self) -> &'static str {
            self.0
        }
        fn storage(&self) -> Option<Storage> {
            Some(Storage {
                schema: self.0,
                migrations: self.1,
            })
        }
    }

    /// The producer: the isolation tests' people, with outboxes.
    pub const PEOPLE: &[Migration] = &[
        Migration {
            version: 1,
            sql: include_str!("people.sql"),
        },
        Migration {
            version: 2,
            sql: OUTBOX,
        },
    ];

    /// The consumer: a row per event it applies, and its inboxes.
    pub const LEDGER: &[Migration] = &[
        Migration {
            version: 1,
            sql: include_str!("ledger.sql"),
        },
        Migration {
            version: 2,
            sql: INBOX,
        },
    ];

    /// Writes one row per event, and counts its calls, which a repeat its inbox skips does not reach.
    #[derive(Default)]
    pub struct Ledger {
        pub calls: AtomicUsize,
        pub failing: AtomicBool,
    }

    impl Consumer for Ledger {
        fn schema(&self) -> &'static str {
            "ledger"
        }
        fn wants(&self, event: &Event) -> bool {
            event.producer == "people" && event.kind == "people.joined"
        }
        fn apply<'a>(&'a self, transaction: &'a Transaction<'a>, event: &'a Event) -> Applied<'a> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                if self.failing.load(Ordering::SeqCst) {
                    return Err("the ledger is down".into());
                }
                let sql = match event.tenant {
                    Some(_) => "INSERT INTO ledger.effects (tenant_id, event_id)
                                VALUES (nullif(current_setting('app.tenant_id', true), '')::bigint, $1::text::uuid)",
                    None => "INSERT INTO ledger.global_effects (event_id) VALUES ($1::text::uuid)",
                };
                transaction.execute(sql, &[&event.id]).await?;
                Ok(())
            })
        }
    }

    pub struct World {
        pub pool: Pool,
        pub ledger: Arc<Ledger>,
        pub dispatcher: Dispatcher,
        pub database: TestDatabase,
    }

    pub async fn world() -> World {
        let database = fresh_database().await.expect("a test database");
        migrate(
            &database,
            &[&Named("people", PEOPLE), &Named("ledger", LEDGER)],
        )
        .await
        .expect("migrates");
        let pool = pool(&database.as_app()).expect("pool");
        let ledger = Arc::new(Ledger::default());
        let consumers: Vec<Arc<dyn Consumer>> = vec![ledger.clone()];
        let dispatcher = Dispatcher::new(pool.clone(), &["people", "ledger"], consumers)
            .await
            .expect("finds the producers");
        World {
            pool,
            ledger,
            dispatcher,
            database,
        }
    }

    pub const JOINED: NewEvent<'static> = NewEvent {
        kind: "people.joined",
        version: 1,
        payload: r#"{"person_id": "0190a3d2-0000-7000-8000-000000000001"}"#,
        keep: false,
    };

    /// Records an event for `tenant` and commits, or rolls back with `commit` false.
    pub async fn record(pool: &Pool, tenant: i64, event: &NewEvent<'_>, commit: bool) -> String {
        let mut client = pool.get().await.expect("a connection");
        let transaction = TenantTransaction::begin(&mut client, "people", tenant)
            .await
            .expect("begins");
        let id = transaction.record(event).await.expect("records");
        if commit {
            transaction.commit().await.expect("commits");
        }
        id
    }

    pub async fn record_global(pool: &Pool, event: &NewEvent<'_>) -> String {
        let mut client = pool.get().await.expect("a connection");
        let transaction = ModuleTransaction::begin(&mut client, "people")
            .await
            .expect("begins");
        let id = transaction.record(event).await.expect("records");
        transaction.commit().await.expect("commits");
        id
    }

    /// One value from the database, read as its owner, which sees every tenant's rows.
    pub async fn read<T: for<'r> tokio_postgres::types::FromSql<'r>>(
        database: &TestDatabase,
        sql: &str,
    ) -> T {
        connect(database)
            .await
            .expect("connects")
            .query_one(sql, &[])
            .await
            .expect("reads")
            .try_get(0)
            .expect("a value")
    }

    /// Log lines a test's dispatcher writes.
    #[derive(Clone, Default)]
    pub struct Logs(Arc<std::sync::Mutex<Vec<u8>>>);

    impl Logs {
        pub fn subscriber(&self) -> impl tracing::Subscriber + Send + Sync {
            let logs = self.clone();
            tracing_subscriber::fmt()
                .json()
                .with_writer(move || logs.clone())
                .finish()
        }

        pub fn lines(&self) -> Vec<String> {
            let bytes = self.0.lock().expect("unpoisoned").clone();
            String::from_utf8(bytes)
                .expect("UTF-8")
                .lines()
                .map(ToString::to_string)
                .collect()
        }
    }

    impl std::io::Write for Logs {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("unpoisoned").extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
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
}

use support::{JOINED, Logs, World, read, record, record_global, run_as_owner, world};

#[tokio::test]
async fn an_event_is_delivered_only_if_its_change_commits() {
    let World {
        pool,
        ledger,
        dispatcher,
        database,
    } = world().await;
    record(&pool, 1, &JOINED, false).await;
    let kept = record(&pool, 1, &JOINED, true).await;
    assert_eq!(dispatcher.deliver_due().await.expect("delivers"), 1);
    assert_eq!(ledger.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        read::<String>(
            &database,
            "SELECT string_agg(event_id::text, ',') FROM ledger.effects"
        )
        .await,
        kept
    );
    assert_eq!(
        dispatcher.deliver_due().await.expect("delivers"),
        0,
        "nothing left"
    );
}

#[tokio::test]
async fn an_event_delivered_twice_takes_effect_once() {
    let World {
        pool,
        ledger,
        dispatcher,
        database,
    } = world().await;
    record(&pool, 1, &JOINED, true).await;
    dispatcher.deliver_due().await.expect("delivers");
    // As if the dispatcher stopped after the consumer committed but before it settled the event.
    run_as_owner(&database, "UPDATE people.outbox SET delivered_at = NULL").await;
    assert_eq!(dispatcher.deliver_due().await.expect("delivers again"), 1);
    assert_eq!(
        read::<i64>(&database, "SELECT count(*) FROM ledger.effects").await,
        1
    );
    assert_eq!(
        ledger.calls.load(Ordering::SeqCst),
        1,
        "the inbox skips the repeat"
    );
}

#[tokio::test]
async fn an_event_for_no_business_goes_through_the_global_inbox() {
    let World {
        pool,
        dispatcher,
        database,
        ..
    } = world().await;
    let id = record_global(&pool, &JOINED).await;
    dispatcher.deliver_due().await.expect("delivers");
    run_as_owner(
        &database,
        "UPDATE people.outbox_global SET delivered_at = NULL",
    )
    .await;
    dispatcher.deliver_due().await.expect("delivers again");
    assert_eq!(
        read::<String>(
            &database,
            "SELECT string_agg(event_id::text, ',') FROM ledger.global_effects"
        )
        .await,
        id
    );
}

#[tokio::test]
async fn events_nobody_wants_are_settled_at_once() {
    let World {
        pool,
        ledger,
        dispatcher,
        database,
    } = world().await;
    let ignored = tetsu_postgres::NewEvent {
        kind: "people.left",
        ..JOINED
    };
    record(&pool, 1, &ignored, true).await;
    assert_eq!(dispatcher.deliver_due().await.expect("delivers"), 1);
    assert_eq!(ledger.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        read::<i64>(
            &database,
            "SELECT count(*) FROM people.outbox WHERE delivered_at IS NOT NULL"
        )
        .await,
        1
    );
}

/// The wait doubles from about a second, with jitter, up to an hour.
#[tokio::test]
async fn a_failure_waits_longer_each_time_up_to_an_hour() {
    let World {
        pool,
        ledger,
        dispatcher,
        database,
    } = world().await;
    ledger.failing.store(true, Ordering::SeqCst);
    record(&pool, 1, &JOINED, true).await;
    for (attempts, at_most) in [(1, 1.0), (2, 2.0), (3, 4.0), (13, 3600.0)] {
        run_as_owner(
            &database,
            &format!(
                "UPDATE people.outbox SET attempts = {}, due_at = now(), keep = true",
                attempts - 1
            ),
        )
        .await;
        assert_eq!(dispatcher.deliver_due().await.expect("tries"), 0);
        let wait: f64 = read(
            &database,
            "SELECT extract(epoch FROM due_at - now())::float8 FROM people.outbox",
        )
        .await;
        assert!(
            wait >= at_most / 2.0 - 0.5 && wait <= at_most + 0.5,
            "{attempts}: {wait}"
        );
    }
}

#[tokio::test]
async fn an_event_stops_after_ten_failures_and_says_so() {
    let World {
        pool,
        ledger,
        dispatcher,
        database,
    } = world().await;
    ledger.failing.store(true, Ordering::SeqCst);
    let id = record(&pool, 7, &JOINED, true).await;
    let logs = Logs::default();
    let _guard = tracing::subscriber::set_default(logs.subscriber());
    for _ in 0..MAX_ATTEMPTS + 2 {
        run_as_owner(&database, "UPDATE people.outbox SET due_at = now()").await;
        dispatcher.deliver_due().await.expect("tries");
    }
    assert_eq!(
        read::<i64>(&database, "SELECT attempts FROM people.outbox").await,
        MAX_ATTEMPTS
    );
    let stopped: Vec<String> = logs
        .lines()
        .into_iter()
        .filter(|line| line.contains("event stopped retrying"))
        .collect();
    assert_eq!(stopped.len(), 1, "{stopped:?}");
    for field in [id.as_str(), "people.joined", "ledger", "ERROR"] {
        assert!(stopped[0].contains(field), "{field} in {}", stopped[0]);
    }
}

#[tokio::test]
async fn a_kept_event_keeps_retrying_past_ten_failures() {
    let World {
        pool,
        ledger,
        dispatcher,
        database,
    } = world().await;
    ledger.failing.store(true, Ordering::SeqCst);
    let kept = NewEvent {
        keep: true,
        ..JOINED
    };
    record(&pool, 1, &kept, true).await;
    for _ in 0..MAX_ATTEMPTS + 2 {
        run_as_owner(&database, "UPDATE people.outbox SET due_at = now()").await;
        dispatcher.deliver_due().await.expect("tries");
    }
    ledger.failing.store(false, Ordering::SeqCst);
    run_as_owner(&database, "UPDATE people.outbox SET due_at = now()").await;
    assert_eq!(dispatcher.deliver_due().await.expect("delivers"), 1);
    assert_eq!(
        read::<i64>(&database, "SELECT count(*) FROM ledger.effects").await,
        1
    );
}

/// Two instances polling at once each take rows the other has not locked, so every event is applied once.
#[tokio::test]
async fn two_dispatchers_at_once_share_the_events() {
    let World {
        pool,
        ledger,
        dispatcher,
        database,
    } = world().await;
    for tenant in 1..=40 {
        record(&pool, tenant, &JOINED, true).await;
    }
    let consumers: Vec<Arc<dyn Consumer>> = vec![ledger.clone()];
    let other = Dispatcher::new(pool.clone(), &["people"], consumers)
        .await
        .expect("a second instance");
    let (first, second) = tokio::join!(dispatcher.deliver_due(), other.deliver_due());
    let (first, second) = (first.expect("delivers"), second.expect("delivers"));
    assert_eq!(first + second, 40);
    assert_eq!(
        read::<i64>(&database, "SELECT count(*) FROM ledger.effects").await,
        40
    );
    assert_eq!(ledger.calls.load(Ordering::SeqCst), 40);
}

#[tokio::test]
async fn delivered_events_go_after_seven_days() {
    let World {
        pool,
        dispatcher,
        database,
        ..
    } = world().await;
    for _ in 0..3 {
        record(&pool, 1, &JOINED, true).await;
    }
    record_global(&pool, &JOINED).await;
    dispatcher.deliver_due().await.expect("delivers");
    record(&pool, 1, &JOINED, true).await;
    run_as_owner(
        &database,
        "WITH oldest AS (SELECT id FROM people.outbox WHERE delivered_at IS NOT NULL LIMIT 2)
           UPDATE people.outbox SET delivered_at = now() - interval '7 days 1 minute' WHERE id IN (SELECT id FROM oldest);
         UPDATE people.outbox SET delivered_at = now() - interval '6 days 23 hours'
           WHERE delivered_at > now() - interval '1 hour';
         UPDATE people.outbox_global SET delivered_at = now() - interval '8 days'",
    )
    .await;
    assert_eq!(
        dispatcher
            .remove_delivered(tetsu_postgres::KEPT_FOR)
            .await
            .expect("removes"),
        3
    );
    assert_eq!(
        read::<i64>(&database, "SELECT count(*) FROM people.outbox").await,
        2
    );
}

/// The dispatcher wakes when this process commits an event, rather than waiting for its next poll.
#[tokio::test]
async fn a_committed_event_wakes_the_dispatcher() {
    let World {
        pool,
        dispatcher,
        database,
        ..
    } = world().await;
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let running = tokio::spawn(async move {
        dispatcher
            .run(Duration::from_secs(600), async {
                let _ = stopped.await;
            })
            .await;
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    record(&pool, 1, &JOINED, true).await;
    let mut applied = 0;
    for _ in 0..50 {
        applied = read::<i64>(&database, "SELECT count(*) FROM ledger.effects").await;
        if applied == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let _ = stop.send(());
    running.await.expect("stops");
    assert_eq!(applied, 1);
}

#[tokio::test]
async fn the_delivery_role_reaches_every_tenant_s_outbox_and_nothing_else() {
    let World {
        pool,
        database: _database,
        ..
    } = world().await;
    record(&pool, 1, &JOINED, true).await;
    record(&pool, 2, &JOINED, true).await;
    let mut client = pool.get().await.expect("a connection");
    let transaction = DeliveryTransaction::begin(&mut client)
        .await
        .expect("begins");
    let seen: i64 = transaction
        .query_one("SELECT count(*) FROM people.outbox", &[])
        .await
        .expect("reads every tenant's events")
        .try_get(0)
        .expect("a count");
    assert_eq!(seen, 2);
    transaction.commit().await.expect("commits");
    for table in [
        "people.people",
        "people.notes",
        "ledger.inbox",
        "ledger.effects",
    ] {
        let transaction = DeliveryTransaction::begin(&mut client)
            .await
            .expect("begins");
        let refused = transaction
            .query_one(&format!("SELECT count(*) FROM {table}"), &[])
            .await
            .expect_err("no rights here");
        assert_eq!(
            refused.code(),
            Some(&tokio_postgres::error::SqlState::INSUFFICIENT_PRIVILEGE),
            "{table}"
        );
    }
}

#[tokio::test]
async fn outboxes_and_inboxes_follow_the_isolation_rules() {
    let World { database, .. } = world().await;
    let mut client = connect(&database).await.expect("connects");
    assert_eq!(
        tetsu_postgres::isolation_violations(&mut client, &["people", "ledger"])
            .await
            .expect("checks"),
        Vec::<String>::new()
    );
}

/// The dispatcher removes old events when it starts, then once an hour, not on every pass.
#[tokio::test]
async fn old_events_are_removed_at_start_then_hourly() {
    let World {
        pool,
        dispatcher,
        database,
        ..
    } = world().await;
    record(&pool, 1, &JOINED, true).await;
    dispatcher.deliver_due().await.expect("delivers");
    let age = "UPDATE people.outbox SET delivered_at = now() - interval '8 days' WHERE delivered_at IS NOT NULL";
    run_as_owner(&database, age).await;
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let running = tokio::spawn(async move {
        dispatcher
            .run(Duration::from_secs(600), async {
                let _ = stopped.await;
            })
            .await;
    });
    let count = |sql: &'static str| {
        let database = &database;
        async move { read::<i64>(database, sql).await }
    };
    let settled = |wanted: i64, sql: &'static str| async move {
        for _ in 0..50 {
            if count(sql).await == wanted {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        false
    };
    let removed_at_start = settled(0, "SELECT count(*) FROM people.outbox").await;
    record(&pool, 1, &JOINED, true).await;
    let delivered = "SELECT count(*) FROM people.outbox WHERE delivered_at IS NOT NULL";
    let first_woken = settled(1, delivered).await;
    run_as_owner(&database, age).await;
    record(&pool, 1, &JOINED, true).await;
    let second_woken = settled(2, delivered).await;
    let _ = stop.send(());
    running.await.expect("stops");
    assert!(removed_at_start && first_woken && second_woken);
    assert_eq!(
        count("SELECT count(*) FROM people.outbox WHERE delivered_at < now() - interval '7 days'")
            .await,
        1,
        "an old event waits for the next hourly removal"
    );
}
