//! Logging an admin in with their passkey, end to end, with passkey-rs's authenticator in place of their device
//! (ADR-0058), against Postgres logged in as the application role. They live in this one build for the reason
//! join's do: llvm-cov scores each build of a generic use case on its own.

use passkey::types::Passkey;
use serde_json::{Value, json};
use tetsu_postgres::connect;
use tetsu_postgres::test_support::TestDatabase;

use crate::adapters::postgres::PostgresAdmins;
use crate::domain::{Login, SessionStart, Signer};
use crate::join_tests::{
    ORIGIN, actor, app_pool, as_db_admin, authenticate, failure, number, party, recorded,
    register_keeping, replace, with_link, without_randomness,
};
use crate::ports::AdminStore;
use crate::{AdminId, Admins, Answer, Realm};

/// How a hostile client rewrites a login's answer.
type Rewrite = fn(&mut Value);

fn not_logging_in() -> String {
    "invalid: this login didn't work. Try again".to_string()
}

/// Kurt Medley, admin 1, joined with a passkey, and the passkey his device keeps. Joining logs him in, but the
/// tests that use this begin from a passkey login, so the session joining began is logged out.
pub(crate) async fn joined() -> (Admins<PostgresAdmins>, TestDatabase, AdminId, Passkey) {
    let (admins, database, kurt, link) = with_link().await;
    let party = party();
    let ceremony = admins.begin_join(&party, &link).await.expect("begins");
    let (response, kept) = register_keeping(&ceremony.options, ORIGIN, true).await;
    admins
        .finish_join(
            &party,
            Answer {
                ceremony: &ceremony.token.to_text(),
                response: &response,
            },
            &[],
            &actor(),
        )
        .await
        .expect("joins");
    as_db_admin(&database, "DELETE FROM identity.sessions").await;
    (admins, database, kurt, kept)
}

/// Begins a login and has the device answer it from `origin`, with the answer rewritten by `rewrite`.
pub(crate) async fn answer(
    admins: &Admins<PostgresAdmins>,
    passkey: Passkey,
    origin: &str,
    rewrite: impl Fn(&mut Value),
) -> (String, String) {
    let ceremony = admins.begin_login(&party()).await.expect("begins");
    let mut response: Value =
        serde_json::from_str(&authenticate(&ceremony.options, origin, passkey, true).await)
            .expect("JSON");
    rewrite(&mut response);
    (ceremony.token.to_text(), response.to_string())
}

/// Finishes a login with an answer, from a browser presenting no session.
async fn finish(
    admins: &Admins<PostgresAdmins>,
    ceremony: &str,
    response: &str,
) -> Result<crate::SessionToken, crate::Error> {
    let answer = Answer { ceremony, response };
    admins.finish_login(&party(), answer, &[], &actor()).await
}

/// What each login recorded, oldest first: its outcome and reason.
async fn logins(database: &TestDatabase) -> Vec<Value> {
    recorded(database, "identity.logged_in").await
}

/// A joined admin logs in with their passkey: the options name no passkey and ask the browser to verify the
/// user with the prompt a button opens; the session starts, the passkey's last use is stored, and it is recorded.
/// The ceremony finishes once.
#[tokio::test]
async fn an_admin_logs_in_with_their_passkey() {
    let (admins, database, _, kept) = joined().await;
    let party = party();
    let ceremony = admins.begin_login(&party).await.expect("begins");
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
    let logged = finish(&admins, &token, &response).await;
    assert!(logged.is_ok());
    let used = "SELECT count(*) FROM identity.passkeys WHERE last_used_at IS NOT NULL";
    assert_eq!(
        [
            number(&database, "SELECT count(*) FROM identity.sessions").await,
            number(&database, used).await
        ],
        [1, 1]
    );
    assert_eq!(
        failure(finish(&admins, &token, &response).await.map(drop)),
        not_logging_in()
    );
    assert_eq!(logins(&database).await, [json!(["changed", null])]);
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

/// A login is refused, the same way each time, when its answer names another admin's user handle, comes from
/// another origin, names no user handle or a credential ID too long, or is not an answer at all (#109), and each is
/// recorded with its reason.
#[tokio::test]
async fn hostile_logins_are_refused() {
    let (admins, database, _, kept) = joined().await;
    let mut refused = Vec::new();
    let cases: [(&str, Rewrite); 6] = [
        (ORIGIN, |answer| {
            answer["response"]["userHandle"] = json!("AAAAAAAAAAAAAAAAAAAAAA");
        }),
        ("https://admin.example.com:8443", |_| {}),
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
        let (token, response) = answer(&admins, kept.clone(), origin, rewrite).await;
        refused.push(failure(finish(&admins, &token, &response).await.map(drop)));
    }
    assert_eq!(refused, vec![not_logging_in(); 6]);
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
        (sessions, logins(&database).await),
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
    let (admins, database, _, kept) = joined().await;
    for length in [1364, 1366] {
        let (token, response) = answer(&admins, kept.clone(), ORIGIN, |answer| {
            answer["id"] = json!("A".repeat(length));
            answer["rawId"] = json!("A".repeat(length));
        })
        .await;
        let _ = finish(&admins, &token, &response).await;
    }
    let named = "SELECT count(*) FROM identity.outbox_global
                 WHERE event_type = 'identity.logged_in' AND payload ->> 'credential' IS NOT NULL";
    assert_eq!(
        (logins(&database).await, number(&database, named).await),
        (
            vec![
                json!(["refused", "unknown_credential"]),
                json!(["refused", "malformed"])
            ],
            1
        )
    );
}

/// A login whose device did not verify the user is refused, and recorded as `not_verified`: `webauthn-rs` checks
/// it before the signature, so anyone could claim it.
#[tokio::test]
async fn a_login_that_does_not_verify_the_user_is_refused() {
    let (admins, database, _, kept) = joined().await;
    let party = party();
    let ceremony = admins.begin_login(&party).await.expect("begins");
    let mut options: Value = serde_json::from_str(&ceremony.options).expect("JSON");
    options["publicKey"]["userVerification"] = json!("discouraged");
    let response = authenticate(&options.to_string(), ORIGIN, kept, false).await;
    let refused = failure(
        finish(&admins, &ceremony.token.to_text(), &response)
            .await
            .map(drop),
    );
    assert_eq!(
        (refused, logins(&database).await),
        (not_logging_in(), vec![json!(["refused", "not_verified"])])
    );
}

/// Logging in ends the admin session the browser presented, whoever's it is, in the transaction that starts
/// the new one (ADR-0059, `v5.0.0-7.2.4`); a refused login leaves it as it was.
#[tokio::test]
async fn logging_in_ends_the_session_the_browser_presented() {
    let (admins, database, _, kept) = joined().await;
    let party = party();
    let earlier = "aa".repeat(32);
    as_db_admin(
        &database,
        &format!(
            "SELECT identity.start_session(sha256(decode('{earlier}', 'hex')), 'admin', 1, true)"
        ),
    )
    .await;
    let cookie = format!("{}={earlier}", Realm::Admin.cookie_name());
    let held = format!(
        "SELECT count(*) FROM identity.sessions WHERE token_hash = sha256(decode('{earlier}', 'hex'))"
    );
    let mut ended = Vec::new();
    for hostile in [true, false] {
        let (token, response) = answer(&admins, kept.clone(), ORIGIN, |answer| {
            if hostile {
                answer["response"]["userHandle"] = json!("AAAAAAAAAAAAAAAAAAAAAA");
            }
        })
        .await;
        let answer = Answer {
            ceremony: &token,
            response: &response,
        };
        let _ = admins
            .finish_login(&party, answer, &[cookie.as_str()], &actor())
            .await;
        ended.push(number(&database, &held).await);
    }
    assert_eq!(ended, [1, 0], "a refused login leaves it; a login ends it");
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.sessions").await,
        1
    );
}

/// A synced passkey's counter stays at 0, so two logins in a row both succeed; a counter that moves forward is
/// stored; one that goes back, as a cloned authenticator's might, is refused and recorded as `counter_regressed`,
/// which `webauthn-rs` checks after the signature (ADR-0047, `WebAuthn` L3 §6.1.1). A changed backup eligibility
/// is refused too, recorded as `not_verified`, since it is checked before the signature.
#[tokio::test]
async fn counters_move_forward_and_a_counter_gone_back_is_refused_and_recorded() {
    let (admins, database, _, kept) = joined().await;
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
        as_db_admin(&database, change).await;
        let (token, response) = answer(&admins, passkey, ORIGIN, |_| {}).await;
        let _ = finish(&admins, &token, &response).await;
    }
    assert_eq!(
        logins(&database).await,
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

/// A disabled admin, or one whose passkeys were removed, logs in nowhere, and the record names the disabled
/// admin; nor does a ceremony token that is malformed or unknown.
#[tokio::test]
async fn a_disabled_or_removed_passkey_logs_in_nowhere() {
    let (admins, database, kurt, kept) = joined().await;
    let mut refused = Vec::new();
    for change in ["disable", "remove"] {
        let (token, response) = answer(&admins, kept.clone(), ORIGIN, |_| {}).await;
        if change == "disable" {
            admins.disable(kurt, &actor()).await.expect("disables");
        } else {
            admins.enable(kurt, &actor()).await.expect("enables");
            admins
                .remove_passkeys(kurt, &actor())
                .await
                .expect("removes");
        }
        refused.push(failure(finish(&admins, &token, &response).await.map(drop)));
    }
    for token in ["not a token".to_string(), "ab".repeat(32)] {
        refused.push(failure(finish(&admins, &token, "{}").await.map(drop)));
    }
    assert_eq!(refused, vec![not_logging_in(); 4]);
    assert_eq!(
        logins(&database).await,
        [
            json!(["refused", "disabled"]),
            json!(["refused", "unknown_credential"])
        ],
        "a token that finds no ceremony records nothing"
    );
    let named = "SELECT count(*) FROM identity.outbox_global
                 WHERE payload ->> 'reason' = 'disabled' AND payload ->> 'admin_id' = '1'";
    assert_eq!(
        number(&database, named).await,
        1,
        "a disabled admin is named"
    );
}

/// Each begin sweeps ceremonies that have ended, and past ten thousand open logins it ends the oldest rather than
/// refusing, so filling the table locks no one out (ADR-0048, `v5.0.0-6.3.1`).
#[tokio::test]
async fn ended_logins_are_swept_and_the_oldest_open_ones_give_way() {
    let (admins, database) = {
        let (admins, database, _, _) = with_link().await;
        (admins, database)
    };
    let fill = |count: u32, ends: &str| {
        format!(
            "INSERT INTO identity.ceremonies (token_hash, kind, state, ends_at)
             SELECT sha256(int8send(n)), 'login', '{{}}', now() + interval '{ends}' + n * interval '1 ms'
             FROM generate_series(1, {count}) AS n"
        )
    };
    as_db_admin(&database, &fill(1000, "-1 minute")).await;
    assert!(admins.begin_login(&party()).await.is_ok());
    let left = "SELECT count(*) FROM identity.ceremonies";
    assert_eq!(number(&database, left).await, 901);
    as_db_admin(
        &database,
        &format!(
            "DELETE FROM identity.ceremonies; {}",
            fill(10_000, "1 minute")
        ),
    )
    .await;
    assert!(admins.begin_login(&party()).await.is_ok());
    assert_eq!(number(&database, left).await, 10_000);
    let oldest = "SELECT count(*) FROM identity.ceremonies WHERE token_hash = sha256(int8send(1))";
    assert_eq!(number(&database, oldest).await, 0, "the oldest gave way");
}

/// The signer the one stored passkey's credential ID names.
async fn stored_signer(database: &TestDatabase, store: &PostgresAdmins) -> Signer {
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

/// A session to start under `hash`, replacing none the browser presented.
fn start(hash: [u8; 32]) -> SessionStart {
    SessionStart {
        hash,
        replacing: None,
    }
}

/// A stored passkey with one field of its credential changed.
fn changed(passkey: &str, field: &str, value: Value) -> String {
    let mut passkey: Value = serde_json::from_str(passkey).expect("JSON");
    passkey["cred"][field] = value;
    passkey.to_string()
}

/// Storing a login refuses a passkey another login changed meanwhile, a passkey that is now another admin's
/// or no longer exists, and an admin disabled since the passkey was read, and records each with its reason.
#[tokio::test]
async fn a_raced_disabled_or_missing_login_starts_no_session() {
    let (admins, database, kurt, _) = joined().await;
    let store = PostgresAdmins {
        pool: app_pool(&database),
    };
    let signer = stored_signer(&database, &store).await;
    // The passkey as another login read it, before a counter this one never saw.
    let stale = Signer {
        passkey: changed(&signer.passkey, "counter", json!(7)),
        ..signer.clone()
    };
    let elsewhere = Signer {
        admin: AdminId::new(2),
        ..signer.clone()
    };
    let missing = Signer {
        credential: vec![9; 16],
        ..signer.clone()
    };
    let mut answers = Vec::new();
    for (n, who) in [(1, &stale), (2, &elsewhere), (3, &signer), (4, &missing)] {
        if n == 3 {
            admins.disable(kurt, &actor()).await.expect("disables");
        }
        answers.push(
            store
                .login(who, &who.passkey, &start([n; 32]), &actor())
                .await,
        );
    }
    let reasons = ["raced", "unknown", "disabled", "unknown"];
    assert_eq!(
        (answers, logins(&database).await),
        (
            vec![
                Ok(Some(Login::Raced)),
                Ok(None),
                Ok(Some(Login::Disabled)),
                Ok(None)
            ],
            reasons.map(|reason| json!(["refused", reason])).to_vec()
        )
    );
}

/// A login stores a passkey's new state only over the very text it was read as: an old value that differs from
/// it only in how a number is written is another login's, so it is raced, and nothing is respelt.
#[tokio::test]
async fn a_login_never_respells_a_stored_number() {
    let (_, database, _, _) = joined().await;
    as_db_admin(
        &database,
        "UPDATE identity.passkeys SET passkey = jsonb_set(passkey, '{cred,x}', '1')",
    )
    .await;
    let store = PostgresAdmins {
        pool: app_pool(&database),
    };
    let signer = stored_signer(&database, &store).await;
    let respelt = Signer {
        passkey: changed(&signer.passkey, "x", json!(1.0)),
        ..signer
    };
    assert_eq!(
        store
            .login(&respelt, &respelt.passkey, &start([6; 32]), &actor())
            .await,
        Ok(Some(Login::Raced))
    );
    let kept = "SELECT count(*) FROM identity.passkeys WHERE passkey #>> '{cred,x}' = '1'";
    assert_eq!(number(&database, kept).await, 1);
}

/// The database cannot check a signature, so it refuses a new state that changes more of a passkey than a login
/// may, as a fault of the caller: its key, a counter moved back, spelt as other than a whole number, or past what
/// an authenticator sends, or a flag that is not true or false. A caller with Identity's role cannot swap an
/// admin's key or leave a passkey `webauthn-rs` cannot read.
#[tokio::test]
async fn a_login_that_changes_more_of_a_passkey_than_it_may_is_a_fault() {
    let (_, database, _, _) = joined().await;
    let store = PostgresAdmins {
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
                .login(&signer, &new, &start([4; 32]), &actor())
                .await
                .map(drop),
        ));
    }
    assert_eq!(
        faults,
        vec!["storage failed: a login changed more of a passkey than it may".to_string(); 10]
    );
}

/// Failures that are not the browser's are told apart: no random source as a login begins or a session starts,
/// a stored ceremony or passkey that cannot be read, and a stored user handle that is not 16 bytes.
#[tokio::test]
async fn failures_of_the_server_are_told_apart() {
    let (admins, database, _, kept) = joined().await;
    let party = party();
    let (failing, unavailable) = without_randomness(&database);
    assert_eq!(
        failure(failing.begin_login(&party).await.map(drop)),
        unavailable
    );
    let (token, response) = answer(&admins, kept.clone(), ORIGIN, |_| {}).await;
    assert_eq!(
        failure(finish(&failing, &token, &response).await.map(drop)),
        unavailable
    );
    let mut said = Vec::new();
    for corrupt in [
        "UPDATE identity.ceremonies SET state = '{\"x\": 1}'",
        "UPDATE identity.passkeys SET passkey = '{\"x\": 1}'",
    ] {
        let (token, response) = answer(&admins, kept.clone(), ORIGIN, |_| {}).await;
        as_db_admin(&database, corrupt).await;
        said.push(failure(finish(&admins, &token, &response).await.map(drop)));
    }
    assert!(said[0].starts_with("storage failed: a stored login ceremony could not be read"));
    assert!(said[1].starts_with("storage failed: a stored passkey could not be read"));
}

/// Each function replaced, what it returns instead, and what it answers.
const UNREADABLE: [(&str, &str, &str); 11] = [
    (
        "take_ceremony(bytea, text)",
        "TABLE (admin_id bigint, link_hash bytea, state integer)",
        "SELECT null::bigint, null::bytea, 1",
    ),
    (
        "signer(bytea)",
        "TABLE (admin_id text, user_handle bytea, passkey text, disabled boolean)",
        "SELECT '1', uuid_send(gen_random_uuid()), '{}', false",
    ),
    (
        "signer(bytea)",
        "TABLE (admin_id bigint, user_handle text, passkey text, disabled boolean)",
        "SELECT 1::bigint, 'x', '{}', false",
    ),
    (
        "signer(bytea)",
        "TABLE (admin_id bigint, user_handle bytea, passkey integer, disabled boolean)",
        "SELECT 1::bigint, uuid_send(gen_random_uuid()), 1, false",
    ),
    (
        "signer(bytea)",
        "TABLE (admin_id bigint, user_handle bytea, passkey text, disabled boolean)",
        "SELECT 1::bigint, '\\x00'::bytea, '{}', false",
    ),
    (
        "signer(bytea)",
        "TABLE (admin_id bigint, user_handle bytea, passkey text, disabled text)",
        "SELECT 1::bigint, uuid_send(gen_random_uuid()), '{}', 'x'",
    ),
    (
        "login(bytea, bigint, jsonb, jsonb, bytea, bytea)",
        "TABLE (outcome text, session_id text)",
        "SELECT 'maybe', 'x'",
    ),
    (
        "login(bytea, bigint, jsonb, jsonb, bytea, bytea)",
        "TABLE (outcome integer, session_id text)",
        "SELECT 1, 'x'",
    ),
    (
        "login(bytea, bigint, jsonb, jsonb, bytea, bytea)",
        "TABLE (outcome text, session_id integer)",
        "SELECT 'logged_in', 1",
    ),
    (
        "login(bytea, bigint, jsonb, jsonb, bytea, bytea)",
        "TABLE (outcome text, session_id text)",
        "SELECT 'logged_in', 'x' WHERE false",
    ),
    (
        "login(bytea, bigint, jsonb, jsonb, bytea, bytea)",
        "TABLE (outcome text, session_id text)",
        "SELECT 'logged_in', null::text",
    ),
];

/// Answers from storage that cannot be read are storage failures, never refusals: a login's stored state of the
/// wrong type, a signer's columns of the wrong type or a user handle that is not 16 bytes, and a login answer
/// the database never writes or of the wrong type.
#[tokio::test]
async fn unreadable_answers_are_storage_failures() {
    let mut said = Vec::new();
    for (function, returns, body) in UNREADABLE {
        let (admins, database, _, kept) = joined().await;
        let (token, response) = answer(&admins, kept, ORIGIN, |_| {}).await;
        replace(&database, function, returns, body).await;
        said.push(failure(finish(&admins, &token, &response).await.map(drop)));
    }
    assert!(
        said.iter().all(|said| said.starts_with("storage failed: ")),
        "{said:?}"
    );
    assert_eq!(said[4], "storage failed: a user handle is not 16 bytes");
    assert_eq!(said[6], "storage failed: logging in answered \"maybe\"");
    assert_eq!(
        said[10],
        "storage failed: a lookup that acted for a session named none"
    );
    assert_eq!(
        said[9],
        "storage failed: a lookup that always answers answered nothing"
    );
}

/// Each step of a login fails as storage when the database refuses its function.
#[tokio::test]
async fn each_login_step_fails_as_storage_when_refused() {
    let mut said = Vec::new();
    for function in [
        "begin_login(bytea, jsonb)",
        "take_ceremony(bytea, text)",
        "signer(bytea)",
        "login(bytea, bigint, jsonb, jsonb, bytea, bytea)",
    ] {
        let (admins, database, _, kept) = joined().await;
        let revoke = format!("REVOKE EXECUTE ON FUNCTION identity.{function} FROM tetsu_identity");
        if function.starts_with("begin") {
            as_db_admin(&database, &revoke).await;
            said.push(failure(admins.begin_login(&party()).await.map(drop)));
        } else {
            let (token, response) = answer(&admins, kept, ORIGIN, |_| {}).await;
            as_db_admin(&database, &revoke).await;
            said.push(failure(finish(&admins, &token, &response).await.map(drop)));
        }
    }
    let expected: Vec<String> = ["begin_login", "take_ceremony", "signer", "login"]
        .iter()
        .map(|name| format!("storage failed: permission denied for function {name}"))
        .collect();
    assert_eq!(said, expected);
}

/// A refusal that cannot be recorded is a storage failure, never a silent refusal.
#[tokio::test]
async fn a_refusal_that_cannot_be_recorded_fails_as_storage() {
    let (admins, database, _, kept) = joined().await;
    let (token, response) = answer(&admins, kept, ORIGIN, |answer| {
        answer["response"]["userHandle"] = json!("AAAAAAAAAAAAAAAAAAAAAA");
    })
    .await;
    as_db_admin(
        &database,
        "REVOKE INSERT ON identity.outbox_global FROM tetsu_identity",
    )
    .await;
    assert_eq!(
        failure(finish(&admins, &token, &response).await.map(drop)),
        "storage failed: permission denied for table outbox_global"
    );
}
