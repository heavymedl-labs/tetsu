//! What the operator command does to operators and sessions (ADR-0040, ADR-0059), in Postgres, logged in as the
//! application role: invite links, disabling an operator, ending sessions, and the audit events each records.
//! Their shared setup is in `common`.

use std::fmt::Write;

use serde_json::{Value, json};

use tetsu_identity::{Disabling, Error, OperatorId, Removal, postgres_store};
use tetsu_postgres::test_support::TestDatabase;
use tetsu_postgres::{ModuleTransaction, Pool, connect, pool};

#[cfg(test)]
mod support {
    use super::*;

    /// A migrated database holding Kurt Medley, operator 1, and a pool over it as the application role.
    pub async fn kurt_and_pool() -> (TestDatabase, Pool) {
        let database = ready().await;
        let pool = pool(&database.as_app()).expect("a pool");
        postgres_store(pool.clone())
            .create(&name("Kurt Medley"), None, &actor())
            .await
            .expect("creates");
        (database, pool)
    }

    /// What a disable reports.
    pub fn disabled(done: bool, sessions_ended: u64, links_ended: u64) -> Disabling {
        Disabling {
            done,
            sessions_ended,
            links_ended,
        }
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

    /// Returns once some statement in the database is waiting on a lock, as a disable does on a row a login
    /// holds.
    pub async fn waits_for_a_lock(database: &TestDatabase) {
        let client = connect(database).await.expect("connects");
        for _ in 0..500 {
            let waiting: i64 = client
                .query_one(
                    "SELECT count(*) FROM pg_stat_activity
                     WHERE datname = current_database() AND wait_event_type = 'Lock'",
                    &[],
                )
                .await
                .expect("reads")
                .try_get(0)
                .expect("a count");
            if waiting > 0 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("nothing waited on a lock");
    }

    /// SQL that stores a passkey for an operator, as join will, with a credential ID of 16 bytes made from
    /// the operator's ID.
    pub fn passkey_for(operator: i64) -> String {
        format!(
            "INSERT INTO identity.passkeys (credential_id, operator_id, passkey, created_at)
             VALUES (sha256(int8send({operator}))::bytea, {operator}, '{{}}', now());"
        )
    }

    /// Every event Identity has recorded, oldest first, as type and payload.
    pub async fn events(database: &TestDatabase) -> Vec<(String, Value)> {
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
                let payload: String = row.try_get(1).expect("a payload");
                (
                    row.try_get(0).expect("a type"),
                    serde_json::from_str(&payload).expect("JSON"),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod common;

use common::{actor, logged_in, migrated, name, ready, with_kurt};
use support::{as_admin, disabled, events, kurt_and_pool, number, passkey_for, waits_for_a_lock};

/// The link's token is stored only as its SHA-256 hash, is good for an hour, and a new one replaces the last; each
/// issue is recorded with the operator's ID and nothing of the token (ADR-0040, ADR-0047).
#[tokio::test]
async fn an_invite_link_is_stored_hashed_for_an_hour_and_replaced() {
    let (database, operators, kurt) = with_kurt().await;
    let first = operators.invite(kurt, &actor()).await.expect("issues");
    let second = operators
        .invite(kurt, &actor())
        .await
        .expect("issues again");
    assert_ne!(first.to_text(), second.to_text());
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.invites").await,
        1,
        "one open link per operator"
    );
    let stored = |token: &str| {
        format!(
            "SELECT count(*) FROM identity.invites
             WHERE token_hash = sha256(decode('{token}', 'hex'))
               AND ends_at - issued_at = interval '1 hour'"
        )
    };
    assert_eq!(number(&database, &stored(&second.to_text())).await, 1);
    assert_eq!(number(&database, &stored(&first.to_text())).await, 0);
    let id = kurt.get();
    let changed = json!({"operator_id": id, "actor": "kurt@workstation", "outcome": "changed"});
    let issued = json!({"operator_id": id, "actor": "kurt@workstation", "outcome": "changed",
                        "reason": null});
    assert_eq!(
        events(&database).await,
        [
            ("identity.operator_created".into(), changed),
            ("identity.invite_issued".into(), issued.clone()),
            ("identity.invite_issued".into(), issued),
        ]
    );
}

/// Disabling ends every session and the open link in one step, records it once, and from then on the operator starts
/// no session and is issued no link (`v5.0.0-7.4.2`).
#[tokio::test]
async fn a_disabled_operator_is_logged_out_and_logs_in_nowhere() {
    let (database, operators, kurt) = with_kurt().await;
    for _ in 0..2 {
        logged_in(&database, kurt).await;
    }
    operators.invite(kurt, &actor()).await.expect("issues");
    assert_eq!(
        operators.disable(kurt, &actor()).await,
        Ok(disabled(true, 2, 1))
    );
    assert_eq!(
        operators.disable(kurt, &actor()).await,
        Ok(disabled(false, 0, 0)),
        "already disabled"
    );
    for table in ["sessions", "invites"] {
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
    assert_eq!(operators.invite(kurt, &actor()).await.map(|_| ()), refused);
    let disabled: Vec<_> = events(&database)
        .await
        .into_iter()
        .filter(|(kind, _)| kind == "identity.operator_disabled")
        .map(|(_, payload)| payload["outcome"].clone())
        .collect();
    assert_eq!(disabled, [json!("changed"), json!("unchanged")]);
    let listed = operators.list().await.expect("lists");
    assert!(listed[0].disabled_at.is_some());
}

/// No command acts on an operator who does not exist: each says so, and is recorded as refused, never as a no-op
/// (`v5.0.0-16.3.3`).
#[tokio::test]
async fn an_unknown_operator_is_issued_no_link() {
    let (database, operators) = migrated().await;
    let nobody = OperatorId::parse("42").expect("an ID");
    assert_eq!(
        operators.invite(nobody, &actor()).await.map(|_| ()),
        Err(Error::Invalid("no operator 42".into()))
    );
    let no_operator = Err(Error::Invalid("no operator 42".into()));
    assert_eq!(
        operators.disable(nobody, &actor()).await.map(drop),
        no_operator
    );
    assert_eq!(
        operators.enable(nobody, &actor()).await.map(drop),
        no_operator
    );
    assert_eq!(
        operators.end_sessions(nobody, &actor()).await.map(drop),
        no_operator
    );
    let refused = json!({"operator_id": 42, "actor": "kurt@workstation", "outcome": "refused"});
    let ended = json!({"operator_id": 42, "realm": "operator", "ended": null,
                       "actor": "kurt@workstation", "outcome": "refused"});
    let unissued = json!({"operator_id": 42, "actor": "kurt@workstation", "outcome": "refused",
                          "reason": "unknown"});
    assert_eq!(
        events(&database).await,
        [
            ("identity.invite_issued".into(), unissued),
            ("identity.operator_disabled".into(), refused.clone()),
            ("identity.operator_enabled".into(), refused),
            ("identity.sessions_ended".into(), ended),
        ]
    );
}

/// An administrator ends one operator's sessions or every one, and each is recorded with how many ended
/// (`v5.0.0-7.4.5`).
#[tokio::test]
async fn sessions_end_for_one_operator_or_all() {
    let (database, operators, kurt) = with_kurt().await;
    let lee = operators
        .create(&name("Lee Park"), None, &actor())
        .await
        .expect("creates");
    for operator in [kurt, kurt, lee] {
        logged_in(&database, operator).await;
    }
    assert_eq!(operators.end_sessions(kurt, &actor()).await, Ok(2));
    assert_eq!(operators.end_sessions(kurt, &actor()).await, Ok(0));
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.sessions").await,
        1,
        "Lee's remains"
    );
    assert_eq!(operators.end_all_sessions(&actor()).await, Ok(1));
    assert_eq!(operators.end_all_sessions(&actor()).await, Ok(0));
    let ended: Vec<Value> = events(&database)
        .await
        .into_iter()
        .filter(|(kind, _)| kind == "identity.sessions_ended")
        .map(|(_, payload)| payload)
        .collect();
    let (id, by) = (kurt.get(), "kurt@workstation");
    assert_eq!(
        ended,
        [
            json!({"operator_id": id, "realm": "operator", "ended": 2, "actor": by, "outcome": "changed"}),
            json!({"operator_id": id, "realm": "operator", "ended": 0, "actor": by, "outcome": "unchanged"}),
            json!({"realm": "operator", "ended": 1, "actor": by, "outcome": "changed"}),
            json!({"realm": "operator", "ended": 0, "actor": by, "outcome": "unchanged"}),
        ]
    );
}

/// Operators are listed oldest first, with whether each is disabled.
#[tokio::test]
async fn operators_are_listed_oldest_first() {
    let (_database, operators, kurt) = with_kurt().await;
    let lee = operators
        .create(&name("Lee Park"), None, &actor())
        .await
        .expect("creates");
    operators.disable(lee, &actor()).await.expect("disables");
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

/// A disable that starts while a login or a link is still being written waits for it, then ends what it wrote:
/// it never leaves a session or a link behind (`v5.0.0-7.4.2`). The writer holds its transaction open until
/// Postgres shows the disable waiting on the operator's row, so the order is the same on every run.
#[tokio::test]
async fn a_disable_waits_for_a_login_or_link_being_written_then_ends_it() {
    for (table, writes) in [
        (
            "sessions",
            "SELECT identity.start_session(sha256('a'::bytea), 'operator', $1, true)",
        ),
        (
            "invites",
            "SELECT identity.issue_invite(sha256('a'::bytea), $1)",
        ),
    ] {
        let database = ready().await;
        let pool = pool(&database.as_app()).expect("a pool");
        let operators = postgres_store(pool.clone());
        let kurt = operators
            .create(&name("Kurt Medley"), None, &actor())
            .await
            .expect("creates");
        // Only a login starts a session, so the administrator stands in for one.
        let mut admin = connect(&database).await.expect("connects");
        let writing = admin.transaction().await.expect("begins");
        writing
            .execute(writes, &[&kurt.get()])
            .await
            .expect("writes");
        let disabling = tokio::spawn(async move { operators.disable(kurt, &actor()).await });
        waits_for_a_lock(&database).await;
        writing.commit().await.expect("commits");
        assert!(
            disabling
                .await
                .expect("finishes")
                .is_ok_and(|done| done.done),
            "{table}"
        );
        assert_eq!(
            number(&database, &format!("SELECT count(*) FROM identity.{table}")).await,
            0,
            "{table}"
        );
    }
}

/// Ending an operator's sessions while a login is still being written waits for it, then ends it too.
#[tokio::test]
async fn ending_sessions_waits_for_a_login_being_written() {
    for all in [false, true] {
        let database = ready().await;
        let pool = pool(&database.as_app()).expect("a pool");
        let operators = postgres_store(pool.clone());
        let kurt = operators
            .create(&name("Kurt Medley"), None, &actor())
            .await
            .expect("creates");
        // Only a login starts a session, so the administrator stands in for one.
        let mut admin = connect(&database).await.expect("connects");
        let writing = admin.transaction().await.expect("begins");
        writing
            .execute(
                "SELECT identity.start_session(sha256('a'::bytea), 'operator', $1, true)",
                &[&kurt.get()],
            )
            .await
            .expect("writes");
        let ending = tokio::spawn(async move {
            if all {
                operators.end_all_sessions(&actor()).await
            } else {
                operators.end_sessions(kurt, &actor()).await
            }
        });
        waits_for_a_lock(&database).await;
        writing.commit().await.expect("commits");
        assert_eq!(ending.await.expect("finishes"), Ok(1), "all: {all}");
        assert_eq!(
            number(&database, "SELECT count(*) FROM identity.sessions").await,
            0
        );
    }
}

/// An operator enabled again logs in afresh: their sessions stayed ended, and each change is recorded.
#[tokio::test]
async fn an_operator_enabled_again_logs_in_afresh() {
    let (database, operators, kurt) = with_kurt().await;
    logged_in(&database, kurt).await;
    assert_eq!(
        operators.enable(kurt, &actor()).await,
        Ok(false),
        "never disabled"
    );
    operators.disable(kurt, &actor()).await.expect("disables");
    assert_eq!(operators.enable(kurt, &actor()).await, Ok(true));
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.sessions").await,
        0,
        "sessions stay ended"
    );
    logged_in(&database, kurt).await;
    operators
        .invite(kurt, &actor())
        .await
        .expect("may join again");
    let enabled: Vec<_> = events(&database)
        .await
        .into_iter()
        .filter(|(kind, _)| kind == "identity.operator_enabled")
        .map(|(_, payload)| payload["outcome"].clone())
        .collect();
    assert_eq!(enabled, [json!("unchanged"), json!("changed")]);
}

/// Two links issued at the same moment both succeed, and the operator holds one: whichever was stored last.
#[tokio::test]
async fn links_issued_at_once_leave_one() {
    let (database, operators, kurt) = with_kurt().await;
    let by = actor();
    let (first, second) = tokio::join!(operators.invite(kurt, &by), operators.invite(kurt, &by));
    let held = |token: &str| {
        format!(
            "SELECT count(*) FROM identity.invites WHERE token_hash = sha256(decode('{token}', 'hex'))"
        )
    };
    let first = number(&database, &held(&first.expect("issues").to_text())).await;
    let second = number(&database, &held(&second.expect("issues").to_text())).await;
    assert_eq!(first + second, 1);
}

/// Identity's own role reaches invite links only through its functions, so no request can list them.
#[tokio::test]
async fn identitys_role_cannot_read_invite_links() {
    let database = ready().await;
    let pool = pool(&database.as_app()).expect("a pool");
    let mut client = pool.get().await.expect("a connection");
    let transaction = ModuleTransaction::begin(&mut client, "identity")
        .await
        .expect("begins");
    let refused = transaction
        .query("SELECT token_hash FROM identity.invites", &[])
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
    let (database, operators, kurt) = with_kurt().await;
    as_admin(
        &database,
        "REVOKE EXECUTE ON FUNCTION identity.issue_invite(bytea, bigint),
           identity.disable_operator(bigint), identity.enable_operator(bigint),
           identity.remove_passkeys(bigint), identity.end_sessions(text, bigint),
           identity.list_operators() FROM tetsu_identity;
         REVOKE INSERT ON identity.outbox_global FROM tetsu_identity",
    )
    .await;
    let storage = |result: Result<(), Error>| matches!(result, Err(Error::Storage(_)));
    assert!(storage(operators.invite(kurt, &actor()).await.map(drop)));
    assert!(storage(operators.disable(kurt, &actor()).await.map(drop)));
    assert!(storage(operators.enable(kurt, &actor()).await.map(drop)));
    assert!(storage(
        operators.remove_passkeys(kurt, &actor()).await.map(drop)
    ));
    assert!(storage(
        operators.end_sessions(kurt, &actor()).await.map(drop)
    ));
    assert!(storage(
        operators.end_all_sessions(&actor()).await.map(drop)
    ));
    assert!(storage(operators.list().await.map(drop)));
    // Creating reads the new ID back, so the event's own refusal shows only once that is allowed again.
    as_admin(
        &database,
        "GRANT SELECT ON identity.operators TO tetsu_identity",
    )
    .await;
    assert!(storage(
        operators
            .create(&name("Lee Park"), None, &actor())
            .await
            .map(drop)
    ));
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.operators").await,
        1,
        "no operator without its event"
    );
}

/// A function that answers in the wrong type is a storage failure, never a guess.
#[tokio::test]
async fn an_answer_of_the_wrong_type_is_a_storage_failure() {
    let (database, operators, kurt) = with_kurt().await;
    let mut replaced = String::new();
    for (function, arguments) in [
        ("issue_invite", "bytea, bigint"),
        ("disable_operator", "bigint"),
        ("enable_operator", "bigint"),
        ("end_sessions", "text, bigint"),
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
    assert!(storage(operators.invite(kurt, &actor()).await.map(drop)));
    assert!(storage(operators.disable(kurt, &actor()).await.map(drop)));
    assert!(storage(operators.enable(kurt, &actor()).await.map(drop)));
    assert!(storage(
        operators.end_sessions(kurt, &actor()).await.map(drop)
    ));
    assert!(storage(
        operators.end_all_sessions(&actor()).await.map(drop)
    ));
}

/// A listed column of the wrong type is a storage failure, whichever column it is. The list's function is replaced
/// outright, so no operator need exist for it to answer.
#[tokio::test]
async fn a_listed_column_of_the_wrong_type_is_a_storage_failure() {
    let right = [
        ("id", "bigint", "1"),
        ("name", "text", "'Kurt Medley'"),
        ("created_at", "timestamptz", "now()"),
        ("disabled_at", "timestamptz", "null"),
        ("passkeys", "bigint", "0"),
        ("link_minutes", "bigint", "null"),
    ];
    for wrong in 0..right.len() {
        let (database, operators) = migrated().await;
        let columns: Vec<String> = right
            .iter()
            .enumerate()
            .map(|(at, (name, kind, _))| {
                format!("{name} {}", if at == wrong { "integer" } else { kind })
            })
            .collect();
        let values: Vec<String> = right
            .iter()
            .enumerate()
            .map(|(at, (_, kind, value))| {
                if at == wrong {
                    "1".to_string()
                } else {
                    format!("{value}::{kind}")
                }
            })
            .collect();
        as_admin(
            &database,
            &format!(
                "DROP FUNCTION identity.list_operators();
                 CREATE FUNCTION identity.list_operators() RETURNS TABLE ({})
                 LANGUAGE sql AS 'SELECT {}';
                 GRANT EXECUTE ON FUNCTION identity.list_operators() TO tetsu_identity",
                columns.join(", "),
                values.join(", ").replace('\'', "''")
            ),
        )
        .await;
        assert!(
            matches!(operators.list().await, Err(Error::Storage(_))),
            "{}",
            right[wrong].0
        );
    }
}

/// A disable whose answer has a column of the wrong type is a storage failure, whichever column it is.
#[tokio::test]
async fn a_disable_answer_of_the_wrong_type_is_a_storage_failure() {
    for (columns, answer) in [
        (
            "done text, sessions_ended bigint, links_ended bigint",
            "SELECT ''yes'', 0::bigint, 0::bigint",
        ),
        (
            "done boolean, sessions_ended text, links_ended bigint",
            "SELECT true, ''none'', 0::bigint",
        ),
        (
            "done boolean, sessions_ended bigint, links_ended text",
            "SELECT true, 0::bigint, ''none''",
        ),
    ] {
        let (database, operators, kurt) = with_kurt().await;
        as_admin(
            &database,
            &format!(
                "DROP FUNCTION identity.disable_operator(bigint);
                 CREATE FUNCTION identity.disable_operator(bigint) RETURNS TABLE ({columns})
                 LANGUAGE sql AS '{answer}';
                 GRANT EXECUTE ON FUNCTION identity.disable_operator(bigint) TO tetsu_identity"
            ),
        )
        .await;
        assert!(
            matches!(
                operators.disable(kurt, &actor()).await,
                Err(Error::Storage(_))
            ),
            "{columns}"
        );
    }
}

/// Disabling, enabling, ending sessions, issuing a link, beginning or finishing a join, logging in, and
/// beginning or finishing an addition refuse to run under repeatable read, where a row committed while they waited
/// would escape their checks, whoever starts the transaction; so does beginning a login, whose sweep would fail
/// there when begins overlap.
#[tokio::test]
async fn changes_that_wait_on_an_operator_need_read_committed() {
    let (_database, pool) = kurt_and_pool().await;
    let mut client = pool.get().await.expect("a connection");
    for call in [
        "SELECT * FROM identity.disable_operator(1)",
        "SELECT identity.end_sessions('operator', 1)",
        "SELECT identity.end_sessions('operator', null)",
        "SELECT identity.issue_invite(sha256('a'::bytea), 1)",
        "SELECT identity.redeem_invite(sha256('a'::bytea), 1, sha256('c'::bytea), '{}', '', sha256('s'::bytea), null)",
        "SELECT identity.begin_join(sha256('t'::bytea), 1, sha256('a'::bytea), '{}')",
        "SELECT identity.enable_operator(1)",
        "SELECT * FROM identity.remove_passkeys(1)",
        "SELECT * FROM identity.login(sha256('c'::bytea), 1, '{}', '{}', sha256('s'::bytea), null)",
        "SELECT identity.begin_login(sha256('t'::bytea), '{}')",
        "SELECT identity.begin_addition(sha256('t'::bytea), sha256('s'::bytea), 1, '{}')",
        "SELECT * FROM identity.add_passkey(
             sha256('t'::bytea), sha256('s'::bytea), 1, sha256('c'::bytea), '{}', ''
         )",
    ] {
        let transaction = client
            .build_transaction()
            .isolation_level(tokio_postgres::IsolationLevel::RepeatableRead)
            .start()
            .await
            .expect("begins");
        transaction
            .batch_execute("SET LOCAL ROLE tetsu_identity")
            .await
            .expect("takes Identity's role");
        let refused = transaction.query(call, &[]).await.expect_err("refused");
        assert_eq!(
            refused.code().map(tokio_postgres::error::SqlState::code),
            Some("TD002"),
            "{call}"
        );
    }
}

/// Only live sessions and links count as ended: spent ones are removed too, but ending them changes nothing, so the
/// record says so.
#[tokio::test]
async fn spent_sessions_and_links_are_removed_but_not_counted() {
    let (database, operators, kurt) = with_kurt().await;
    let spent = "SELECT identity.start_session(sha256('a'::bytea), 'operator', 1, true);
                 UPDATE identity.sessions SET last_seen_at = last_seen_at - interval '16 minutes';
                 SELECT identity.issue_invite(sha256('b'::bytea), 1);
                 UPDATE identity.invites SET ends_at = now() - interval '1 second'";
    as_admin(&database, spent).await;
    assert_eq!(operators.end_sessions(kurt, &actor()).await, Ok(0));
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.sessions").await,
        0
    );
    as_admin(&database, spent).await;
    assert_eq!(operators.end_all_sessions(&actor()).await, Ok(0));
    as_admin(&database, spent).await;
    assert_eq!(
        operators.disable(kurt, &actor()).await,
        Ok(disabled(true, 0, 0))
    );
    assert_eq!(
        number(
            &database,
            "SELECT (SELECT count(*) FROM identity.sessions) + (SELECT count(*) FROM identity.invites)"
        )
        .await,
        0
    );
    let outcomes: Vec<Value> = events(&database)
        .await
        .into_iter()
        .skip(1)
        .map(|(_, payload)| payload["outcome"].clone())
        .collect();
    assert_eq!(
        outcomes,
        [json!("unchanged"), json!("unchanged"), json!("changed")]
    );
}

/// A login that waits while the operator's passkeys are removed finds its passkey gone: it starts no session,
/// and answers that the passkey is unknown, not that another login changed it.
#[tokio::test]
async fn a_login_waiting_on_a_removal_finds_its_passkey_unknown() {
    let (database, pool) = kurt_and_pool().await;
    as_admin(&database, &passkey_for(1)).await;
    let mut admin = connect(&database).await.expect("connects");
    let removing = admin.transaction().await.expect("begins");
    removing
        .execute("SELECT * FROM identity.remove_passkeys(1)", &[])
        .await
        .expect("removes");
    let signing = tokio::spawn(async move {
        let mut client = pool.get().await.expect("a connection");
        let transaction = ModuleTransaction::begin(&mut client, "identity")
            .await
            .expect("begins");
        let answer: Option<String> = transaction
            .query_one(
                "SELECT outcome FROM identity.login(
                     sha256(int8send(1)), 1,
                     '{\"cred\": {\"counter\": 0, \"backup_eligible\": false, \"backup_state\": false}}',
                     '{\"cred\": {\"counter\": 0, \"backup_eligible\": false, \"backup_state\": false}}',
                     sha256('s'::bytea), null
                 )",
                &[],
            )
            .await
            .expect("answers")
            .try_get(0)
            .expect("text or null");
        transaction.commit().await.expect("commits");
        answer
    });
    waits_for_a_lock(&database).await;
    removing.commit().await.expect("commits");
    assert_eq!(signing.await.expect("finishes"), None);
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.sessions").await,
        0
    );
}

/// A login that names no operator logs no one in: it answers that the passkey is unknown.
#[tokio::test]
async fn a_login_for_no_operator_signs_no_one_in() {
    let (database, pool) = kurt_and_pool().await;
    let passkey = r#"{"cred": {"counter": 0, "backup_eligible": false, "backup_state": false}}"#;
    as_admin(
        &database,
        &format!(
            "INSERT INTO identity.passkeys (credential_id, operator_id, passkey, created_at)
             VALUES (sha256(int8send(1)), 1, '{passkey}', now())"
        ),
    )
    .await;
    let mut client = pool.get().await.expect("a connection");
    let transaction = ModuleTransaction::begin(&mut client, "identity")
        .await
        .expect("begins");
    let answer: Option<String> = transaction
        .query_one(
            "SELECT outcome FROM identity.login(
                 sha256(int8send(1)), null, $1::text::jsonb, $1::text::jsonb, sha256('s'::bytea), null
             )",
            &[&passkey],
        )
        .await
        .expect("answers")
        .try_get(0)
        .expect("text or null");
    assert_eq!(answer, None);
}

/// Beginning an addition while its session ends waits for the end, then refuses as for any session that is not
/// live, rather than failing on the addition's tie to its session.
#[tokio::test]
async fn an_addition_begun_as_its_session_ends_is_refused() {
    let (database, pool) = kurt_and_pool().await;
    as_admin(
        &database,
        "SELECT identity.start_session(sha256('s'::bytea), 'operator', 1, true)",
    )
    .await;
    let mut admin = connect(&database).await.expect("connects");
    let ending = admin.transaction().await.expect("begins");
    ending
        .execute("DELETE FROM identity.sessions", &[])
        .await
        .expect("ends");
    let beginning = tokio::spawn(async move {
        let mut client = pool.get().await.expect("a connection");
        let transaction = ModuleTransaction::begin(&mut client, "identity")
            .await
            .expect("begins");
        let begun: bool = transaction
            .query_one(
                "SELECT identity.begin_addition(sha256('t'::bytea), sha256('s'::bytea), 1, '{}')",
                &[],
            )
            .await
            .expect("answers")
            .try_get(0)
            .expect("a boolean");
        transaction.commit().await.expect("commits");
        begun
    });
    waits_for_a_lock(&database).await;
    ending.commit().await.expect("commits");
    assert!(!beginning.await.expect("finishes"));
}

/// A session starts when it is stored, after any wait for the operator's row, never before an end of sessions that
/// held it up.
#[tokio::test]
async fn a_session_starts_after_the_wait_that_held_it_up() {
    let database = ready().await;
    let pool = pool(&database.as_app()).expect("a pool");
    let operators = postgres_store(pool.clone());
    operators
        .create(&name("Kurt Medley"), None, &actor())
        .await
        .expect("creates");
    let mut admin = connect(&database).await.expect("connects");
    let holding = admin.transaction().await.expect("begins");
    holding
        .execute(
            "SELECT FROM identity.operators WHERE id = 1 FOR NO KEY UPDATE",
            &[],
        )
        .await
        .expect("locks");
    // Only a login starts a session, so the administrator stands in for one.
    let mut client = connect(&database).await.expect("connects");
    let starting = tokio::spawn(async move {
        let writing = client.transaction().await.expect("begins");
        writing
            .execute(
                "SELECT identity.start_session(sha256('a'::bytea), 'operator', 1, true)",
                &[],
            )
            .await
            .expect("writes");
        writing.commit().await.expect("commits");
    });
    waits_for_a_lock(&database).await;
    let released: std::time::SystemTime = holding
        .query_one("SELECT clock_timestamp()", &[])
        .await
        .expect("reads the clock")
        .try_get(0)
        .expect("a time");
    holding.commit().await.expect("commits");
    starting.await.expect("finishes");
    let started: std::time::SystemTime = connect(&database)
        .await
        .expect("connects")
        .query_one("SELECT started_at FROM identity.sessions", &[])
        .await
        .expect("reads")
        .try_get(0)
        .expect("a time");
    assert!(started >= released, "{started:?} before {released:?}");
}

/// Identity's role adds an operator by name alone, and the database refuses a name with a control character, so
/// neither a direct insert nor a later path around the domain's checks can forge a line of the command's output.
#[tokio::test]
async fn an_operator_is_added_by_a_plain_name_alone() {
    let database = ready().await;
    let pool = pool(&database.as_app()).expect("a pool");
    let mut client = pool.get().await.expect("a connection");
    for (insert, refused) in [
        (
            "INSERT INTO identity.operators (name, disabled_at) VALUES ('Eve', now())",
            tokio_postgres::error::SqlState::INSUFFICIENT_PRIVILEGE,
        ),
        (
            "INSERT INTO identity.operators (id, name) OVERRIDING SYSTEM VALUE VALUES (3, 'Eve')",
            tokio_postgres::error::SqlState::INSUFFICIENT_PRIVILEGE,
        ),
        (
            "INSERT INTO identity.operators (name) VALUES (E'Eve\\n1  Kurt Medley  enabled')",
            tokio_postgres::error::SqlState::CHECK_VIOLATION,
        ),
    ] {
        let transaction = ModuleTransaction::begin(&mut client, "identity")
            .await
            .expect("begins");
        let error = transaction.execute(insert, &[]).await.expect_err("refused");
        assert_eq!(error.code(), Some(&refused), "{insert}");
    }
}

/// Names are unique, so a create retried after its output was lost finds the first operator rather than making a
/// second; the refusal is recorded, naming the operator who holds the name.
#[tokio::test]
async fn a_name_already_taken_is_refused_and_recorded() {
    let (database, operators) = migrated().await;
    operators
        .create(&name("Kurt Medley"), None, &actor())
        .await
        .expect("creates");
    assert_eq!(
        operators.create(&name("Kurt Medley"), None, &actor()).await,
        Err(Error::Invalid("operator 1 already has that name".into()))
    );
    let (kind, payload) = events(&database).await.pop().expect("an event");
    assert_eq!(kind, "identity.operator_created");
    assert_eq!(payload["operator_id"], json!(1));
    assert_eq!(payload["outcome"], json!("refused"));
}

/// A create that waits on another of the same name, still being written, is refused once that one commits, though
/// it cannot yet see who holds the name.
#[tokio::test]
async fn a_name_taken_while_a_create_waits_is_refused() {
    let (database, operators) = migrated().await;
    let mut admin = connect(&database).await.expect("connects");
    let writing = admin.transaction().await.expect("begins");
    writing
        .execute(
            "INSERT INTO identity.operators (name) VALUES ('Kurt Medley')",
            &[],
        )
        .await
        .expect("writes");
    let creating =
        tokio::spawn(async move { operators.create(&name("Kurt Medley"), None, &actor()).await });
    waits_for_a_lock(&database).await;
    writing.commit().await.expect("commits");
    assert_eq!(
        creating.await.expect("finishes"),
        Err(Error::Invalid(
            "another operator already has that name".into()
        ))
    );
    let (_, payload) = events(&database).await.pop().expect("an event");
    assert_eq!(payload["operator_id"], Value::Null);
    assert_eq!(payload["outcome"], json!("refused"));
}

/// A disable never answers "already disabled" over an enable about to commit: it waits for the enable, then
/// disables, so the operator ends disabled, as it says.
#[tokio::test]
async fn a_disable_waits_for_an_enable_being_written() {
    let database = ready().await;
    let pool = pool(&database.as_app()).expect("a pool");
    let operators = postgres_store(pool.clone());
    let kurt = operators
        .create(&name("Kurt Medley"), None, &actor())
        .await
        .expect("creates");
    operators.disable(kurt, &actor()).await.expect("disables");
    let mut client = pool.get().await.expect("a connection");
    let enabling = ModuleTransaction::begin(&mut client, "identity")
        .await
        .expect("begins");
    enabling
        .execute("SELECT identity.enable_operator($1)", &[&kurt.get()])
        .await
        .expect("enables");
    let disabling = tokio::spawn(async move { operators.disable(kurt, &actor()).await });
    waits_for_a_lock(&database).await;
    enabling.commit().await.expect("commits");
    assert_eq!(disabling.await.expect("finishes"), Ok(disabled(true, 0, 0)));
    assert_eq!(
        number(
            &database,
            "SELECT count(*) FROM identity.operators WHERE disabled_at IS NOT NULL"
        )
        .await,
        1
    );
}

/// An operator who holds a passkey is issued no link: another passkey is added only after logging in
/// (`v5.0.0-6.4.1`). Each refusal records why.
#[tokio::test]
async fn a_joined_or_disabled_operator_is_issued_no_link() {
    let (database, operators, kurt) = with_kurt().await;
    let lee = operators
        .create(&name("Lee Park"), None, &actor())
        .await
        .expect("creates");
    as_admin(&database, &passkey_for(kurt.get())).await;
    operators.disable(lee, &actor()).await.expect("disables");
    assert_eq!(
        operators.invite(kurt, &actor()).await.map(drop),
        Err(Error::Invalid(
            "operator 1 has a passkey already, and adds another after logging in".into()
        ))
    );
    assert_eq!(
        operators.invite(lee, &actor()).await.map(drop),
        Err(Error::Invalid("no enabled operator 2".into()))
    );
    let reasons: Vec<Value> = events(&database)
        .await
        .into_iter()
        .filter(|(kind, _)| kind == "identity.invite_issued")
        .map(|(_, payload)| json!([payload["outcome"], payload["reason"]]))
        .collect();
    assert_eq!(
        reasons,
        [json!(["refused", "joined"]), json!(["refused", "disabled"])]
    );
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.invites").await,
        0
    );
}

/// The list shows how many passkeys each operator holds, and the minutes left on an open link, rounded up, but
/// not a link that has ended.
#[tokio::test]
async fn the_list_shows_passkeys_and_open_links() {
    let (database, operators, kurt) = with_kurt().await;
    let lee = operators
        .create(&name("Lee Park"), None, &actor())
        .await
        .expect("creates");
    let ana = operators
        .create(&name("Ana Ortiz"), None, &actor())
        .await
        .expect("creates");
    operators.invite(kurt, &actor()).await.expect("issues");
    operators.invite(ana, &actor()).await.expect("issues");
    as_admin(
        &database,
        &format!(
            "{} UPDATE identity.invites SET ends_at = now() - interval '1 second' WHERE operator_id = {};",
            passkey_for(lee.get()),
            ana.get()
        ),
    )
    .await;
    let listed: Vec<(u64, Option<u64>)> = operators
        .list()
        .await
        .expect("lists")
        .iter()
        .map(|operator| (operator.passkeys, operator.link_minutes))
        .collect();
    assert_eq!(listed, [(0, Some(60)), (1, None), (0, None)]);
}

/// A begin holds the operator's row as it stores a ceremony, so a disable at the same moment waits for it and then
/// ends what it stored, and two begins on one link leave one ceremony.
#[tokio::test]
async fn a_disable_or_second_begin_waits_for_a_begin_being_written() {
    for second in ["disable", "begin"] {
        let database = ready().await;
        let pool = pool(&database.as_app()).expect("a pool");
        let operators = postgres_store(pool.clone());
        let kurt = operators
            .create(&name("Kurt Medley"), None, &actor())
            .await
            .expect("creates");
        let link = operators
            .invite(kurt, &actor())
            .await
            .expect("issues")
            .to_text();
        let begin = "SELECT identity.begin_join(
             sha256(convert_to($1, 'UTF8')), 1, sha256(decode($2, 'hex')), '{}')";
        let mut client = pool.get().await.expect("a connection");
        let writing = ModuleTransaction::begin(&mut client, "identity")
            .await
            .expect("begins");
        writing
            .execute(begin, &[&"first", &link])
            .await
            .expect("writes");
        let other = pool.clone();
        let then = tokio::spawn(async move {
            if second == "disable" {
                postgres_store(other).disable(kurt, &actor()).await.is_ok()
            } else {
                let mut client = other.get().await.expect("a connection");
                let racing = ModuleTransaction::begin(&mut client, "identity")
                    .await
                    .expect("begins");
                racing
                    .execute(begin, &[&"second", &link])
                    .await
                    .expect("writes");
                racing.commit().await.is_ok()
            }
        });
        waits_for_a_lock(&database).await;
        writing.commit().await.expect("commits");
        assert!(then.await.expect("finishes"), "{second}");
        let ceremonies = number(&database, "SELECT count(*) FROM identity.ceremonies").await;
        assert_eq!(ceremonies, i64::from(second != "disable"), "{second}");
    }
}

/// A new link takes the operator's row for update, so a begin from the old link waits for it, then finds its link
/// gone and keeps no ceremony.
#[tokio::test]
async fn a_begin_waits_for_a_new_link_being_written() {
    let database = ready().await;
    let pool = pool(&database.as_app()).expect("a pool");
    let operators = postgres_store(pool.clone());
    let kurt = operators
        .create(&name("Kurt Medley"), None, &actor())
        .await
        .expect("creates");
    let old = operators
        .invite(kurt, &actor())
        .await
        .expect("issues")
        .to_text();
    let mut client = pool.get().await.expect("a connection");
    let issuing = ModuleTransaction::begin(&mut client, "identity")
        .await
        .expect("begins");
    issuing
        .execute("SELECT identity.issue_invite(sha256('new'::bytea), 1)", &[])
        .await
        .expect("issues");
    let other = pool.clone();
    let beginning = tokio::spawn(async move {
        let mut client = other.get().await.expect("a connection");
        let racing = ModuleTransaction::begin(&mut client, "identity")
            .await
            .expect("begins");
        let begun: bool = racing
            .query_one(
                "SELECT identity.begin_join(sha256('t'::bytea), 1, sha256(decode($1, 'hex')), '{}')",
                &[&old],
            )
            .await
            .expect("answers")
            .try_get(0)
            .expect("a boolean");
        racing.commit().await.expect("commits");
        begun
    });
    waits_for_a_lock(&database).await;
    issuing.commit().await.expect("commits");
    assert!(!beginning.await.expect("finishes"));
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.ceremonies").await,
        0
    );
}

/// Removing an operator's passkeys lets them be issued a link and join again, as when their only one is lost. It
/// ends their sessions and any ceremony with it, says what it removed, and records each attempt: changed, unchanged
/// when there was nothing to remove, and refused for an operator who does not exist.
#[tokio::test]
async fn removing_passkeys_lets_an_operator_join_again() {
    let (database, operators, kurt) = with_kurt().await;
    as_admin(&database, &passkey_for(kurt.get())).await;
    logged_in(&database, kurt).await;
    let removed = [
        operators.remove_passkeys(kurt, &actor()).await,
        operators.remove_passkeys(kurt, &actor()).await,
    ];
    let removal = |passkeys_removed, sessions_ended| {
        Ok(Removal {
            passkeys_removed,
            sessions_ended,
        })
    };
    assert_eq!(removed, [removal(1, 1), removal(0, 0)]);
    assert_eq!(
        operators
            .remove_passkeys(OperatorId::parse("99").expect("an ID"), &actor())
            .await,
        Err(Error::Invalid("no operator 99".into()))
    );
    assert!(operators.invite(kurt, &actor()).await.is_ok());
    let recorded: Vec<Value> = events(&database)
        .await
        .into_iter()
        .filter(|(kind, _)| kind == "identity.passkeys_removed")
        .map(|(_, payload)| {
            json!([
                payload["outcome"],
                payload["passkeys_removed"],
                payload["sessions_ended"]
            ])
        })
        .collect();
    assert_eq!(
        recorded,
        [
            json!(["changed", 1, 1]),
            json!(["unchanged", 0, 0]),
            json!(["refused", null, null])
        ]
    );
}

/// A removal whose answer has a column of the wrong type is a storage failure, whichever column it is.
#[tokio::test]
async fn a_removal_answer_of_the_wrong_type_is_a_storage_failure() {
    for (columns, values) in [
        (
            "passkeys_removed text, sessions_ended bigint",
            "''1'', 0::bigint",
        ),
        (
            "passkeys_removed bigint, sessions_ended text",
            "0::bigint, ''1''",
        ),
    ] {
        let (database, operators, kurt) = with_kurt().await;
        as_admin(
            &database,
            &format!(
                "DROP FUNCTION identity.remove_passkeys(bigint);
                 CREATE FUNCTION identity.remove_passkeys(bigint) RETURNS TABLE ({columns})
                 LANGUAGE sql AS 'SELECT {values}';
                 GRANT EXECUTE ON FUNCTION identity.remove_passkeys(bigint) TO tetsu_identity"
            ),
        )
        .await;
        assert!(
            matches!(
                operators.remove_passkeys(kurt, &actor()).await,
                Err(Error::Storage(_))
            ),
            "{columns}"
        );
    }
}
