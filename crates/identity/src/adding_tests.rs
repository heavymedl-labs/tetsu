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
use crate::{Answer, OperatorId, Operators, Realm, Session};

fn not_adding() -> String {
    "invalid: this passkey could not be added".to_string()
}

/// Kurt Medley, enrolled and freshly signed in, and his live session.
async fn signed_in() -> (
    Operators<PostgresOperators>,
    TestDatabase,
    OperatorId,
    Session,
) {
    let (operators, database, kurt, kept) = enrolled().await;
    let (token, response) = answer(&operators, kept, ORIGIN, |_| {}).await;
    let session_token = operators
        .finish_sign_in(&party(), &token, &response, &actor())
        .await
        .expect("signs in");
    let cookie = format!(
        "{}={}",
        Realm::Operator.cookie_name(),
        session_token.to_text()
    );
    let session = operators
        .session([cookie.as_str()])
        .await
        .expect("reads")
        .expect("a session");
    (operators, database, kurt, session)
}

/// Begins adding a passkey for `session`, registers one on a new device, and finishes with `response` rewritten by
/// `rewrite`: what finishing says, and the options the device was given.
async fn add(
    operators: &Operators<PostgresOperators>,
    session: &Session,
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

/// An addition belongs to the session that began it: another of the operator's sessions cannot finish it, and an
/// operator has one addition open at a time, whatever begins them.
#[tokio::test]
async fn only_the_session_that_began_an_addition_finishes_it() {
    let (operators, database, _, session) = signed_in().await;
    let party = party();
    let ceremony = operators
        .begin_adding_passkey(&party, &session)
        .await
        .expect("begins");
    let mut other = session.clone();
    other.hash = [7; 32];
    let (response, _kept) = register_keeping(&ceremony.options, ORIGIN, true).await;
    let answer = Answer {
        ceremony: &ceremony.token.to_text(),
        response: &response,
    };
    assert_eq!(
        failure(
            operators
                .finish_adding_passkey(&party, &other, answer, &actor())
                .await
        ),
        not_adding()
    );
    let open = "SELECT count(*) FROM identity.ceremonies WHERE kind = 'add'";
    operators
        .begin_adding_passkey(&party, &session)
        .await
        .expect("begins again");
    operators
        .begin_adding_passkey(&party, &session)
        .await
        .expect("and again");
    assert_eq!(number(&database, open).await, 1);
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
/// malformed, and a session that ended meanwhile, which is recorded.
#[tokio::test]
async fn finishing_refuses_what_no_longer_allows_it() {
    let (operators, database, _, session) = signed_in().await;
    let (refused, _) = add(&operators, &session, |_| "{}".to_string()).await;
    let unknown = operators
        .finish_adding_passkey(
            &party(),
            &session,
            Answer {
                ceremony: &"ab".repeat(32),
                response: "{}",
            },
            &actor(),
        )
        .await;
    let malformed = operators
        .finish_adding_passkey(
            &party(),
            &session,
            Answer {
                ceremony: "not a token",
                response: "{}",
            },
            &actor(),
        )
        .await;
    let ended = {
        let ceremony = operators
            .begin_adding_passkey(&party(), &session)
            .await
            .expect("begins");
        let (response, _) = register_keeping(&ceremony.options, ORIGIN, true).await;
        as_admin(&database, "DELETE FROM identity.sessions").await;
        let answer = Answer {
            ceremony: &ceremony.token.to_text(),
            response: &response,
        };
        operators
            .finish_adding_passkey(&party(), &session, answer, &actor())
            .await
    };
    let said: Vec<String> = [refused, unknown, malformed, ended]
        .into_iter()
        .map(failure)
        .collect();
    assert_eq!(said, vec![not_adding(); 4]);
    assert_eq!(
        recorded(&database, "identity.passkey_added").await,
        [json!(["refused", "signed_out"])]
    );
}

/// Storing an added passkey refuses an operator disabled, holding two already, a credential or passkey larger than
/// the table keeps, a credential another passkey has, and an operator who does not exist, recording each.
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
        answers.push(
            store
                .add_passkey(&session.hash, operator, &passkey, &actor())
                .await,
        );
    }
    operators.disable(kurt, &actor()).await.expect("disables");
    answers.push(
        store
            .add_passkey(&session.hash, kurt, &fresh(4, 16), &actor())
            .await,
    );
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

/// A credential another operator's passkey has is refused as taken, and recorded.
#[tokio::test]
async fn a_taken_credential_is_not_added() {
    let (_operators, database, kurt, session) = signed_in().await;
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
    assert_eq!(
        store
            .add_passkey(&session.hash, kurt, &taken, &actor())
            .await,
        Ok(Some(Addition::Taken))
    );
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
    assert_eq!(said[6], "storage failed: a session hash is not 32 bytes");
    assert_eq!(
        said[8],
        "storage failed: adding a passkey answered \"maybe\""
    );
}

/// Each function replaced, what it returns instead, and what it answers.
const UNREADABLE: [(&str, &str, &str); 11] = [
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
        "take_ceremony(bytea, text)",
        "TABLE (operator_id text, link_hash bytea, session_hash bytea, state text)",
        "SELECT '1', null::bytea, sha256('a'::bytea), '{}'",
    ),
    (
        "take_ceremony(bytea, text)",
        "TABLE (operator_id bigint, link_hash bytea, session_hash bytea, state integer)",
        "SELECT 1::bigint, null::bytea, sha256('a'::bytea), 1",
    ),
    (
        "take_ceremony(bytea, text)",
        "TABLE (operator_id bigint, link_hash bytea, session_hash bytea, state text)",
        "SELECT 1::bigint, null::bytea, '\\x00'::bytea, '{}'",
    ),
    (
        "take_ceremony(bytea, text)",
        "TABLE (operator_id bigint, link_hash bytea, session_hash integer, state text)",
        "SELECT 1::bigint, null::bytea, 1, '{}'",
    ),
    (
        "add_passkey(bytea, bigint, bytea, jsonb)",
        "TABLE (added text, sessions_ended bigint)",
        "SELECT 'maybe', 0::bigint",
    ),
    (
        "add_passkey(bytea, bigint, bytea, jsonb)",
        "TABLE (added integer, sessions_ended bigint)",
        "SELECT 1, 0::bigint",
    ),
    (
        "add_passkey(bytea, bigint, bytea, jsonb)",
        "TABLE (added text, sessions_ended text)",
        "SELECT 'added', 'x'",
    ),
];

/// Begins and finishes one addition, stopping at the first failure.
async fn add_once(
    operators: &Operators<PostgresOperators>,
    session: &Session,
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
        "take_ceremony(bytea, text)",
        "add_passkey(bytea, bigint, bytea, jsonb)",
    ] {
        let (operators, database, _, session) = signed_in().await;
        as_admin(
            &database,
            &format!("REVOKE EXECUTE ON FUNCTION identity.{function} FROM tetsu_identity"),
        )
        .await;
        said.push(failure(add_once(&operators, &session).await));
    }
    let expected: Vec<String> = ["adder", "begin_addition", "take_ceremony", "add_passkey"]
        .iter()
        .map(|name| format!("storage failed: permission denied for function {name}"))
        .collect();
    assert_eq!(said, expected);
}
