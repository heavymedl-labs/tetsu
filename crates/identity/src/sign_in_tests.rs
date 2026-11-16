//! Signing an operator in with their passkey, end to end, with passkey-rs's authenticator in place of their device
//! (ADR-0058), against Postgres signed in as the application role. They live in this one build for the reason
//! enrollment's do: llvm-cov scores each build of a generic use case on its own.

use passkey::types::Passkey;
use serde_json::{Value, json};
use tetsu_postgres::connect;
use tetsu_postgres::test_support::TestDatabase;

use crate::adapters::postgres::PostgresOperators;
use crate::domain::{SignIn, Signer};
use crate::enrollment_tests::{
    ORIGIN, actor, app_pool, as_admin, authenticate, failure, number, party, recorded,
    register_keeping, replace, with_link,
};
use crate::ports::OperatorStore;
use crate::{OperatorId, Operators};

/// How a hostile client rewrites a sign-in's answer.
type Rewrite = fn(&mut Value);

fn not_signing_in() -> String {
    "invalid: this sign-in could not be completed".to_string()
}

/// Kurt Medley, operator 1, enrolled with a passkey, and the passkey his device keeps.
pub(crate) async fn enrolled() -> (
    Operators<PostgresOperators>,
    TestDatabase,
    OperatorId,
    Passkey,
) {
    let (operators, database, kurt, link) = with_link().await;
    let party = party();
    let ceremony = operators
        .begin_enrollment(&party, &link)
        .await
        .expect("begins");
    let (response, kept) = register_keeping(&ceremony.options, ORIGIN, true).await;
    operators
        .finish_enrollment(&party, &ceremony.token.to_text(), &response, &actor())
        .await
        .expect("enrolls");
    (operators, database, kurt, kept)
}

/// Begins a sign-in and has the device answer it from `origin`, with the answer rewritten by `rewrite`.
pub(crate) async fn answer(
    operators: &Operators<PostgresOperators>,
    passkey: Passkey,
    origin: &str,
    rewrite: impl Fn(&mut Value),
) -> (String, String) {
    let ceremony = operators.begin_sign_in(&party()).await.expect("begins");
    let mut response: Value =
        serde_json::from_str(&authenticate(&ceremony.options, origin, passkey, true).await)
            .expect("JSON");
    rewrite(&mut response);
    (ceremony.token.to_text(), response.to_string())
}

/// What each sign-in recorded, oldest first: its outcome and reason.
async fn sign_ins(database: &TestDatabase) -> Vec<Value> {
    recorded(database, "identity.signed_in").await
}

/// An enrolled operator signs in with their passkey: the options name no passkey and ask the browser to verify the
/// user with the prompt a button opens; the session starts, the passkey's last use is stored, and it is recorded.
/// The ceremony finishes once.
#[tokio::test]
async fn an_operator_signs_in_with_their_passkey() {
    let (operators, database, _, kept) = enrolled().await;
    let party = party();
    let ceremony = operators.begin_sign_in(&party).await.expect("begins");
    let options: Value = serde_json::from_str(&ceremony.options).expect("JSON");
    assert_eq!(
        [
            &options["publicKey"]["userVerification"],
            &options["mediation"],
            &options["publicKey"]["allowCredentials"]
        ],
        [&json!("required"), &Value::Null, &json!([])]
    );
    let response = authenticate(&ceremony.options, ORIGIN, kept, true).await;
    let token = ceremony.token.to_text();
    let signed = operators
        .finish_sign_in(&party, &token, &response, &actor())
        .await;
    assert!(signed.is_ok());
    let used = "SELECT count(*) FROM identity.passkeys WHERE last_used_at IS NOT NULL";
    assert_eq!(
        [
            number(&database, "SELECT count(*) FROM identity.sessions").await,
            number(&database, used).await
        ],
        [1, 1]
    );
    assert_eq!(
        failure(
            operators
                .finish_sign_in(&party, &token, &response, &actor())
                .await
                .map(drop)
        ),
        not_signing_in()
    );
    assert_eq!(sign_ins(&database).await, [json!(["changed", null])]);
    let described = "SELECT count(*) FROM identity.outbox_global AS o, identity.passkeys AS p,
                     identity.sessions AS s
                     WHERE o.payload ->> 'credential' = encode(p.credential_id, 'hex')
                       AND o.payload ->> 'session' = s.id::text
                       AND o.payload -> 'flags' ?& array['backup_eligible', 'backup_state', 'user_verified']";
    assert_eq!(
        number(&database, described).await,
        1,
        "the event names the credential, the session by its public ID, and the passkey's flags"
    );
}

/// A sign-in is refused, the same way each time, when its answer names another operator's user handle, comes from
/// another origin, names no user handle or a credential ID too long, or is not an answer at all (#109), and each is
/// recorded with its reason.
#[tokio::test]
async fn hostile_sign_ins_are_refused() {
    let (operators, database, _, kept) = enrolled().await;
    let party = party();
    let mut refused = Vec::new();
    let cases: [(&str, Rewrite); 6] = [
        (ORIGIN, |answer| {
            answer["response"]["userHandle"] = json!("AAAAAAAAAAAAAAAAAAAAAA");
        }),
        ("https://ops.example.com:8443", |_| {}),
        (ORIGIN, |answer| *answer = json!({"not": "an answer"})),
        (ORIGIN, |answer| *answer = json!("not JSON at all")),
        (ORIGIN, |answer| {
            answer["response"]["userHandle"] = Value::Null;
        }),
        // A credential ID past WebAuthn's 1023 bytes, which no record could keep.
        (ORIGIN, |answer| {
            answer["id"] = json!("A".repeat(1368));
            answer["rawId"] = json!("A".repeat(1368));
        }),
    ];
    for (origin, rewrite) in cases {
        let (token, response) = answer(&operators, kept.clone(), origin, rewrite).await;
        refused.push(failure(
            operators
                .finish_sign_in(&party, &token, &response, &actor())
                .await
                .map(drop),
        ));
    }
    assert_eq!(refused, vec![not_signing_in(); 6]);
    let kept_credential = "SELECT count(*) FROM identity.outbox_global
                           WHERE payload ->> 'reason' = 'malformed' AND payload ->> 'credential' IS NOT NULL";
    assert_eq!(
        number(&database, kept_credential).await,
        1,
        "an answer with no user handle still names its credential"
    );
    let reasons = "handle_mismatch not_verified malformed malformed malformed malformed";
    let sessions = number(&database, "SELECT count(*) FROM identity.sessions").await;
    assert_eq!(
        (sessions, sign_ins(&database).await),
        (
            0,
            reasons
                .split(' ')
                .map(|reason| json!(["refused", reason]))
                .collect()
        )
    );
}

/// A credential ID of exactly the 1023 bytes `WebAuthn` allows is looked up, and recorded as unknown with the ID;
/// one byte more is malformed, recorded without it.
#[tokio::test]
async fn a_credential_id_of_the_longest_kind_is_looked_up() {
    let (operators, database, _, kept) = enrolled().await;
    for length in [1364, 1366] {
        let (token, response) = answer(&operators, kept.clone(), ORIGIN, |answer| {
            answer["id"] = json!("A".repeat(length));
            answer["rawId"] = json!("A".repeat(length));
        })
        .await;
        let _ = operators
            .finish_sign_in(&party(), &token, &response, &actor())
            .await;
    }
    let named = "SELECT count(*) FROM identity.outbox_global
                 WHERE event_type = 'identity.signed_in' AND payload ->> 'credential' IS NOT NULL";
    assert_eq!(
        (sign_ins(&database).await, number(&database, named).await),
        (
            vec![
                json!(["refused", "unknown_credential"]),
                json!(["refused", "malformed"])
            ],
            1
        )
    );
}

/// A sign-in whose device did not verify the user is refused, and recorded as `not_verified`: `webauthn-rs` checks
/// it before the signature, so anyone could claim it.
#[tokio::test]
async fn a_sign_in_that_does_not_verify_the_user_is_refused() {
    let (operators, database, _, kept) = enrolled().await;
    let party = party();
    let ceremony = operators.begin_sign_in(&party).await.expect("begins");
    let mut options: Value = serde_json::from_str(&ceremony.options).expect("JSON");
    options["publicKey"]["userVerification"] = json!("discouraged");
    let response = authenticate(&options.to_string(), ORIGIN, kept, false).await;
    let refused = failure(
        operators
            .finish_sign_in(&party, &ceremony.token.to_text(), &response, &actor())
            .await
            .map(drop),
    );
    assert_eq!(
        (refused, sign_ins(&database).await),
        (not_signing_in(), vec![json!(["refused", "not_verified"])])
    );
}

/// A synced passkey's counter stays at 0, so two sign-ins in a row both succeed; a counter that moves forward is
/// stored; one that goes back, as a cloned authenticator's might, is refused and recorded as `counter_regressed`,
/// which `webauthn-rs` checks after the signature (ADR-0047, `WebAuthn` L3 §6.1.1). A changed backup eligibility
/// is refused too, recorded as `not_verified`, since it is checked before the signature.
#[tokio::test]
async fn counters_move_forward_and_a_counter_gone_back_is_refused_and_recorded() {
    let (operators, database, _, kept) = enrolled().await;
    let party = party();
    let (mut ahead, mut further) = (kept.clone(), kept.clone());
    ahead.counter = Some(5);
    further.counter = Some(50);
    let flip = "UPDATE identity.passkeys SET passkey = jsonb_set(passkey, '{cred,backup_eligible}',
                to_jsonb(NOT (passkey #>> '{cred,backup_eligible}')::boolean))";
    for (passkey, change) in [
        (kept.clone(), ""),
        (kept.clone(), ""),
        (ahead, ""),
        (kept, ""),
        (further, flip),
    ] {
        as_admin(&database, change).await;
        let (token, response) = answer(&operators, passkey, ORIGIN, |_| {}).await;
        let _ = operators
            .finish_sign_in(&party, &token, &response, &actor())
            .await;
    }
    assert_eq!(
        sign_ins(&database).await,
        [
            json!(["changed", null]),
            json!(["changed", null]),
            json!(["changed", null]),
            json!(["refused", "counter_regressed"]),
            json!(["refused", "not_verified"])
        ]
    );
    let counter = "SELECT (passkey #>> '{cred,counter}')::bigint FROM identity.passkeys";
    assert_eq!(
        [
            number(&database, "SELECT count(*) FROM identity.sessions").await,
            number(&database, counter).await
        ],
        [3, 6]
    );
}

/// A disabled operator, or one whose passkeys were removed, signs in nowhere, and the record names the disabled
/// operator; nor does a ceremony token that is malformed or unknown.
#[tokio::test]
async fn a_disabled_or_removed_passkey_signs_in_nowhere() {
    let (operators, database, kurt, kept) = enrolled().await;
    let party = party();
    let mut refused = Vec::new();
    for change in ["disable", "remove"] {
        let (token, response) = answer(&operators, kept.clone(), ORIGIN, |_| {}).await;
        if change == "disable" {
            operators.disable(kurt, &actor()).await.expect("disables");
        } else {
            operators.enable(kurt, &actor()).await.expect("enables");
            operators
                .remove_passkeys(kurt, &actor())
                .await
                .expect("removes");
        }
        refused.push(failure(
            operators
                .finish_sign_in(&party, &token, &response, &actor())
                .await
                .map(drop),
        ));
    }
    for token in ["not a token".to_string(), "ab".repeat(32)] {
        refused.push(failure(
            operators
                .finish_sign_in(&party, &token, "{}", &actor())
                .await
                .map(drop),
        ));
    }
    assert_eq!(refused, vec![not_signing_in(); 4]);
    assert_eq!(
        sign_ins(&database).await,
        [
            json!(["refused", "disabled"]),
            json!(["refused", "unknown_credential"])
        ],
        "a token that finds no ceremony records nothing"
    );
    let named = "SELECT count(*) FROM identity.outbox_global
                 WHERE payload ->> 'reason' = 'disabled' AND payload ->> 'operator_id' = '1'";
    assert_eq!(
        number(&database, named).await,
        1,
        "a disabled operator is named"
    );
}

/// Each begin sweeps ceremonies that have ended, and past ten thousand open sign-ins it ends the oldest rather than
/// refusing, so filling the table locks no one out (ADR-0048, `v5.0.0-6.3.1`).
#[tokio::test]
async fn ended_sign_ins_are_swept_and_the_oldest_open_ones_give_way() {
    let (operators, database) = {
        let (operators, database, _, _) = with_link().await;
        (operators, database)
    };
    let fill = |count: u32, ends: &str| {
        format!(
            "INSERT INTO identity.ceremonies (token_hash, kind, state, ends_at)
             SELECT sha256(int8send(n)), 'sign_in', '{{}}', now() + interval '{ends}' + n * interval '1 ms'
             FROM generate_series(1, {count}) AS n"
        )
    };
    as_admin(&database, &fill(1000, "-1 minute")).await;
    assert!(operators.begin_sign_in(&party()).await.is_ok());
    let left = "SELECT count(*) FROM identity.ceremonies";
    assert_eq!(number(&database, left).await, 901);
    as_admin(
        &database,
        &format!(
            "DELETE FROM identity.ceremonies; {}",
            fill(10_000, "1 minute")
        ),
    )
    .await;
    assert!(operators.begin_sign_in(&party()).await.is_ok());
    assert_eq!(number(&database, left).await, 10_000);
    let oldest = "SELECT count(*) FROM identity.ceremonies WHERE token_hash = sha256(int8send(1))";
    assert_eq!(number(&database, oldest).await, 0, "the oldest gave way");
}

/// The signer the one stored passkey's credential ID names.
async fn stored_signer(database: &TestDatabase, store: &PostgresOperators) -> Signer {
    let credential: Vec<u8> = connect(database)
        .await
        .expect("connects")
        .query_one("SELECT credential_id FROM identity.passkeys", &[])
        .await
        .expect("reads")
        .try_get(0)
        .expect("bytes");
    store
        .signer(&credential)
        .await
        .expect("finds")
        .expect("a signer")
}

/// A stored passkey with one field of its credential changed.
fn changed(passkey: &str, field: &str, value: Value) -> String {
    let mut passkey: Value = serde_json::from_str(passkey).expect("JSON");
    passkey["cred"][field] = value;
    passkey.to_string()
}

/// Storing a sign-in refuses a passkey another sign-in changed meanwhile, a passkey that is now another operator's
/// or no longer exists, and an operator disabled since the passkey was read, and records each with its reason.
#[tokio::test]
async fn a_raced_disabled_or_missing_sign_in_starts_no_session() {
    let (operators, database, kurt, _) = enrolled().await;
    let store = PostgresOperators {
        pool: app_pool(&database),
    };
    let signer = stored_signer(&database, &store).await;
    // The passkey as another sign-in read it, before a counter this one never saw.
    let stale = Signer {
        passkey: changed(&signer.passkey, "counter", json!(7)),
        ..signer.clone()
    };
    let elsewhere = Signer {
        operator: OperatorId::new(2),
        ..signer.clone()
    };
    let missing = Signer {
        credential: vec![9; 16],
        ..signer.clone()
    };
    let mut answers = Vec::new();
    for (n, who) in [(1, &stale), (2, &elsewhere), (3, &signer), (4, &missing)] {
        if n == 3 {
            operators.disable(kurt, &actor()).await.expect("disables");
        }
        answers.push(store.sign_in(who, &who.passkey, &[n; 32], &actor()).await);
    }
    let reasons = ["raced", "unknown", "disabled", "unknown"];
    assert_eq!(
        (answers, sign_ins(&database).await),
        (
            vec![
                Ok(Some(SignIn::Raced)),
                Ok(None),
                Ok(Some(SignIn::Disabled)),
                Ok(None)
            ],
            reasons.map(|reason| json!(["refused", reason])).to_vec()
        )
    );
}

/// A sign-in stores a passkey's new state only over the very text it was read as: an old value that differs from
/// it only in how a number is written is another sign-in's, so it is raced, and nothing is respelt.
#[tokio::test]
async fn a_sign_in_never_respells_a_stored_number() {
    let (_, database, _, _) = enrolled().await;
    as_admin(
        &database,
        "UPDATE identity.passkeys SET passkey = jsonb_set(passkey, '{cred,x}', '1')",
    )
    .await;
    let store = PostgresOperators {
        pool: app_pool(&database),
    };
    let signer = stored_signer(&database, &store).await;
    let respelt = Signer {
        passkey: changed(&signer.passkey, "x", json!(1.0)),
        ..signer
    };
    assert_eq!(
        store
            .sign_in(&respelt, &respelt.passkey, &[6; 32], &actor())
            .await,
        Ok(Some(SignIn::Raced))
    );
    let kept = "SELECT count(*) FROM identity.passkeys WHERE passkey #>> '{cred,x}' = '1'";
    assert_eq!(number(&database, kept).await, 1);
}

/// The database cannot check a signature, so it refuses a new state that changes more of a passkey than a sign-in
/// may, as a fault of the caller: its key, a counter moved back, spelt as other than a whole number, or past what
/// an authenticator sends, or a flag that is not true or false. A caller with Identity's role cannot swap an
/// operator's key or leave a passkey `webauthn-rs` cannot read.
#[tokio::test]
async fn a_sign_in_that_changes_more_of_a_passkey_than_it_may_is_a_fault() {
    let (_, database, _, _) = enrolled().await;
    let store = PostgresOperators {
        pool: app_pool(&database),
    };
    let signer = stored_signer(&database, &store).await;
    let mut faults = Vec::new();
    for (field, value) in [
        ("cred_id", json!("AAAAAAAAAAAAAAAAAAAAAA")),
        ("counter", json!(-1)),
        ("counter", json!("6")),
        ("counter", json!(6.0)),
        ("counter", json!(4_294_967_296_u64)),
        ("counter", json!(1e30)),
        ("backup_state", json!("yes")),
        ("backup_state", Value::Null),
        ("backup_eligible", json!("on")),
        ("backup_eligible", Value::Null),
    ] {
        let new = changed(&signer.passkey, field, value);
        faults.push(failure(
            store
                .sign_in(&signer, &new, &[4; 32], &actor())
                .await
                .map(drop),
        ));
    }
    assert_eq!(
        faults,
        vec!["storage failed: a sign-in changed more of a passkey than it may".to_string(); 10]
    );
}

/// Failures that are not the browser's are told apart: no random source as a sign-in begins or a session starts,
/// a stored ceremony or passkey that cannot be read, and a stored user handle that is not 16 bytes.
#[tokio::test]
async fn failures_of_the_server_are_told_apart() {
    let (operators, database, _, kept) = enrolled().await;
    let party = party();
    let failing = Operators::with_fill(
        PostgresOperators {
            pool: app_pool(&database),
        },
        |_| Err(getrandom::Error::UNSUPPORTED),
    );
    let unavailable = format!(
        "temporarily unavailable: no secure random source: {}",
        getrandom::Error::UNSUPPORTED
    );
    assert_eq!(
        failure(failing.begin_sign_in(&party).await.map(drop)),
        unavailable
    );
    let (token, response) = answer(&operators, kept.clone(), ORIGIN, |_| {}).await;
    assert_eq!(
        failure(
            failing
                .finish_sign_in(&party, &token, &response, &actor())
                .await
                .map(drop)
        ),
        unavailable
    );
    let mut said = Vec::new();
    for corrupt in [
        "UPDATE identity.ceremonies SET state = '{\"x\": 1}'",
        "UPDATE identity.passkeys SET passkey = '{\"x\": 1}'",
    ] {
        let (token, response) = answer(&operators, kept.clone(), ORIGIN, |_| {}).await;
        as_admin(&database, corrupt).await;
        said.push(failure(
            operators
                .finish_sign_in(&party, &token, &response, &actor())
                .await
                .map(drop),
        ));
    }
    assert!(said[0].starts_with("storage failed: a stored sign-in ceremony could not be read"));
    assert!(said[1].starts_with("storage failed: a stored passkey could not be read"));
}

/// Each function replaced, what it returns instead, and what it answers.
const UNREADABLE: [(&str, &str, &str); 11] = [
    (
        "take_ceremony(bytea, text)",
        "TABLE (operator_id bigint, link_hash bytea, state integer)",
        "SELECT null::bigint, null::bytea, 1",
    ),
    (
        "signer(bytea)",
        "TABLE (operator_id text, user_handle bytea, passkey text, disabled boolean)",
        "SELECT '1', uuid_send(gen_random_uuid()), '{}', false",
    ),
    (
        "signer(bytea)",
        "TABLE (operator_id bigint, user_handle text, passkey text, disabled boolean)",
        "SELECT 1::bigint, 'x', '{}', false",
    ),
    (
        "signer(bytea)",
        "TABLE (operator_id bigint, user_handle bytea, passkey integer, disabled boolean)",
        "SELECT 1::bigint, uuid_send(gen_random_uuid()), 1, false",
    ),
    (
        "signer(bytea)",
        "TABLE (operator_id bigint, user_handle bytea, passkey text, disabled boolean)",
        "SELECT 1::bigint, '\\x00'::bytea, '{}', false",
    ),
    (
        "signer(bytea)",
        "TABLE (operator_id bigint, user_handle bytea, passkey text, disabled text)",
        "SELECT 1::bigint, uuid_send(gen_random_uuid()), '{}', 'x'",
    ),
    (
        "sign_in(bytea, bigint, jsonb, jsonb, bytea)",
        "TABLE (signed text, session_id text)",
        "SELECT 'maybe', 'x'",
    ),
    (
        "sign_in(bytea, bigint, jsonb, jsonb, bytea)",
        "TABLE (signed integer, session_id text)",
        "SELECT 1, 'x'",
    ),
    (
        "sign_in(bytea, bigint, jsonb, jsonb, bytea)",
        "TABLE (signed text, session_id integer)",
        "SELECT 'signed_in', 1",
    ),
    (
        "sign_in(bytea, bigint, jsonb, jsonb, bytea)",
        "TABLE (signed text, session_id text)",
        "SELECT 'signed_in', 'x' WHERE false",
    ),
    (
        "sign_in(bytea, bigint, jsonb, jsonb, bytea)",
        "TABLE (signed text, session_id text)",
        "SELECT 'signed_in', null::text",
    ),
];

/// Answers from storage that cannot be read are storage failures, never refusals: a sign-in's stored state of the
/// wrong type, a signer's columns of the wrong type or a user handle that is not 16 bytes, and a sign-in answer
/// the database never writes or of the wrong type.
#[tokio::test]
async fn unreadable_answers_are_storage_failures() {
    let mut said = Vec::new();
    for (function, returns, body) in UNREADABLE {
        let (operators, database, _, kept) = enrolled().await;
        let (token, response) = answer(&operators, kept, ORIGIN, |_| {}).await;
        replace(&database, function, returns, body).await;
        said.push(failure(
            operators
                .finish_sign_in(&party(), &token, &response, &actor())
                .await
                .map(drop),
        ));
    }
    assert!(
        said.iter().all(|said| said.starts_with("storage failed: ")),
        "{said:?}"
    );
    assert_eq!(said[4], "storage failed: a user handle is not 16 bytes");
    assert_eq!(said[6], "storage failed: signing in answered \"maybe\"");
    assert_eq!(
        said[10],
        "storage failed: a lookup that acted for a session named none"
    );
    assert_eq!(
        said[9],
        "storage failed: a lookup that always answers answered nothing"
    );
}

/// Each step of a sign-in fails as storage when the database refuses its function.
#[tokio::test]
async fn each_sign_in_step_fails_as_storage_when_refused() {
    let mut said = Vec::new();
    for function in [
        "begin_sign_in(bytea, jsonb)",
        "take_ceremony(bytea, text)",
        "signer(bytea)",
        "sign_in(bytea, bigint, jsonb, jsonb, bytea)",
    ] {
        let (operators, database, _, kept) = enrolled().await;
        let revoke = format!("REVOKE EXECUTE ON FUNCTION identity.{function} FROM tetsu_identity");
        if function.starts_with("begin") {
            as_admin(&database, &revoke).await;
            said.push(failure(operators.begin_sign_in(&party()).await.map(drop)));
        } else {
            let (token, response) = answer(&operators, kept, ORIGIN, |_| {}).await;
            as_admin(&database, &revoke).await;
            said.push(failure(
                operators
                    .finish_sign_in(&party(), &token, &response, &actor())
                    .await
                    .map(drop),
            ));
        }
    }
    let expected: Vec<String> = ["begin_sign_in", "take_ceremony", "signer", "sign_in"]
        .iter()
        .map(|name| format!("storage failed: permission denied for function {name}"))
        .collect();
    assert_eq!(said, expected);
}

/// A refusal that cannot be recorded is a storage failure, never a silent refusal.
#[tokio::test]
async fn a_refusal_that_cannot_be_recorded_fails_as_storage() {
    let (operators, database, _, kept) = enrolled().await;
    let (token, response) = answer(&operators, kept, ORIGIN, |answer| {
        answer["response"]["userHandle"] = json!("AAAAAAAAAAAAAAAAAAAAAA");
    })
    .await;
    as_admin(
        &database,
        "REVOKE INSERT ON identity.outbox_global FROM tetsu_identity",
    )
    .await;
    assert_eq!(
        failure(
            operators
                .finish_sign_in(&party(), &token, &response, &actor())
                .await
                .map(drop)
        ),
        "storage failed: permission denied for table outbox_global"
    );
}
