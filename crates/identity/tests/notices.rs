//! Identity's notices (ADR-0074): a login or an added passkey that went through is emailed to the admin, once,
//! through the dispatcher, and a notice that could not go just now is tried again.

use support::{ready, record};
use tetsu_kernel::{Delivery, MailError, Message};
use tetsu_postgres::connect;

#[cfg(test)]
mod support {
    use std::sync::{Arc, Mutex};

    use tetsu_identity::{Actor, Identity, PersonName, notices, postgres_store};
    use tetsu_kernel::{Address, Delivery, MailError, Mailer, Message};
    use tetsu_postgres::test_support::{TestDatabase, migrated_database};
    use tetsu_postgres::{Dispatcher, ModuleTransaction, NewEvent, Pool, pool};

    /// A mailer that keeps what it is handed and answers as the test sets.
    pub struct Recorder {
        sent: Mutex<Vec<Message>>,
        answer: Mutex<Result<Delivery, MailError>>,
    }

    impl Recorder {
        pub fn sent(&self) -> Vec<Message> {
            self.sent.lock().expect("a lock").clone()
        }

        pub fn answer(&self, answer: Result<Delivery, MailError>) {
            *self.answer.lock().expect("a lock") = answer;
        }
    }

    impl Mailer for Recorder {
        fn send(
            &self,
            message: &Message,
        ) -> impl Future<Output = Result<Delivery, MailError>> + Send {
            self.sent.lock().expect("a lock").push(message.clone());
            std::future::ready(*self.answer.lock().expect("a lock"))
        }
    }

    /// Admin 1 with an address and admin 2 without, a dispatcher handing Identity's events to the notices,
    /// and the mailer they use, with the events of making the admins already delivered.
    pub async fn ready() -> (TestDatabase, Pool, Dispatcher, Arc<Recorder>) {
        let database = migrated_database(&[&Identity]).await.expect("a database");
        let pool = pool(&database.as_app()).expect("a pool");
        let admins = postgres_store(pool.clone());
        let actor = Actor::parse("kurt@workstation").expect("an actor");
        let ada = Address::parse("ada@example.com").expect("an address");
        for (name, email) in [("Ada Lovelace", Some(&ada)), ("Lee Park", None)] {
            let name = PersonName::parse(name).expect("a name");
            admins.create(&name, email, &actor).await.expect("created");
        }
        let recorder = Arc::new(Recorder {
            sent: Mutex::new(Vec::new()),
            answer: Mutex::new(Ok(Delivery::Sent)),
        });
        let dispatcher =
            Dispatcher::new(pool.clone(), &["identity"], vec![notices(recorder.clone())])
                .await
                .expect("a dispatcher");
        dispatcher
            .deliver_due()
            .await
            .expect("delivers the creations");
        (database, pool, dispatcher, recorder)
    }

    /// Records an event as Identity records one, and returns its ID.
    pub async fn record(pool: &Pool, kind: &str, payload: &str) -> String {
        let mut client = pool.get().await.expect("a connection");
        let transaction = ModuleTransaction::begin(&mut client, "identity")
            .await
            .expect("begins");
        let id = transaction
            .record(&NewEvent {
                kind,
                version: 1,
                payload,
                keep: false,
            })
            .await
            .expect("recorded");
        transaction.commit().await.expect("committed");
        id
    }
}

#[tokio::test]
async fn a_login_a_passkey_and_a_join_are_each_told_once() {
    let (_database, pool, dispatcher, recorder) = ready().await;
    let login = record(
        &pool,
        "identity.logged_in",
        r#"{"admin_id": 1, "outcome": "changed", "actor": "admin-1@Firefox on Linux"}"#,
    )
    .await;
    record(
        &pool,
        "identity.passkey_added",
        r#"{"admin_id": 1, "outcome": "changed"}"#,
    )
    .await;
    record(
        &pool,
        "identity.invite_redeemed",
        r#"{"admin_id": 1, "outcome": "changed"}"#,
    )
    .await;
    dispatcher.deliver_due().await.unwrap();
    dispatcher.deliver_due().await.unwrap();
    let sent = recorder.sent();
    assert_eq!(sent.len(), 3, "a login, a passkey added, and a join");
    let subjects: Vec<&str> = sent.iter().map(Message::subject).collect();
    assert!(
        subjects.contains(&"New login to your Tetsu account"),
        "{subjects:?}"
    );
    assert!(
        subjects.contains(&"A passkey was added to your Tetsu account"),
        "{subjects:?}"
    );
    assert!(
        sent.iter()
            .all(|message| message.to().as_str() == "ada@example.com")
    );
    assert!(
        sent.iter()
            .any(|message| message.key() == format!("notice:{login}"))
    );
    let told = sent
        .iter()
        .find(|message| message.key() == format!("notice:{login}"))
        .unwrap();
    assert!(told.text().contains(" UTC, from Firefox on Linux."));
}

/// A staff member's login or new passkey is told to them, at their own address, as an admin's is.
#[tokio::test]
async fn a_staff_members_login_and_passkey_are_told_to_them() {
    let (database, pool, dispatcher, recorder) = ready().await;
    connect(&database)
        .await
        .unwrap()
        .batch_execute(
            "INSERT INTO identity.staff (name, email) VALUES ('Bo Lim', 'bo@example.com')",
        )
        .await
        .unwrap();
    let staff: i64 = connect(&database)
        .await
        .unwrap()
        .query_one("SELECT id FROM identity.staff", &[])
        .await
        .unwrap()
        .try_get(0)
        .unwrap();
    record(
        &pool,
        "identity.logged_in",
        &format!(r#"{{"staff_id": {staff}, "outcome": "changed", "actor": "staff-{staff}@Safari on iOS"}}"#),
    )
    .await;
    record(
        &pool,
        "identity.passkey_added",
        &format!(r#"{{"staff_id": {staff}, "outcome": "changed"}}"#),
    )
    .await;
    dispatcher.deliver_due().await.unwrap();
    let sent = recorder.sent();
    assert_eq!(sent.len(), 2, "{sent:?}");
    assert!(
        sent.iter()
            .all(|message| message.to().as_str() == "bo@example.com")
    );
    assert!(
        sent.iter()
            .any(|message| message.text().contains(" UTC, from Safari on iOS."))
    );
}

#[tokio::test]
async fn a_refused_login_or_an_admin_without_an_address_is_told_nothing() {
    let (database, pool, dispatcher, recorder) = ready().await;
    record(
        &pool,
        "identity.logged_in",
        r#"{"admin_id": 1, "outcome": "refused"}"#,
    )
    .await;
    record(
        &pool,
        "identity.logged_in",
        r#"{"admin_id": 2, "outcome": "changed"}"#,
    )
    .await;
    record(
        &pool,
        "identity.logged_in",
        r#"{"admin_id": 9, "outcome": "changed"}"#,
    )
    .await;
    record(
        &pool,
        "identity.admin_created",
        r#"{"admin_id": 1, "outcome": "changed"}"#,
    )
    .await;
    assert_eq!(dispatcher.deliver_due().await.unwrap(), 4);
    assert_eq!(recorder.sent(), Vec::<Message>::new());
    // The notices take only the logins: neither the creation here nor the two in setup.
    let applied: i64 = connect(&database)
        .await
        .expect("connects")
        .query_one("SELECT count(*) FROM identity.inbox_global", &[])
        .await
        .expect("counts")
        .try_get(0)
        .expect("a count");
    assert_eq!(applied, 3);
}

#[tokio::test]
async fn a_notice_that_could_not_go_just_now_is_tried_again() {
    let (_database, pool, dispatcher, recorder) = ready().await;
    recorder.answer(Err(MailError::Unavailable));
    let login = record(
        &pool,
        "identity.logged_in",
        r#"{"admin_id": 1, "outcome": "changed"}"#,
    )
    .await;
    assert_eq!(dispatcher.deliver_due().await.unwrap(), 0);
    recorder.answer(Ok(Delivery::Sent));
    dispatcher.replay("identity", &login).await.unwrap();
    assert_eq!(dispatcher.deliver_due().await.unwrap(), 1);
    assert_eq!(recorder.sent().len(), 2);
}

#[tokio::test]
async fn an_address_that_cannot_be_looked_up_leaves_the_notice_to_retry() {
    let (database, pool, dispatcher, recorder) = ready().await;
    connect(&database)
        .await
        .unwrap()
        .batch_execute("REVOKE SELECT (email) ON identity.admins FROM tetsu_identity")
        .await
        .unwrap();
    record(
        &pool,
        "identity.logged_in",
        r#"{"admin_id": 1, "outcome": "changed"}"#,
    )
    .await;
    assert_eq!(dispatcher.deliver_due().await.unwrap(), 0);
    assert_eq!(recorder.sent(), Vec::<Message>::new());
}

/// An address someone wrote into the table by hand that is not plain gets no notice, and the event is done.
#[tokio::test]
async fn an_address_that_is_not_plain_is_told_nothing() {
    let (database, pool, dispatcher, recorder) = ready().await;
    connect(&database)
        .await
        .unwrap()
        .batch_execute("UPDATE identity.admins SET email = 'admin@localhost' WHERE id = 1")
        .await
        .unwrap();
    record(
        &pool,
        "identity.logged_in",
        r#"{"admin_id": 1, "outcome": "changed"}"#,
    )
    .await;
    assert_eq!(dispatcher.deliver_due().await.unwrap(), 1);
    assert_eq!(recorder.sent(), Vec::<Message>::new());
}

/// An address that reads as the wrong type is a fault, so the notice is tried again rather than dropped.
#[tokio::test]
async fn an_address_that_cannot_be_read_leaves_the_notice_to_retry() {
    let (database, pool, dispatcher, recorder) = ready().await;
    connect(&database)
        .await
        .unwrap()
        .batch_execute(
            "ALTER TABLE identity.admins DROP CONSTRAINT admins_email_check;
             ALTER TABLE identity.admins ALTER COLUMN email TYPE bytea USING convert_to(email, 'UTF8')",
        )
        .await
        .unwrap();
    record(
        &pool,
        "identity.logged_in",
        r#"{"admin_id": 1, "outcome": "changed"}"#,
    )
    .await;
    assert_eq!(dispatcher.deliver_due().await.unwrap(), 0);
    assert_eq!(recorder.sent(), Vec::<Message>::new());
}

/// A security notice is worth sending late, so it keeps retrying hourly rather than giving up.
#[tokio::test]
async fn a_notice_is_never_lost() {
    let (_database, _pool, _dispatcher, recorder) = ready().await;
    assert!(tetsu_identity::notices(recorder).never_lose());
}

/// Only Identity's own logins, added passkeys, and joins are told; another module's event of the same kind is not
/// Identity's to tell.
#[tokio::test]
async fn the_notices_want_only_identitys_own_events() {
    let (_database, _pool, _dispatcher, recorder) = ready().await;
    let notices = tetsu_identity::notices(recorder);
    let event = |producer: &str, kind: &str| tetsu_postgres::Event {
        id: "e-1".into(),
        producer: producer.into(),
        tenant: None,
        kind: kind.into(),
        version: 1,
        occurred_at: std::time::SystemTime::UNIX_EPOCH,
        payload: "{}".into(),
    };
    assert!(notices.wants(&event("identity", "identity.logged_in")));
    assert!(!notices.wants(&event("tenancy", "identity.logged_in")));
    assert!(!notices.wants(&event("identity", "identity.admin_created")));
}
