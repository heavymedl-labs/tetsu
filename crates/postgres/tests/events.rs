//! Domain events through the outbox (ADR-0043): recorded with the change, delivered at least once, applied once,
//! retried with backoff, and removed past their use: once delivered for a week, and after 30 days for an event
//! that stopped retrying or an inbox's ID no outbox still holds (ADR-0080). Everything runs as the application role.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use tetsu_kernel::{Migration, Module, Storage};
use tetsu_postgres::test_support::{TestDatabase, fresh_database};
use tetsu_postgres::{
    Applied, Consumer, DeliveryTransaction, Dispatcher, Event, INBOX, INBOX_DELIVERY, MAX_ATTEMPTS,
    ModuleTransaction, NewEvent, OUTBOX, Pool, Removed, TenantTransaction, Transaction, connect,
    migrate, pool,
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
        // A second producer, so a pass has more than one outbox to get through.
        Migration {
            version: 3,
            sql: OUTBOX,
        },
        Migration {
            version: 4,
            sql: INBOX_DELIVERY,
        },
    ];

    /// A second consumer, with its own inbox and nothing else.
    pub const JOURNAL: &[Migration] = &[
        Migration {
            version: 1,
            sql: INBOX,
        },
        Migration {
            version: 2,
            sql: INBOX_DELIVERY,
        },
    ];

    /// Writes one row per event, and counts its calls, which a repeat its inbox skips does not reach.
    #[derive(Default)]
    pub struct Ledger {
        pub calls: AtomicUsize,
        pub failing: AtomicBool,
        /// Never answers, as a consumer stuck on an outside call would.
        pub hanging: AtomicBool,
        /// Fails with a database error whose text holds a person's email.
        pub leaky: AtomicBool,
    }

    /// A consumer that must never lose an event, as Audit must not, which fails a given number of times and then
    /// applies events by recording them in its inbox alone.
    pub struct Journal {
        pub failures_left: AtomicUsize,
    }

    impl Consumer for Journal {
        fn schema(&self) -> &'static str {
            "journal"
        }
        fn wants(&self, event: &Event) -> bool {
            event.producer == "people"
        }
        fn never_lose(&self) -> bool {
            true
        }
        fn apply<'a>(&'a self, _: &'a Transaction<'a>, _: &'a Event) -> Applied<'a> {
            let failing = self
                .failures_left
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                    left.checked_sub(1)
                })
                .is_ok();
            Box::pin(async move {
                if failing {
                    Err("not ready".into())
                } else {
                    Ok(())
                }
            })
        }
    }

    /// Fails, and on its way out queues for a connection, which takes the one it is returning, so the next
    /// consumer finds the pool empty.
    pub struct Hog {
        pub pool: Pool,
        pub held: Arc<std::sync::Mutex<Vec<tetsu_postgres::Object>>>,
    }

    impl Consumer for Hog {
        fn schema(&self) -> &'static str {
            "journal"
        }
        fn wants(&self, event: &Event) -> bool {
            event.producer == "people"
        }
        fn apply<'a>(&'a self, _: &'a Transaction<'a>, _: &'a Event) -> Applied<'a> {
            let (pool, held) = (self.pool.clone(), self.held.clone());
            tokio::spawn(async move {
                if let Ok(client) = pool.get().await {
                    held.lock().expect("unpoisoned").push(client);
                }
            });
            Box::pin(async {
                // Lets the task queue for a connection before this one is returned.
                tokio::task::yield_now().await;
                Err("the journal is down".into())
            })
        }
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
                if self.hanging.load(Ordering::SeqCst) {
                    std::future::pending::<()>().await;
                }
                if self.leaky.load(Ordering::SeqCst) {
                    transaction
                        .execute("SELECT 'lee.park@example.com'::uuid", &[])
                        .await?;
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
            &[
                &Named("people", PEOPLE),
                &Named("ledger", LEDGER),
                &Named("journal", JOURNAL),
            ],
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

    /// How many rows of `table` the delivery role sees, every tenant's.
    pub async fn counted_as_delivery(client: &mut tetsu_postgres::Object, table: &str) -> i64 {
        let transaction = DeliveryTransaction::begin(client).await.expect("begins");
        let seen = transaction
            .query_one(&format!("SELECT count(*) FROM {table}"), &[])
            .await
            .expect("reads every tenant's rows")
            .try_get(0)
            .expect("a count");
        transaction.commit().await.expect("commits");
        seen
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

    thread_local! {
        static CAPTURING: std::cell::RefCell<Option<Logs>> = const { std::cell::RefCell::new(None) };
    }

    /// Stops a capture when dropped.
    pub struct Capture;

    impl Drop for Capture {
        fn drop(&mut self) {
            CAPTURING.with(|capturing| capturing.replace(None));
        }
    }

    /// Writes to the capture of the thread logging, or nowhere.
    struct ThisThread;

    impl std::io::Write for ThisThread {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            CAPTURING.with(|capturing| match &mut *capturing.borrow_mut() {
                Some(logs) => logs.write(bytes),
                None => Ok(bytes.len()),
            })
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Logs {
        /// Captures what this thread logs until the returned guard drops. One subscriber serves the whole binary:
        /// tracing caches across threads whether any subscriber wants a log call, so a subscriber set for one
        /// thread can lose its lines while another test's thread sets or drops its own.
        pub fn capture(&self) -> Capture {
            static GLOBAL: std::sync::Once = std::sync::Once::new();
            GLOBAL.call_once(|| {
                let subscriber = tracing_subscriber::fmt()
                    .json()
                    .with_writer(|| ThisThread)
                    .finish();
                tracing::subscriber::set_global_default(subscriber).expect("the only subscriber");
            });
            CAPTURING.with(|capturing| capturing.replace(Some(self.clone())));
            Capture
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

use support::{
    JOINED, Logs, World, counted_as_delivery, read, record, record_global, run_as_owner, world,
};

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
    let _guard = logs.capture();
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
    for field in [
        id.as_str(),
        "people.joined",
        "ledger",
        "ERROR",
        "\"alert\":\"event_stopped\"",
    ] {
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
        dispatcher.remove_spent().await.expect("removes"),
        Removed {
            delivered: 3,
            stopped: 0,
            applied: 0,
        }
    );
    assert_eq!(
        read::<i64>(&database, "SELECT count(*) FROM people.outbox").await,
        2
    );
}

/// An event that stopped retrying goes 30 days after its last attempt; one still retrying, or marked never to be
/// lost, stays however old it is.
#[tokio::test]
async fn stopped_events_go_after_thirty_days() {
    let World {
        pool,
        dispatcher,
        database,
        ..
    } = world().await;
    let mut ids = Vec::new();
    for _ in 0..4 {
        ids.push(record(&pool, 1, &JOINED, true).await);
    }
    let global = record_global(&pool, &JOINED).await;
    // Gone: the first, and the global one. Kept: one stopped under 30 days, one never to be lost, one retrying.
    let changes: String = [
        ("outbox", &ids[0], false, "30 days 1 minute", MAX_ATTEMPTS),
        ("outbox", &ids[1], false, "29 days 23 hours", MAX_ATTEMPTS),
        ("outbox", &ids[2], true, "60 days", MAX_ATTEMPTS),
        ("outbox", &ids[3], false, "60 days", MAX_ATTEMPTS - 1),
        ("outbox_global", &global, false, "31 days", MAX_ATTEMPTS),
    ]
    .map(|(table, id, keep, ago, attempts)| {
        format!(
            "UPDATE people.{table} SET attempts = {attempts}, keep = {keep}, due_at = now() - interval '{ago}'
             WHERE id = '{id}';"
        )
    })
    .concat();
    run_as_owner(&database, &changes).await;
    assert_eq!(
        dispatcher.remove_spent().await.expect("removes"),
        Removed {
            delivered: 0,
            stopped: 2,
            applied: 0,
        }
    );
    let left = read::<String>(
        &database,
        "SELECT string_agg(id::text, ' ' ORDER BY id) FROM people.outbox",
    )
    .await;
    assert_eq!(
        left,
        [&ids[1], &ids[2], &ids[3]].map(String::as_str).join(" ")
    );
    assert_eq!(
        read::<i64>(&database, "SELECT count(*) FROM people.outbox_global").await,
        0
    );
}

/// An inbox keeps an applied event's ID for 30 days, and after that only while an outbox of its kind still holds
/// the event, which could then come again.
#[tokio::test]
async fn inbox_ids_go_once_no_outbox_holds_their_event() {
    let World {
        pool,
        dispatcher,
        database,
        ..
    } = world().await;
    let mut ids = Vec::new();
    for _ in 0..3 {
        ids.push(record(&pool, 1, &JOINED, true).await);
    }
    let global = record_global(&pool, &JOINED).await;
    dispatcher.deliver_due().await.expect("delivers");
    // The first still has its outbox row; the others' went, and the third was applied just under 30 days ago.
    run_as_owner(
        &database,
        &format!(
            "UPDATE ledger.inbox SET applied_at = now() - interval '30 days 1 minute' WHERE id <> '{}';
             UPDATE ledger.inbox SET applied_at = now() - interval '29 days 23 hours' WHERE id = '{}';
             UPDATE ledger.inbox_global SET applied_at = now() - interval '31 days';
             DELETE FROM people.outbox WHERE id <> '{}';
             DELETE FROM people.outbox_global;",
            ids[2], ids[2], ids[0]
        ),
    )
    .await;
    assert_eq!(
        dispatcher.remove_spent().await.expect("removes"),
        Removed {
            delivered: 0,
            stopped: 0,
            applied: 2,
        }
    );
    let left = read::<String>(
        &database,
        "SELECT string_agg(id::text, ' ' ORDER BY id) FROM ledger.inbox",
    )
    .await;
    assert_eq!(left, [&ids[0], &ids[2]].map(String::as_str).join(" "));
    assert_eq!(
        read::<i64>(
            &database,
            &format!("SELECT count(*) FROM ledger.inbox_global WHERE id = '{global}'")
        )
        .await,
        0
    );
}

/// A global inbox's ID stays while a global outbox still holds its event, as a tenant inbox's does.
#[tokio::test]
async fn a_global_inbox_id_stays_while_its_event_is_held() {
    let World {
        pool,
        dispatcher,
        database,
        ..
    } = world().await;
    let global = record_global(&pool, &JOINED).await;
    dispatcher.deliver_due().await.expect("delivers");
    run_as_owner(
        &database,
        "UPDATE ledger.inbox_global SET applied_at = now() - interval '31 days'",
    )
    .await;
    assert_eq!(dispatcher.remove_spent().await.expect("removes").applied, 0);
    let held = format!("SELECT count(*) FROM ledger.inbox_global WHERE id = '{global}'");
    assert_eq!(read::<i64>(&database, &held).await, 1);
}

/// A large inbox goes a thousand IDs a statement, twenty statements a pass across every inbox, so delivery waits
/// for no more, and what is left goes in the next.
#[tokio::test]
async fn inbox_ids_go_in_bounded_batches() {
    let World {
        dispatcher,
        database,
        ..
    } = world().await;
    run_as_owner(
        &database,
        "INSERT INTO ledger.inbox_global (id, applied_at)
         SELECT gen_random_uuid(), now() - interval '31 days' FROM generate_series(1, 20005)",
    )
    .await;
    assert_eq!(
        dispatcher.remove_spent().await.expect("removes").applied,
        20_000
    );
    assert_eq!(dispatcher.remove_spent().await.expect("removes").applied, 5);
    assert_eq!(
        read::<i64>(&database, "SELECT count(*) FROM ledger.inbox_global").await,
        0
    );
}

/// An ID stays while any outbox holds its event, even one in a module this dispatcher does not deliver, as an
/// instance of a newer build might.
#[tokio::test]
async fn an_inbox_id_stays_while_an_outbox_this_dispatcher_does_not_know_holds_it() {
    let World {
        pool,
        ledger,
        database,
        ..
    } = world().await;
    let consumers: Vec<Arc<dyn Consumer>> = vec![ledger];
    let dispatcher = Dispatcher::new(pool, &["people"], consumers)
        .await
        .expect("finds the producers");
    run_as_owner(
        &database,
        "INSERT INTO ledger.outbox_global (id, event_type, event_version, payload)
           VALUES ('0190a3d2-0000-7000-8000-0000000000aa', 'ledger.noted', 1, '{}');
         INSERT INTO ledger.inbox_global (id, applied_at)
           VALUES ('0190a3d2-0000-7000-8000-0000000000aa', now() - interval '31 days')",
    )
    .await;
    assert_eq!(dispatcher.remove_spent().await.expect("removes").applied, 0);
}

/// Only the shared outbox counts: a table of that name elsewhere, or a session's temporary one, is passed over, a
/// shared one in a schema whose name needs quoting still holds its events' IDs, and one the delivery role can no
/// longer read fails the cleanup rather than being passed over.
#[tokio::test]
async fn only_shared_outboxes_hold_inbox_ids() {
    let World {
        dispatcher,
        database,
        ..
    } = world().await;
    let held = "0190a3d2-0000-7000-8000-0000000000bb";
    run_as_owner(
        &database,
        &format!(
            "CREATE TABLE public.outbox (id uuid);
             CREATE SCHEMA \"Odd Name\";
             CREATE TABLE \"Odd Name\".outbox (tenant_id bigint, id uuid);
             CREATE TABLE \"Odd Name\".outbox_global (id uuid);
             CREATE POLICY delivery ON \"Odd Name\".outbox TO tetsu_delivery USING (TRUE);
             GRANT USAGE ON SCHEMA \"Odd Name\" TO tetsu_delivery;
             GRANT SELECT ON \"Odd Name\".outbox, \"Odd Name\".outbox_global TO tetsu_delivery;
             INSERT INTO \"Odd Name\".outbox_global VALUES ('{held}');
             INSERT INTO ledger.inbox_global VALUES ('{held}', now() - interval '31 days')"
        ),
    )
    .await;
    // A temporary outbox, with the delivery role's policy, in a session that stays open while the cleanup runs.
    let session = connect(&database).await.expect("connects");
    session
        .batch_execute(
            "CREATE TEMP TABLE outbox (id uuid);
             CREATE POLICY delivery ON outbox TO tetsu_delivery USING (TRUE)",
        )
        .await
        .expect("makes a temporary outbox");
    assert_eq!(dispatcher.remove_spent().await.expect("removes").applied, 0);
    let kept = format!("SELECT count(*) FROM ledger.inbox_global WHERE id = '{held}'");
    assert_eq!(read::<i64>(&database, &kept).await, 1);
    run_as_owner(
        &database,
        "REVOKE SELECT ON \"Odd Name\".outbox_global FROM tetsu_delivery",
    )
    .await;
    assert!(dispatcher.remove_spent().await.is_err());
    drop(session);
}

/// An outbox this dispatcher delivers that no longer holds the delivery role's policy, as when the policy is
/// renamed, stops the cleanup with an error, rather than letting its events' IDs go.
#[tokio::test]
async fn a_delivered_outbox_that_lost_its_policy_stops_the_cleanup() {
    let World {
        pool,
        dispatcher,
        database,
        ..
    } = world().await;
    record(&pool, 1, &JOINED, true).await;
    dispatcher.deliver_due().await.expect("delivers");
    run_as_owner(
        &database,
        "UPDATE people.outbox SET delivered_at = now() - interval '8 days';
         ALTER POLICY delivery ON people.outbox RENAME TO deliver",
    )
    .await;
    let stopped = dispatcher
        .remove_spent()
        .await
        .err()
        .map(|error| error.to_string());
    assert_eq!(
        stopped.as_deref(),
        Some("people's outbox lacks the delivery role's policy")
    );
    // What came before stays done: the delivered event went.
    assert_eq!(
        read::<i64>(&database, "SELECT count(*) FROM people.outbox").await,
        0
    );
}

/// The cap on batches holds across every inbox in a pass, not each.
#[tokio::test]
async fn the_batch_cap_holds_across_inboxes() {
    let World {
        pool,
        ledger,
        database,
        ..
    } = world().await;
    let journal = Arc::new(support::Journal {
        failures_left: AtomicUsize::new(0),
    });
    let consumers: Vec<Arc<dyn Consumer>> = vec![ledger, journal];
    let dispatcher = Dispatcher::new(pool, &["people"], consumers)
        .await
        .expect("a dispatcher");
    run_as_owner(
        &database,
        "INSERT INTO ledger.inbox_global (id, applied_at)
         SELECT gen_random_uuid(), now() - interval '31 days' FROM generate_series(1, 15000);
         INSERT INTO journal.inbox_global (id, applied_at)
         SELECT gen_random_uuid(), now() - interval '31 days' FROM generate_series(1, 15000)",
    )
    .await;
    assert_eq!(
        dispatcher.remove_spent().await.expect("removes").applied,
        20_000
    );
    assert_eq!(
        dispatcher.remove_spent().await.expect("removes").applied,
        10_000
    );
}

/// An event that runs out of attempts while the pool had no connection for a consumer that must never lose it is
/// kept too.
#[tokio::test]
async fn an_event_a_starved_never_lose_consumer_needs_is_kept() {
    let World { pool, database, .. } = world().await;
    let held = Arc::new(std::sync::Mutex::new(Vec::new()));
    let hog = Arc::new(support::Hog {
        pool: pool.clone(),
        held: held.clone(),
    });
    let journal = Arc::new(support::Journal {
        failures_left: AtomicUsize::new(0),
    });
    let consumers: Vec<Arc<dyn Consumer>> = vec![hog, journal];
    let dispatcher = Dispatcher::new(pool.clone(), &["people"], consumers)
        .await
        .expect("a dispatcher");
    record(&pool, 1, &JOINED, true).await;
    run_as_owner(
        &database,
        &format!("UPDATE people.outbox SET attempts = {}", MAX_ATTEMPTS - 1),
    )
    .await;
    // Every connection but the dispatcher's own and the one the first consumer uses.
    let mut taken = Vec::new();
    for _ in 2..pool.status().max_size {
        taken.push(pool.get().await.expect("a connection"));
    }
    dispatcher.deliver_due().await.expect("ends quietly");
    drop(taken);
    held.lock().expect("unpoisoned").clear();
    assert!(read::<bool>(&database, "SELECT keep FROM people.outbox").await);
}

/// A cleanup step that fails is logged as an error, and the steps before it stay done.
#[tokio::test]
async fn a_failed_cleanup_is_logged_and_keeps_what_it_did() {
    let World {
        pool,
        dispatcher,
        database,
        ..
    } = world().await;
    record(&pool, 1, &JOINED, true).await;
    dispatcher.deliver_due().await.expect("delivers");
    run_as_owner(
        &database,
        "UPDATE people.outbox SET delivered_at = now() - interval '8 days';
         REVOKE DELETE ON ledger.inbox_global FROM tetsu_delivery",
    )
    .await;
    let logs = Logs::default();
    let _guard = logs.capture();
    dispatcher.run(Duration::from_secs(600), async {}).await;
    let failed: Vec<String> = logs
        .lines()
        .into_iter()
        .filter(|line| line.contains("removing event rows past their use failed"))
        .collect();
    assert_eq!(failed.len(), 1, "{:?}", logs.lines());
    assert!(failed[0].contains("ERROR"), "{}", failed[0]);
    assert_eq!(
        read::<i64>(&database, "SELECT count(*) FROM people.outbox").await,
        0
    );
}

/// An event that runs out of attempts while a consumer that must never lose it was not yet reached is kept, as if
/// that consumer had failed, so it is never removed undelivered.
#[tokio::test]
async fn an_event_a_skipped_never_lose_consumer_needs_is_kept() {
    let World {
        pool,
        ledger,
        database,
        ..
    } = world().await;
    ledger.failing.store(true, Ordering::SeqCst);
    let journal = Arc::new(support::Journal {
        failures_left: AtomicUsize::new(0),
    });
    let consumers: Vec<Arc<dyn Consumer>> = vec![ledger.clone(), journal];
    // A pass with no time left reaches only its first consumer.
    let dispatcher = Dispatcher::new(pool.clone(), &["people"], consumers)
        .await
        .expect("finds the producers")
        .with_pass_limit(Duration::ZERO);
    record(&pool, 1, &JOINED, true).await;
    for _ in 0..MAX_ATTEMPTS {
        run_as_owner(&database, "UPDATE people.outbox SET due_at = now()").await;
        dispatcher.deliver_due().await.expect("tries");
    }
    assert!(read::<bool>(&database, "SELECT keep FROM people.outbox").await);
}

/// A consumer whose inboxes the delivery role cannot clean is refused at start, rather than failing every hour:
/// without its use of the schema, its right to read or delete, or its policy on the tenant inbox.
#[tokio::test]
async fn a_consumer_without_inbox_delivery_is_refused() {
    let World { pool, database, .. } = world().await;
    let journal: Arc<dyn Consumer> = Arc::new(support::Journal {
        failures_left: AtomicUsize::new(0),
    });
    // Each right the cleanup uses, taken away in turn, and given back after.
    for (taken, back) in [
        (
            "REVOKE DELETE ON journal.inbox FROM tetsu_delivery",
            "GRANT DELETE ON journal.inbox TO tetsu_delivery",
        ),
        (
            "REVOKE SELECT ON journal.inbox_global FROM tetsu_delivery",
            "GRANT SELECT ON journal.inbox_global TO tetsu_delivery",
        ),
        (
            "REVOKE USAGE ON SCHEMA journal FROM tetsu_delivery",
            "GRANT USAGE ON SCHEMA journal TO tetsu_delivery",
        ),
        (
            "DROP POLICY delivery ON journal.inbox",
            "CREATE POLICY delivery ON journal.inbox TO tetsu_delivery USING (TRUE)",
        ),
    ] {
        run_as_owner(&database, taken).await;
        let refused = Dispatcher::new(pool.clone(), &["people"], vec![journal.clone()])
            .await
            .err()
            .map(|error| error.to_string());
        assert_eq!(
            refused.as_deref(),
            Some("journal consumes events, but its migrations lack INBOX_DELIVERY"),
            "{taken}"
        );
        run_as_owner(&database, back).await;
    }
    assert!(
        Dispatcher::new(pool, &["people"], vec![journal])
            .await
            .is_ok()
    );
}

/// The dispatcher's cleanup logs how many rows it removed, and says nothing when it removed none.
#[tokio::test]
async fn the_cleanup_logs_what_it_removed() {
    let World {
        pool,
        dispatcher,
        database,
        ..
    } = world().await;
    record(&pool, 1, &JOINED, true).await;
    dispatcher.deliver_due().await.expect("delivers");
    run_as_owner(
        &database,
        "UPDATE people.outbox SET delivered_at = now() - interval '8 days'",
    )
    .await;
    let logs = Logs::default();
    let _guard = logs.capture();
    // A stop already sent ends each run after one pass, which cleans up first.
    for _ in 0..2 {
        dispatcher.run(Duration::from_secs(600), async {}).await;
    }
    let removed: Vec<String> = logs
        .lines()
        .into_iter()
        .filter(|line| line.contains("removed event rows past their use"))
        .collect();
    assert_eq!(removed.len(), 1, "{removed:?}");
    for field in ["\"delivered\":1", "\"stopped\":0", "\"applied\":0", "INFO"] {
        assert!(removed[0].contains(field), "{field} in {}", removed[0]);
    }
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
async fn the_delivery_role_reaches_every_tenant_s_outbox_and_inbox_and_nothing_else() {
    let World {
        pool,
        dispatcher,
        database: _database,
        ..
    } = world().await;
    record(&pool, 1, &JOINED, true).await;
    record(&pool, 2, &JOINED, true).await;
    let mut client = pool.get().await.expect("a connection");
    assert_eq!(counted_as_delivery(&mut client, "people.outbox").await, 2);
    // It reads every tenant's inbox too, to remove the IDs past their use (ADR-0080).
    dispatcher.deliver_due().await.expect("delivers");
    assert_eq!(counted_as_delivery(&mut client, "ledger.inbox").await, 2);
    // It settles events and never rewrites one, never adds to or changes an inbox, and reaches no other table.
    for statement in [
        "UPDATE people.outbox SET payload = '{}'",
        "UPDATE people.outbox SET tenant_id = 3",
        "UPDATE people.outbox SET event_type = 'x'",
        "SELECT count(*) FROM people.people",
        "SELECT count(*) FROM people.notes",
        "INSERT INTO ledger.inbox_global (id) VALUES (gen_random_uuid())",
        "UPDATE ledger.inbox SET applied_at = now()",
        "SELECT count(*) FROM ledger.effects",
    ] {
        let transaction = DeliveryTransaction::begin(&mut client)
            .await
            .expect("begins");
        let refused = transaction
            .execute(statement, &[])
            .await
            .expect_err("refused");
        assert_eq!(
            refused.code(),
            Some(&tokio_postgres::error::SqlState::INSUFFICIENT_PRIVILEGE),
            "{statement}"
        );
    }
}

#[tokio::test]
async fn outboxes_and_inboxes_follow_the_isolation_rules() {
    let World { database, .. } = world().await;
    let mut client = connect(&database).await.expect("connects");
    assert_eq!(
        tetsu_postgres::isolation_violations(&mut client, &["people", "ledger", "journal"])
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

/// The wait stops growing at an hour, however many times a kept event has failed, rather than overflowing.
#[tokio::test]
async fn a_kept_event_that_fails_for_days_still_waits_an_hour() {
    let World {
        pool,
        ledger,
        dispatcher,
        database,
    } = world().await;
    ledger.failing.store(true, Ordering::SeqCst);
    record(
        &pool,
        1,
        &NewEvent {
            keep: true,
            ..JOINED
        },
        true,
    )
    .await;
    for attempts in [13, 44, 60, 1000] {
        run_as_owner(
            &database,
            &format!("UPDATE people.outbox SET attempts = {attempts}, due_at = now()"),
        )
        .await;
        dispatcher.deliver_due().await.expect("tries");
        let wait: f64 = read(
            &database,
            "SELECT extract(epoch FROM due_at - now())::float8 FROM people.outbox",
        )
        .await;
        assert!((3599.0..=3601.0).contains(&wait), "{attempts}: {wait}");
    }
}

/// A kept event never stops, but once it has failed as often as others may, it says so, so an alert can fire.
#[tokio::test]
async fn a_kept_event_that_keeps_failing_says_so_once() {
    let World {
        pool,
        ledger,
        dispatcher,
        database,
    } = world().await;
    ledger.failing.store(true, Ordering::SeqCst);
    let id = record(
        &pool,
        1,
        &NewEvent {
            keep: true,
            ..JOINED
        },
        true,
    )
    .await;
    let logs = Logs::default();
    let _guard = logs.capture();
    for _ in 0..MAX_ATTEMPTS + 3 {
        run_as_owner(&database, "UPDATE people.outbox SET due_at = now()").await;
        dispatcher.deliver_due().await.expect("tries");
    }
    let failing: Vec<String> = logs
        .lines()
        .into_iter()
        .filter(|line| line.contains("event keeps failing"))
        .collect();
    assert_eq!(failing.len(), 1, "{failing:?}");
    for field in [
        id.as_str(),
        "people.joined",
        "ledger",
        "ERROR",
        "\"alert\":\"event_stopped\"",
    ] {
        assert!(failing[0].contains(field), "{field} in {}", failing[0]);
    }
}

/// When the pool has no connection to spare, the pass waits for the next one rather than charging the event an
/// attempt its consumer never had.
#[tokio::test]
async fn a_busy_pool_costs_no_attempts() {
    let World {
        pool,
        dispatcher,
        database,
        ..
    } = world().await;
    record(&pool, 1, &JOINED, true).await;
    // Every connection but the one the dispatcher reads its outbox with.
    let mut held = Vec::new();
    for _ in 1..pool.status().max_size {
        held.push(pool.get().await.expect("a connection"));
    }
    let passed = dispatcher.deliver_due().await;
    drop(held);
    assert_eq!(passed.expect("a shortage ends the pass quietly"), 0);
    assert_eq!(
        read::<i64>(&database, "SELECT attempts FROM people.outbox").await,
        0
    );
    assert_eq!(dispatcher.deliver_due().await.expect("delivers"), 1);
}

/// A pass whose time is up stops before a later consumer, at no charge, so it never outlasts its limits by more
/// than one consumer's time, and the next pass finishes the event (ADR-0060).
#[tokio::test]
async fn a_pass_out_of_time_stops_before_a_later_consumer() {
    let World {
        pool,
        ledger,
        database,
        ..
    } = world().await;
    let journal = Arc::new(support::Journal {
        failures_left: AtomicUsize::new(0),
    });
    let consumers: Vec<Arc<dyn Consumer>> = vec![ledger.clone(), journal];
    let dispatcher = Dispatcher::new(pool.clone(), &["people"], consumers)
        .await
        .expect("a dispatcher");
    record(&pool, 1, &JOINED, true).await;
    let out_of_time = dispatcher.with_pass_limit(Duration::ZERO);
    assert_eq!(out_of_time.deliver_due().await.expect("passes"), 0);
    assert_eq!(
        ledger.calls.load(Ordering::SeqCst),
        1,
        "the first consumer ran"
    );
    assert_eq!(
        read::<i64>(&database, "SELECT count(*) FROM journal.inbox").await,
        0,
        "the later one did not"
    );
    assert_eq!(
        read::<i64>(&database, "SELECT attempts FROM people.outbox").await,
        0,
        "no attempt is charged"
    );
    let dispatcher = out_of_time.with_pass_limit(Duration::from_mins(1));
    assert_eq!(dispatcher.deliver_due().await.expect("delivers"), 1);
    assert_eq!(
        read::<i64>(&database, "SELECT count(*) FROM journal.inbox").await,
        1
    );
}

/// A consumer that never answers counts as a failure after its time, so delivery goes on.
#[tokio::test]
async fn a_consumer_that_never_answers_fails_after_its_time() {
    let World {
        pool,
        ledger,
        dispatcher,
        database,
    } = world().await;
    ledger.hanging.store(true, Ordering::SeqCst);
    record(&pool, 1, &JOINED, true).await;
    let dispatcher = dispatcher.with_apply_timeout(Duration::from_millis(200));
    let passed = tokio::time::timeout(Duration::from_secs(5), dispatcher.deliver_due())
        .await
        .expect("gives up on the consumer within its time");
    assert_eq!(passed.expect("passes"), 0);
    assert_eq!(
        read::<i64>(&database, "SELECT attempts FROM people.outbox").await,
        1
    );
}

/// A consumer that must not lose events, as Audit must not, keeps an event it fails on retrying past the limit.
#[tokio::test]
async fn an_event_a_never_lose_consumer_fails_keeps_retrying() {
    let World { pool, database, .. } = world().await;
    let journal = Arc::new(support::Journal {
        failures_left: AtomicUsize::new(usize::MAX),
    });
    let consumers: Vec<Arc<dyn Consumer>> = vec![journal];
    let dispatcher = Dispatcher::new(pool.clone(), &["people"], consumers)
        .await
        .expect("a dispatcher");
    record(&pool, 1, &JOINED, true).await;
    for _ in 0..MAX_ATTEMPTS + 2 {
        run_as_owner(&database, "UPDATE people.outbox SET due_at = now()").await;
        dispatcher.deliver_due().await.expect("tries");
    }
    assert_eq!(
        read::<i64>(&database, "SELECT attempts FROM people.outbox WHERE keep").await,
        MAX_ATTEMPTS + 2,
        "marked kept, and still tried"
    );
}

/// A never-lose consumer that failed once and then recovered does not keep an event another consumer goes on
/// failing, so that event still stops for a fix.
#[tokio::test]
async fn an_event_is_kept_only_for_a_never_lose_consumer_still_failing() {
    let World {
        pool,
        ledger,
        database,
        ..
    } = world().await;
    ledger.failing.store(true, Ordering::SeqCst);
    let journal = Arc::new(support::Journal {
        failures_left: AtomicUsize::new(1),
    });
    let consumers: Vec<Arc<dyn Consumer>> = vec![journal, ledger.clone()];
    let dispatcher = Dispatcher::new(pool.clone(), &["people"], consumers)
        .await
        .expect("a dispatcher");
    record(&pool, 1, &JOINED, true).await;
    for _ in 0..MAX_ATTEMPTS + 2 {
        run_as_owner(&database, "UPDATE people.outbox SET due_at = now()").await;
        dispatcher.deliver_due().await.expect("tries");
    }
    assert_eq!(
        read::<i64>(
            &database,
            "SELECT attempts FROM people.outbox WHERE NOT keep"
        )
        .await,
        MAX_ATTEMPTS,
        "stopped, not kept"
    );
}

/// An outbox that cannot be read is reported, and the other producers' events are still delivered.
#[tokio::test]
async fn one_broken_outbox_holds_up_no_other() {
    let World {
        pool,
        dispatcher,
        database,
        ..
    } = world().await;
    record(&pool, 1, &JOINED, true).await;
    // The ledger's outbox comes first, and the delivery role can no longer read it.
    run_as_owner(
        &database,
        "REVOKE SELECT ON ledger.outbox FROM tetsu_delivery",
    )
    .await;
    assert!(
        dispatcher.deliver_due().await.is_err(),
        "reports the broken outbox"
    );
    assert_eq!(
        read::<i64>(&database, "SELECT count(*) FROM ledger.effects").await,
        1,
        "delivers the people's event anyway"
    );
}

/// A consumer's database error is logged by its SQL state alone, since its text can quote a row's values.
#[tokio::test]
async fn a_consumer_s_database_error_is_logged_without_its_values() {
    let World {
        pool,
        ledger,
        dispatcher,
        database: _database,
    } = world().await;
    ledger.leaky.store(true, Ordering::SeqCst);
    record(&pool, 1, &JOINED, true).await;
    let logs = Logs::default();
    let _guard = logs.capture();
    dispatcher.deliver_due().await.expect("tries");
    let text = logs.lines().join("\n");
    assert!(text.contains("event not applied"), "{text}");
    assert!(text.contains("22P02"), "{text}");
    assert!(!text.contains("lee.park"), "{text}");
}

/// A pool with no connection to spare ends the whole pass, rather than each later outbox waiting for one too.
#[tokio::test]
async fn a_shortage_ends_the_whole_pass() {
    let World {
        pool,
        dispatcher,
        database,
        ..
    } = world().await;
    // Two in one outbox and one in the next, so stopping at the first shortage shows within an outbox and across.
    record(&pool, 1, &JOINED, true).await;
    record(&pool, 2, &JOINED, true).await;
    record_global(&pool, &JOINED).await;
    let logs = Logs::default();
    let _guard = logs.capture();
    let mut held = Vec::new();
    for _ in 1..pool.status().max_size {
        held.push(pool.get().await.expect("a connection"));
    }
    assert_eq!(dispatcher.deliver_due().await.expect("ends quietly"), 0);
    drop(held);
    let short = logs
        .lines()
        .into_iter()
        .filter(|line| line.contains("no connection for a consumer"))
        .count();
    assert_eq!(
        short,
        1,
        "stopped at the first shortage: {:?}",
        logs.lines()
    );
    assert_eq!(
        read::<i64>(
            &database,
            "SELECT ((SELECT sum(attempts) FROM people.outbox) + (SELECT sum(attempts) FROM people.outbox_global))::bigint"
        )
        .await,
        0
    );
}

/// A consumer that failed before the pool ran short is still charged for it, so its event can reach the limit.
#[tokio::test]
async fn a_failure_before_a_shortage_is_still_charged() {
    let World {
        pool,
        ledger,
        database,
        ..
    } = world().await;
    let held = Arc::new(std::sync::Mutex::new(Vec::new()));
    let hog = Arc::new(support::Hog {
        pool: pool.clone(),
        held: held.clone(),
    });
    let consumers: Vec<Arc<dyn Consumer>> = vec![hog, ledger];
    let dispatcher = Dispatcher::new(pool.clone(), &["people"], consumers)
        .await
        .expect("a dispatcher");
    record(&pool, 1, &JOINED, true).await;
    // Every connection but the dispatcher's own and the one the first consumer uses.
    let mut taken = Vec::new();
    for _ in 2..pool.status().max_size {
        taken.push(pool.get().await.expect("a connection"));
    }
    dispatcher.deliver_due().await.expect("ends quietly");
    drop(taken);
    held.lock().expect("unpoisoned").clear();
    assert_eq!(
        read::<i64>(
            &database,
            "SELECT attempts FROM people.outbox WHERE delivered_at IS NULL"
        )
        .await,
        1
    );
}

/// The delivery role's limits cover its longest pass, a minute of claiming and one consumer's 30 seconds (ADR-0060).
#[tokio::test]
async fn the_delivery_role_has_limits_that_cover_its_pass() {
    let World {
        pool,
        database: _database,
        ..
    } = world().await;
    let mut client = pool.get().await.expect("a connection");
    let transaction = DeliveryTransaction::begin(&mut client)
        .await
        .expect("begins");
    let limits = transaction
        .query_one(
            "SELECT current_setting('transaction_timeout'),
                    current_setting('idle_in_transaction_session_timeout')",
            &[],
        )
        .await
        .expect("reads the limits");
    assert_eq!(
        (
            limits.try_get::<_, String>(0).expect("text"),
            limits.try_get::<_, String>(1).expect("text")
        ),
        ("2min".into(), "2min".into())
    );
}

/// The outbox takes only a `module.fact` type and a JSON object of at most 64 KiB, in both its tables, since event
/// types are written in code and the database is their check (ADR-0060).
#[tokio::test]
async fn the_outbox_refuses_a_malformed_event() {
    let World {
        pool,
        database: _database,
        ..
    } = world().await;
    let big = format!(r#"{{"note": "{}"}}"#, "a".repeat(65_536));
    for (kind, payload) in [
        ("people.logged-in", JOINED.payload),
        ("joined", JOINED.payload),
        ("People.joined", JOINED.payload),
        ("people.joined", "[1, 2]"),
        ("people.joined", "\"text\""),
        ("people.joined", big.as_str()),
    ] {
        let event = NewEvent {
            kind,
            payload,
            ..JOINED
        };
        let mut client = pool.get().await.expect("a connection");
        let transaction = TenantTransaction::begin(&mut client, "people", 1)
            .await
            .expect("begins");
        let tenant = transaction.record(&event).await.expect_err(kind);
        drop(transaction);
        let transaction = ModuleTransaction::begin(&mut client, "people")
            .await
            .expect("begins");
        let global = transaction.record(&event).await.expect_err(kind);
        for refused in [tenant, global] {
            assert_eq!(
                refused.code(),
                Some(&tokio_postgres::error::SqlState::CHECK_VIOLATION),
                "{kind} {payload:.20}"
            );
        }
    }
}

/// A stopped event, of a business or of none, is made due again with its attempts cleared, and then delivered;
/// an event in neither outbox is reported as not found (ADR-0043).
#[tokio::test]
async fn a_stopped_event_is_replayed_from_either_outbox() {
    let World {
        pool,
        dispatcher,
        database,
        ..
    } = world().await;
    let tenant = record(&pool, 1, &JOINED, true).await;
    let global = record_global(&pool, &JOINED).await;
    run_as_owner(
        &database,
        &format!(
            "UPDATE people.outbox SET attempts = {MAX_ATTEMPTS}, due_at = 'infinity';
             UPDATE people.outbox_global SET attempts = {MAX_ATTEMPTS}, due_at = 'infinity'"
        ),
    )
    .await;
    assert_eq!(
        dispatcher.deliver_due().await.expect("passes"),
        0,
        "stopped"
    );
    assert_eq!(
        dispatcher.replay("people", &tenant).await.expect("replays"),
        Some("outbox")
    );
    assert_eq!(
        dispatcher.replay("people", &global).await.expect("replays"),
        Some("outbox_global")
    );
    assert_eq!(
        read::<i64>(
            &database,
            "SELECT ((SELECT sum(attempts) FROM people.outbox)
                  + (SELECT sum(attempts) FROM people.outbox_global))::bigint"
        )
        .await,
        0
    );
    assert_eq!(dispatcher.deliver_due().await.expect("delivers"), 2);
    assert_eq!(
        dispatcher
            .replay("people", "0190a3d2-0000-7000-8000-00000000ffff")
            .await
            .expect("looks"),
        None
    );
}

/// Only a producer's outboxes are reached, so no other name reaches the SQL, and an ID must be a UUID.
#[tokio::test]
async fn a_replay_names_a_producer_and_a_uuid() {
    let World {
        dispatcher,
        database: _database,
        ..
    } = world().await;
    for producer in ["ledger_x", "people\".outbox; --", "pg_catalog"] {
        let refused = dispatcher
            .replay(producer, "0190a3d2-0000-7000-8000-000000000001")
            .await
            .expect_err(producer);
        assert_eq!(
            refused.to_string(),
            format!("{producer:?} records no events here")
        );
    }
    assert!(dispatcher.replay("people", "not-a-uuid").await.is_err());
}
