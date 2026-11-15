//! What the operator command does to operators and sessions (ADR-0040, ADR-0059), in Postgres, signed in as the
//! application role: enrollment links, disabling an operator, ending sessions, and the audit events each records.

use std::fmt::Write;

use tetsu_identity::{Error, Identity, OperatorId, OperatorName, postgres_store};
use tetsu_postgres::test_support::{TestDatabase, fresh_database};
use tetsu_postgres::{ModuleTransaction, connect, migrate, pool};

#[cfg(test)]
mod support {
    use super::*;

    pub async fn ready() -> TestDatabase {
        let database = fresh_database().await.expect("a test database");
        migrate(&database, &[&Identity]).await.expect("migrates");
        database
    }

    pub fn name(name: &str) -> OperatorName {
        OperatorName::parse(name).expect("a name")
    }

    /// One number the database answers, read past row-level security as the test's own superuser.
    pub async fn number(database: &TestDatabase, sql: &str) -> i64 {
        connect(database)
            .await
            .expect("connects")
            .query_one(sql, &[])
            .await
            .expect("reads")
            .try_get(0)
            .expect("a number")
    }

    /// Runs SQL as the test's own superuser, to break one thing on purpose.
    pub async fn as_admin(database: &TestDatabase, sql: &str) {
        connect(database)
            .await
            .expect("connects")
            .batch_execute(sql)
            .await
            .expect("breaks it");
    }

    /// Every event Identity has recorded, oldest first, as type and payload.
    pub async fn events(database: &TestDatabase) -> Vec<(String, String)> {
        connect(database)
            .await
            .expect("connects")
            .query(
                "SELECT event_type, payload::text FROM identity.outbox_global ORDER BY occurred_at, id",
                &[],
            )
            .await
            .expect("reads")
            .iter()
            .map(|row| {
                (
                    row.try_get(0).expect("a type"),
                    row.try_get(1).expect("a payload"),
                )
            })
            .collect()
    }
}

use support::{as_admin, events, name, number, ready};

/// The link's token is stored only as its SHA-256 hash, is good for an hour, and a new one replaces the last; each
/// issue is recorded with the operator's ID and nothing of the token (ADR-0040, ADR-0047).
#[tokio::test]
async fn an_enrollment_link_is_stored_hashed_for_an_hour_and_replaced() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let first = operators.enroll(kurt).await.expect("issues");
    let second = operators.enroll(kurt).await.expect("issues again");
    assert_ne!(first.to_text(), second.to_text());
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.enrollments").await,
        1,
        "one open link per operator"
    );
    let stored = |token: &str| {
        format!(
            "SELECT count(*) FROM identity.enrollments
             WHERE token_hash = sha256(decode('{token}', 'hex'))
               AND ends_at - issued_at = interval '1 hour'"
        )
    };
    assert_eq!(number(&database, &stored(&second.to_text())).await, 1);
    assert_eq!(number(&database, &stored(&first.to_text())).await, 0);
    let id = kurt.get();
    assert_eq!(
        events(&database).await,
        [
            (
                "identity.operator_created".into(),
                format!(r#"{{"operator_id": {id}}}"#)
            ),
            (
                "identity.enrollment_issued".into(),
                format!(r#"{{"operator_id": {id}}}"#)
            ),
            (
                "identity.enrollment_issued".into(),
                format!(r#"{{"operator_id": {id}}}"#)
            ),
        ]
    );
}

/// Disabling ends every session and the open link in one step, records it once, and from then on the operator starts
/// no session and is issued no link (`v5.0.0-7.4.2`).
#[tokio::test]
async fn a_disabled_operator_is_signed_out_and_signs_in_nowhere() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    for _ in 0..2 {
        operators.start_session(kurt).await.expect("starts");
    }
    operators.enroll(kurt).await.expect("issues");
    assert_eq!(operators.disable(kurt).await, Ok(true));
    assert_eq!(operators.disable(kurt).await, Ok(false), "already disabled");
    for table in ["sessions", "enrollments"] {
        assert_eq!(
            number(&database, &format!("SELECT count(*) FROM identity.{table}")).await,
            0,
            "{table}"
        );
    }
    let refused = Err(Error::Invalid(format!(
        "no enabled operator {}",
        kurt.get()
    )));
    assert_eq!(operators.enroll(kurt).await.map(|_| ()), refused);
    assert_eq!(
        operators.start_session(kurt).await.map(|_| ()),
        Err(Error::Invalid("no enabled operator".into()))
    );
    let disabled: Vec<_> = events(&database)
        .await
        .into_iter()
        .filter(|(kind, _)| kind == "identity.operator_disabled")
        .collect();
    assert_eq!(disabled.len(), 1);
    let listed = operators.list().await.expect("lists");
    assert!(listed[0].disabled_at.is_some());
}

/// No link is issued to an operator who does not exist.
#[tokio::test]
async fn an_unknown_operator_is_issued_no_link() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let nobody = OperatorId::parse("42").expect("an ID");
    assert_eq!(
        operators.enroll(nobody).await.map(|_| ()),
        Err(Error::Invalid("no enabled operator 42".into()))
    );
    assert_eq!(operators.disable(nobody).await, Ok(false));
    assert_eq!(events(&database).await, Vec::new());
}

/// An administrator ends one operator's sessions or every one, and each is recorded with how many ended
/// (`v5.0.0-7.4.5`).
#[tokio::test]
async fn sessions_end_for_one_operator_or_all() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let lee = operators.create(&name("Lee Park")).await.expect("creates");
    for operator in [kurt, kurt, lee] {
        operators.start_session(operator).await.expect("starts");
    }
    assert_eq!(operators.end_sessions(kurt).await, Ok(2));
    assert_eq!(operators.end_sessions(kurt).await, Ok(0));
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.sessions").await,
        1,
        "Lee's remains"
    );
    assert_eq!(operators.end_all_sessions().await, Ok(1));
    let ended: Vec<String> = events(&database)
        .await
        .into_iter()
        .filter(|(kind, _)| kind == "identity.sessions_ended")
        .map(|(_, payload)| payload)
        .collect();
    let id = kurt.get();
    assert_eq!(
        ended,
        [
            format!(r#"{{"ended": 2, "realm": "operator", "operator_id": {id}}}"#),
            format!(r#"{{"ended": 0, "realm": "operator", "operator_id": {id}}}"#),
            r#"{"ended": 1, "realm": "operator"}"#.to_string(),
        ]
    );
}

/// Operators are listed oldest first, with whether each is disabled.
#[tokio::test]
async fn operators_are_listed_oldest_first() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let lee = operators.create(&name("Lee Park")).await.expect("creates");
    operators.disable(lee).await.expect("disables");
    let listed = operators.list().await.expect("lists");
    let shown: Vec<_> = listed
        .iter()
        .map(|operator| {
            (
                operator.id,
                operator.name.as_str(),
                operator.disabled_at.is_some(),
            )
        })
        .collect();
    assert_eq!(
        shown,
        [(kurt, "Kurt Medley", false), (lee, "Lee Park", true)]
    );
    assert!(listed[0].created_at <= listed[1].created_at);
}

/// A disable at the same moment as sign-ins never leaves a session behind: each start holds the operator's row, so
/// the disable waits for it and then ends what it started.
#[tokio::test]
async fn a_disable_racing_sign_ins_leaves_no_session() {
    let database = ready().await;
    let operators = std::sync::Arc::new(postgres_store(pool(&database.as_app()).expect("a pool")));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let starts: Vec<_> = (0..6)
        .map(|_| {
            let operators = operators.clone();
            tokio::spawn(async move { operators.start_session(kurt).await.map(|_| ()) })
        })
        .collect();
    assert_eq!(operators.disable(kurt).await, Ok(true));
    for start in starts {
        let started = start.await.expect("finishes");
        assert!(
            matches!(started, Ok(()) | Err(Error::Invalid(_))),
            "{started:?}"
        );
    }
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.sessions").await,
        0
    );
}

/// Identity's own role reaches enrollment links only through its functions, so no request can list them.
#[tokio::test]
async fn identitys_role_cannot_read_enrollment_links() {
    let database = ready().await;
    let pool = pool(&database.as_app()).expect("a pool");
    let mut client = pool.get().await.expect("a connection");
    let transaction = ModuleTransaction::begin(&mut client, "identity")
        .await
        .expect("begins");
    let refused = transaction
        .query("SELECT token_hash FROM identity.enrollments", &[])
        .await
        .expect_err("refused");
    assert_eq!(
        refused.code(),
        Some(&tokio_postgres::error::SqlState::INSUFFICIENT_PRIVILEGE)
    );
}

/// Each statement the operator command runs fails as storage when its function is out of reach, and so does a
/// change whose audit event cannot be recorded, which rolls the change back.
#[tokio::test]
async fn each_command_fails_as_storage_when_the_database_refuses_it() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    as_admin(
        &database,
        "REVOKE EXECUTE ON FUNCTION identity.issue_enrollment(bytea, bigint),
           identity.disable_operator(bigint), identity.end_operator_sessions(bigint, text),
           identity.end_realm_sessions(text) FROM tetsu_identity;
         REVOKE SELECT ON identity.operators FROM tetsu_identity;
         REVOKE INSERT ON identity.outbox_global FROM tetsu_identity",
    )
    .await;
    let storage = |result: Result<(), Error>| matches!(result, Err(Error::Storage(_)));
    assert!(storage(operators.enroll(kurt).await.map(drop)));
    assert!(storage(operators.disable(kurt).await.map(drop)));
    assert!(storage(operators.end_sessions(kurt).await.map(drop)));
    assert!(storage(operators.end_all_sessions().await.map(drop)));
    assert!(storage(operators.list().await.map(drop)));
    // Creating reads the new ID back, so the event's own refusal shows only once that is allowed again.
    as_admin(
        &database,
        "GRANT SELECT ON identity.operators TO tetsu_identity",
    )
    .await;
    assert!(storage(operators.create(&name("Lee Park")).await.map(drop)));
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.operators").await,
        1,
        "no operator without its event"
    );
}

/// A function that answers in the wrong type is a storage failure, never a guess.
#[tokio::test]
async fn an_answer_of_the_wrong_type_is_a_storage_failure() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let mut replaced = String::new();
    for (function, arguments) in [
        ("issue_enrollment", "bytea, bigint"),
        ("disable_operator", "bigint"),
        ("end_operator_sessions", "bigint, text"),
        ("end_realm_sessions", "text"),
    ] {
        write!(
            replaced,
            "DROP FUNCTION identity.{function}({arguments});
             CREATE FUNCTION identity.{function}({arguments}) RETURNS text LANGUAGE sql AS 'SELECT ''x''';
             GRANT EXECUTE ON FUNCTION identity.{function}({arguments}) TO tetsu_identity;"
        )
        .expect("writes");
    }
    as_admin(&database, &replaced).await;
    let storage = |result: Result<(), Error>| matches!(result, Err(Error::Storage(_)));
    assert!(storage(operators.enroll(kurt).await.map(drop)));
    assert!(storage(operators.disable(kurt).await.map(drop)));
    assert!(storage(operators.end_sessions(kurt).await.map(drop)));
    assert!(storage(operators.end_all_sessions().await.map(drop)));
}

/// A listed column of the wrong type is a storage failure, whichever column it is.
#[tokio::test]
async fn a_listed_column_of_the_wrong_type_is_a_storage_failure() {
    for columns in [
        "o.id::text AS id, o.name, o.created_at, o.disabled_at",
        "o.id, 1 AS name, o.created_at, o.disabled_at",
        "o.id, o.name, 'x'::text AS created_at, o.disabled_at",
        "o.id, o.name, o.created_at, 1 AS disabled_at",
    ] {
        let database = ready().await;
        let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
        operators
            .create(&name("Kurt Medley"))
            .await
            .expect("creates");
        as_admin(
            &database,
            &format!(
                "ALTER TABLE identity.operators RENAME TO operators_real;
                 CREATE VIEW identity.operators AS SELECT {columns} FROM identity.operators_real AS o;
                 GRANT SELECT ON identity.operators TO tetsu_identity"
            ),
        )
        .await;
        assert!(
            matches!(operators.list().await, Err(Error::Storage(_))),
            "{columns}"
        );
    }
}
