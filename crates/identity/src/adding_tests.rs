//! Adding an operator's second passkey after a fresh sign-in (ADR-0057), end to end, with passkey-rs's authenticator
//! in place of their devices (ADR-0058), against Postgres signed in as the application role. They live in this one
//! build for the reason enrollment's do: llvm-cov scores each build of a generic use case on its own.

use passkey::types::Passkey;
use serde_json::{Value, json};
use tetsu_postgres::test_support::TestDatabase;

use crate::adapters::postgres::PostgresOperators;
use crate::domain::{Addition, NewPasskey};
use crate::enrollment_tests::{
    ORIGIN, actor, app_pool, as_admin, failure, number, party, recorded, register_keeping, replace,
};
use crate::ports::OperatorStore;
use crate::sign_in_tests::{answer, enrolled};
use crate::{
    Answer, Change, CheckedSession, ForChange, OperatorId, Operators, Origins, Realm, Session,
    TakenAddition,
};

fn not_adding() -> String {
    "invalid: this passkey could not be added".to_string()
}

/// Kurt Medley, enrolled and freshly signed in, and his live session, as a request that passed the checks for a
/// change finds it.
async fn signed_in() -> (
    Operators<PostgresOperators>,
    TestDatabase,
    OperatorId,
    CheckedSession,
) {
    let (operators, database, kurt, kept) = enrolled().await;
    let (token, response) = answer(&operators, kept, ORIGIN, |_| {}).await;
    let session_token = operators
        .finish_sign_in(
            &party(),
            Answer {
                ceremony: &token,
                response: &response,
            },
            &[],
            &actor(),
        )
        .await
        .expect("signs in");
    let cookie = format!(
        "{}={}",
        Realm::Operator.cookie_name(),
        session_token.to_text()
    );
    let form = session_token.form_token().expect("a form token");
    let origins = Origins::parse(&[ORIGIN]).expect("origins");
    let change = Change::new([cookie.as_str()], Some(ORIGIN), None, &form);
    let ForChange::Acting(session) = operators
        .session_for_change(&origins, &change)
        .await
        .expect("reads")
    else {
        panic!("a session acting for a change");
    };
    (operators, database, kurt, session)
}

/// An addition taken for `session`, stored as the administrator, as storing a passkey needs one: any addition
/// the operator held is replaced.
async fn taken(database: &TestDatabase, session: &CheckedSession, n: u8) -> TakenAddition {
    let addition = TakenAddition {
        token: [n; 32],
        session: session.session().hash,
        operator: session.session().operator(),
        state: "{}".to_string(),
    };
    let hex = |bytes: &[u8]| {
        bytes
            .iter()
            .fold(String::new(), |text, byte| format!("{text}{byte:02x}"))
    };
    as_admin(
        database,
        &format!(
            "DELETE FROM identity.ceremonies WHERE kind = 'add';
             INSERT INTO identity.ceremonies
                 (token_hash, kind, operator_id, session_hash, taken, state, ends_at)
             VALUES ('\\x{}', 'add', {}, '\\x{}', true, '{{}}', now() + interval '10 minutes')",
            hex(&addition.token),
            addition.operator.get(),
            hex(&addition.session)
        ),
    )
    .await;
    addition
}

/// Begins adding a passkey for `session`, registers one on a new device, and finishes with `response` rewritten by
/// `rewrite`: what finishing says, and the options the device was given.
async fn add(
    operators: &Operators<PostgresOperators>,
    session: &CheckedSession,
    rewrite: impl Fn(String) -> String,
) -> (Result<(), crate::Error>, Value) {
    let party = party();
    let ceremony = operators
        .begin_adding_passkey(&party, session)
        .await
        .expect("begins");
    let (response, _kept): (String, Passkey) =
        register_keeping(&ceremony.options, ORIGIN, true).await;
    let answer = Answer {
        ceremony: &ceremony.token.to_text(),
        response: &rewrite(response),
    };
    let added = operators
        .finish_adding_passkey(&party, session, answer, &actor())
        .await;
    (
        added,
        serde_json::from_str(&ceremony.options).expect("JSON"),
    )
}

/// A live session finds its account: the operator, their user handle and name, and every credential they hold, as
/// the operator's page and the browser's passkey list need, however long ago it began. A session that ended, or
/// whose operator is disabled, finds none.
#[tokio::test]
async fn a_live_session_finds_its_account() {
    let (operators, database, kurt, session) = signed_in().await;
    as_admin(
        &database,
        "UPDATE identity.sessions SET started_at = now() - interval '2 hours'",
    )
    .await;
    let account = operators
        .account(session.session())
        .await
        .expect("reads")
        .expect("an account");
    let handle = "SELECT count(*) FROM identity.operators WHERE uuid_send(user_handle) = $1";
    let found: i64 = tetsu_postgres::connect(&database)
        .await
        .expect("connects")
        .query_one(handle, &[&account.user_handle.as_slice()])
        .await
        .expect("counts")
        .try_get(0)
        .expect("a count");
    assert_eq!(
        (
            account.operator,
            account.name.as_str(),
            account.credentials.len(),
            found
        ),
        (kurt, "Kurt Medley", 1, 1)
    );
    operators.disable(kurt, &actor()).await.expect("disables");
    assert_eq!(operators.account(session.session()).await, Ok(None));
}

/// Reading an account fails as storage when the database refuses its lookup.
#[tokio::test]
async fn an_account_that_cannot_be_read_is_a_storage_failure() {
    let (operators, database, _, session) = signed_in().await;
    as_admin(
        &database,
        "REVOKE EXECUTE ON FUNCTION identity.account(bytea) FROM tetsu_identity",
    )
    .await;
    assert_eq!(
        failure(operators.account(session.session()).await.map(drop)),
        "storage failed: permission denied for function account"
    );
}

/// A freshly signed-in operator adds a second passkey: the device is told which passkey they hold, so it makes no
/// second on a device that has one, and the addition is recorded. A third is refused, as ADR-0057 allows two.
#[tokio::test]
async fn an_operator_adds_a_second_passkey_and_no_third() {
    let (operators, database, _, session) = signed_in().await;
    let (added, options) = add(&operators, &session, |response| response).await;
    assert_eq!(added, Ok(()));
    let excluded = options["publicKey"]["excludeCredentials"]
        .as_array()
        .map_or(0, Vec::len);
    assert_eq!(
        [
            excluded,
            usize::try_from(number(&database, "SELECT count(*) FROM identity.passkeys").await)
                .unwrap_or_default()
        ],
        [1, 2]
    );
    assert_eq!(
        failure(
            operators
                .begin_adding_passkey(&party(), &session)
                .await
                .map(drop)
        ),
        not_adding()
    );
    assert_eq!(
        recorded(&database, "identity.passkey_added").await,
        [json!(["changed", null])]
    );
}

/// Adding a passkey ends the operator's other sessions and keeps the one that added it (ADR-0059,
/// `v5.0.0-7.4.3`), and the record says how many live ones it ended.
#[tokio::test]
async fn adding_a_passkey_ends_the_operators_other_sessions() {
    let (operators, database, _, session) = signed_in().await;
    as_admin(
        &database,
        "SELECT identity.start_session(sha256('other'::bytea), 'operator', 1)",
    )
    .await;
    let (added, _) = add(&operators, &session, |response| response).await;
    assert_eq!(added, Ok(()));
    let left = "SELECT count(*) FROM identity.sessions WHERE token_hash <> sha256('other'::bytea)";
    assert_eq!(
        [
            number(&database, "SELECT count(*) FROM identity.sessions").await,
            number(&database, left).await
        ],
        [1, 1]
    );
    let ended = "SELECT (payload ->> 'sessions_ended')::bigint FROM identity.outbox_global
                 WHERE event_type = 'identity.passkey_added'";
    assert_eq!(number(&database, ended).await, 1);
}

/// An addition belongs to the session that began it: another of the operator's sessions cannot finish it, nor use
/// it up.
#[tokio::test]
async fn only_the_session_that_began_an_addition_finishes_it() {
    let (operators, _database, _, session) = signed_in().await;
    let party = party();
    let ceremony = operators
        .begin_adding_passkey(&party, &session)
        .await
        .expect("begins");
    let other = CheckedSession(Session {
        hash: [7; 32],
        ..session.session().clone()
    });
    let (response, _kept) = register_keeping(&ceremony.options, ORIGIN, true).await;
    let token = ceremony.token.to_text();
    let answer = || Answer {
        ceremony: &token,
        response: &response,
    };
    assert_eq!(
        failure(
            operators
                .finish_adding_passkey(&party, &other, answer(), &actor())
                .await
        ),
        not_adding()
    );
    assert_eq!(
        operators
            .finish_adding_passkey(&party, &session, answer(), &actor())
            .await,
        Ok(()),
        "another session's attempt left it for its own"
    );
}

/// An operator has one addition open at a time, however many begin, and it ends with the session that began it.
#[tokio::test]
async fn one_addition_is_open_at_a_time_and_ends_with_its_session() {
    let (operators, database, _, session) = signed_in().await;
    let open = "SELECT count(*) FROM identity.ceremonies WHERE kind = 'add'";
    for _ in 0..2 {
        operators
            .begin_adding_passkey(&party(), &session)
            .await
            .expect("begins");
    }
    assert_eq!(number(&database, open).await, 1);
    as_admin(&database, "DELETE FROM identity.sessions").await;
    assert_eq!(number(&database, open).await, 0, "it ends with its session");
}

/// Only a fresh sign-in may add a passkey: a session begun more than ten minutes ago is refused.
#[tokio::test]
async fn a_session_that_is_not_fresh_adds_no_passkey() {
    let (operators, database, _, session) = signed_in().await;
    as_admin(
        &database,
        "UPDATE identity.sessions SET started_at = now() - interval '11 minutes'",
    )
    .await;
    assert_eq!(
        failure(
            operators
                .begin_adding_passkey(&party(), &session)
                .await
                .map(drop)
        ),
        not_adding()
    );
}

/// Finishing refuses, the same way each time, a registration that is not one, a ceremony token that is unknown or
/// malformed, and a session that went idle meanwhile, which is recorded.
#[tokio::test]
async fn finishing_refuses_what_no_longer_allows_it() {
    let (operators, database, _, session) = signed_in().await;
    let (refused, _) = add(&operators, &session, |_| "{}".to_string()).await;
    let mut said = vec![refused];
    for ceremony in ["ab".repeat(32), "not a token".to_string()] {
        let answer = Answer {
            ceremony: &ceremony,
            response: "{}",
        };
        said.push(
            operators
                .finish_adding_passkey(&party(), &session, answer, &actor())
                .await,
        );
    }
    let ended = {
        let ceremony = operators
            .begin_adding_passkey(&party(), &session)
            .await
            .expect("begins");
        let (response, _) = register_keeping(&ceremony.options, ORIGIN, true).await;
        as_admin(
            &database,
            "UPDATE identity.sessions SET last_seen_at = now() - interval '16 minutes'",
        )
        .await;
        let answer = Answer {
            ceremony: &ceremony.token.to_text(),
            response: &response,
        };
        operators
            .finish_adding_passkey(&party(), &session, answer, &actor())
            .await
    };
    said.push(ended);
    let said: Vec<String> = said.into_iter().map(failure).collect();
    assert_eq!(said, vec![not_adding(); 4]);
    assert_eq!(
        recorded(&database, "identity.passkey_added").await,
        [json!(["refused", "signed_out"])]
    );
}

/// Storing an added passkey refuses an operator disabled, holding two already, a credential or passkey larger than
/// the table keeps, and an operator who does not exist, recording each.
#[tokio::test]
async fn storing_an_added_passkey_refuses_and_records_why() {
    let (operators, database, kurt, session) = signed_in().await;
    let store = PostgresOperators {
        pool: app_pool(&database),
    };
    let fresh = |id: u8, size: usize| NewPasskey {
        credential_id: vec![id; size],
        passkey: "{}".to_string(),
    };
    let mut answers = Vec::new();
    for (passkey, operator) in [
        (fresh(1, 15), kurt),
        (fresh(2, 16), OperatorId::new(99)),
        (fresh(3, 16), kurt),
        (fresh(3, 16), kurt),
    ] {
        let addition = TakenAddition {
            operator,
            ..taken(&database, &session, 1).await
        };
        answers.push(store.add_passkey(&addition, &passkey, &actor()).await);
    }
    let addition = taken(&database, &session, 2).await;
    operators.disable(kurt, &actor()).await.expect("disables");
    answers.push(store.add_passkey(&addition, &fresh(4, 16), &actor()).await);
    assert_eq!(
        answers,
        [
            Ok(Some(Addition::Oversized)),
            Ok(None),
            Ok(Some(Addition::Added)),
            Ok(Some(Addition::Full)),
            Ok(Some(Addition::Disabled)),
        ]
    );
    let reasons: Vec<Value> = recorded(&database, "identity.passkey_added").await;
    assert_eq!(
        reasons,
        [
            json!(["refused", "oversized"]),
            json!(["refused", "unknown"]),
            json!(["changed", null]),
            json!(["refused", "full"]),
            json!(["refused", "disabled"]),
        ]
    );
}

/// An addition this session did not take is refused as ended, and a credential another passkey has as taken; each
/// record names the adding session by its public ID, and the passkey's flags.
#[tokio::test]
async fn an_addition_not_taken_or_a_credential_taken_adds_nothing() {
    let (_operators, database, _, session) = signed_in().await;
    let store = PostgresOperators {
        pool: app_pool(&database),
    };
    let first: Vec<u8> = tetsu_postgres::connect(&database)
        .await
        .expect("connects")
        .query_one("SELECT credential_id FROM identity.passkeys", &[])
        .await
        .expect("reads")
        .try_get(0)
        .expect("bytes");
    let taken = NewPasskey {
        credential_id: first,
        passkey: "{}".to_string(),
    };
    let untaken = TakenAddition {
        token: [9; 32],
        ..self::taken(&database, &session, 8).await
    };
    let addition = self::taken(&database, &session, 1).await;
    assert_eq!(
        [
            store.add_passkey(&untaken, &taken, &actor()).await,
            store.add_passkey(&addition, &taken, &actor()).await
        ],
        [Ok(Some(Addition::Ended)), Ok(Some(Addition::Taken))]
    );
    let described = "SELECT count(*) FROM identity.outbox_global AS o, identity.sessions AS s
                     WHERE o.event_type = 'identity.passkey_added' AND o.payload ->> 'session' = s.id::text
                       AND o.payload -> 'flags' ?& array['backup_eligible', 'backup_state', 'user_verified']";
    assert_eq!(number(&database, described).await, 2);
}

/// Failures that are not the browser's are told apart: no random source, and answers from storage that cannot be
/// read, whether a column of the wrong type, a user handle that is not 16 bytes, or an answer never written.
#[tokio::test]
async fn failures_of_the_server_are_told_apart() {
    let (_operators, database, _, session) = signed_in().await;
    let failing = Operators::with_fill(
        PostgresOperators {
            pool: app_pool(&database),
        },
        |_| Err(getrandom::Error::UNSUPPORTED),
    );
    assert_eq!(
        failure(
            failing
                .begin_adding_passkey(&party(), &session)
                .await
                .map(drop)
        ),
        format!(
            "temporarily unavailable: no secure random source: {}",
            getrandom::Error::UNSUPPORTED
        )
    );
    let mut said = Vec::new();
    for (function, returns, body) in UNREADABLE {
        let (operators, database, _, session) = signed_in().await;
        replace(&database, function, returns, body).await;
        said.push(failure(add_once(&operators, &session).await));
    }
    assert!(
        said.iter().all(|said| said.starts_with("storage failed: ")),
        "{said:?}"
    );
    assert_eq!(said[1], "storage failed: a user handle is not 16 bytes");
    assert_eq!(
        said[6],
        "storage failed: adding a passkey answered \"maybe\""
    );
    assert_eq!(
        said[10..],
        [
            "storage failed: a lookup that always answers answered nothing",
            "storage failed: a lookup that acted for a session named none",
            "storage failed: a count is negative",
            "storage failed: a lookup that always answers answered nothing"
        ]
    );
}

/// Each function replaced, what it returns instead, and what it answers.
const UNREADABLE: [(&str, &str, &str); 14] = [
    (
        "adder(bytea)",
        "TABLE (operator_id text, user_handle bytea, name text, credentials bytea[])",
        "SELECT '1', uuid_send(gen_random_uuid()), 'Kurt', array[]::bytea[]",
    ),
    (
        "adder(bytea)",
        "TABLE (operator_id bigint, user_handle bytea, name text, credentials bytea[])",
        "SELECT 1::bigint, '\\x00'::bytea, 'Kurt', array[]::bytea[]",
    ),
    (
        "adder(bytea)",
        "TABLE (operator_id bigint, user_handle bytea, name integer, credentials bytea[])",
        "SELECT 1::bigint, uuid_send(gen_random_uuid()), 1, array[]::bytea[]",
    ),
    (
        "adder(bytea)",
        "TABLE (operator_id bigint, user_handle bytea, name text, credentials text)",
        "SELECT 1::bigint, uuid_send(gen_random_uuid()), 'Kurt', 'x'",
    ),
    (
        "take_addition(bytea, bytea)",
        "TABLE (operator_id text, state text)",
        "SELECT '1', '{}'",
    ),
    (
        "take_addition(bytea, bytea)",
        "TABLE (operator_id bigint, state integer)",
        "SELECT 1::bigint, 1",
    ),
    (
        "add_passkey(bytea, bytea, bigint, bytea, jsonb)",
        "TABLE (added text, sessions_ended bigint, session_id text)",
        "SELECT 'maybe', 0::bigint, 'x'",
    ),
    (
        "add_passkey(bytea, bytea, bigint, bytea, jsonb)",
        "TABLE (added integer, sessions_ended bigint, session_id text)",
        "SELECT 1, 0::bigint, 'x'",
    ),
    (
        "add_passkey(bytea, bytea, bigint, bytea, jsonb)",
        "TABLE (added text, sessions_ended text, session_id text)",
        "SELECT 'added', 'x', 'x'",
    ),
    (
        "add_passkey(bytea, bytea, bigint, bytea, jsonb)",
        "TABLE (added text, sessions_ended bigint, session_id integer)",
        "SELECT 'added', 0::bigint, 1",
    ),
    (
        "add_passkey(bytea, bytea, bigint, bytea, jsonb)",
        "TABLE (added text, sessions_ended bigint, session_id text)",
        "SELECT 'added', 0::bigint, 'x' WHERE false",
    ),
    (
        "add_passkey(bytea, bytea, bigint, bytea, jsonb)",
        "TABLE (added text, sessions_ended bigint, session_id text)",
        "SELECT 'added', 0::bigint, null::text",
    ),
    (
        "add_passkey(bytea, bytea, bigint, bytea, jsonb)",
        "TABLE (added text, sessions_ended bigint, session_id text)",
        "SELECT 'added', -1::bigint, 'x'",
    ),
    (
        "begin_addition(bytea, bytea, bigint, jsonb)",
        "SETOF boolean",
        "SELECT true WHERE false",
    ),
];

/// Begins and finishes one addition, stopping at the first failure.
async fn add_once(
    operators: &Operators<PostgresOperators>,
    session: &CheckedSession,
) -> Result<(), crate::Error> {
    let ceremony = operators.begin_adding_passkey(&party(), session).await?;
    let (response, _) = register_keeping(&ceremony.options, ORIGIN, true).await;
    let answer = Answer {
        ceremony: &ceremony.token.to_text(),
        response: &response,
    };
    operators
        .finish_adding_passkey(&party(), session, answer, &actor())
        .await
}

/// Each step of adding a passkey fails as storage when the database refuses its function.
#[tokio::test]
async fn each_step_fails_as_storage_when_refused() {
    let mut said = Vec::new();
    for function in [
        "adder(bytea)",
        "begin_addition(bytea, bytea, bigint, jsonb)",
        "take_addition(bytea, bytea)",
        "add_passkey(bytea, bytea, bigint, bytea, jsonb)",
    ] {
        let (operators, database, _, session) = signed_in().await;
        as_admin(
            &database,
            &format!("REVOKE EXECUTE ON FUNCTION identity.{function} FROM tetsu_identity"),
        )
        .await;
        said.push(failure(add_once(&operators, &session).await));
    }
    let expected: Vec<String> = ["adder", "begin_addition", "take_addition", "add_passkey"]
        .iter()
        .map(|name| format!("storage failed: permission denied for function {name}"))
        .collect();
    assert_eq!(said, expected);
}
