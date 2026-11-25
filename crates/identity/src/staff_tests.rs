//! Staff's login flow over Postgres, end to end, with passkey-rs's authenticator in place of a person's: the same
//! use cases as admins', over the staff store, whose statements are rendered from the admins' (ADR-0091), and whose
//! invites each business keeps (ADR-0089).

use serde_json::{Value, json};
use tetsu_postgres::connect;
use tetsu_postgres::test_support::{TestDatabase, migrated_database};

use crate::adapters::postgres::{PostgresStaff, hex};
use crate::domain::{Addition, Ending, InviteToken, NewPasskey};
use crate::join_tests::{
    ORIGIN, actor, app_pool, as_db_admin, authenticate, number, party, register_keeping,
};
use crate::ports::LoginStore;
use passkey::types::Passkey;

use crate::{AccountId, Accounts, Answer, Identity, Realm, SessionToken};

/// The business Ada is invited to.
const BUSINESS: i64 = 3;

/// Staff's use cases over a fresh migrated database, with Ada Park invited to the business as its owner, and her
/// link.
async fn invited() -> (Accounts<impl LoginStore>, TestDatabase, String) {
    let database = migrated_database(&[&Identity])
        .await
        .expect("a migrated test database");
    let link = InviteToken::generate(getrandom::fill).expect("a link");
    let hash = hex(&link.hash());
    as_db_admin(
        &database,
        &format!(
            "INSERT INTO identity.staff (name, email) VALUES ('Ada Park', 'ada@example.com');
             INSERT INTO identity.staff_invites (tenant_id, token_hash, staff_id, role_name, issued_at, ends_at)
               SELECT {BUSINESS}, decode('{hash}', 'hex'), id, 'owner', now(), now() + interval '7 days'
               FROM identity.staff"
        ),
    )
    .await;
    let staff = crate::postgres_staff_store(app_pool(&database));
    (staff, database, link.to_text())
}

/// The payload of each event of a kind in an outbox, oldest first.
async fn events(database: &TestDatabase, outbox: &str, kind: &str) -> Vec<Value> {
    connect(database)
        .await
        .expect("connects")
        .query(
            &format!(
                "SELECT payload::text FROM identity.{outbox} WHERE event_type = $1 ORDER BY occurred_at, id"
            ),
            &[&kind],
        )
        .await
        .expect("reads")
        .iter()
        .map(|row| serde_json::from_str(row.try_get::<_, &str>(0).expect("a payload")).expect("JSON"))
        .collect()
}

/// The cookie a browser sends back with a staff session's token.
fn cookie(token: &SessionToken) -> String {
    format!("{}={}", Realm::Staff.cookie_name(), token.to_text())
}

/// Ada joins by her link, as her browser would, and keeps her passkey to log in with.
async fn join(staff: &Accounts<impl LoginStore>, link: &str) -> (AccountId, Passkey) {
    let party = party();
    let ceremony = staff.begin_join(&party, link).await.expect("begins");
    let options: Value = serde_json::from_str(&ceremony.options).expect("JSON");
    assert_eq!(
        options["publicKey"]["user"]["name"],
        json!("Tetsu staff · Ada Park")
    );
    let (response, kept) = register_keeping(&ceremony.options, ORIGIN, true).await;
    let answer = Answer {
        ceremony: &ceremony.token.to_text(),
        response: &response,
    };
    let (ada, _) = staff
        .finish_join(&party, answer, &[], &actor())
        .await
        .expect("joins");
    (ada, kept)
}

/// Logs in with a kept passkey, as a browser presenting no session would.
async fn log_in(staff: &Accounts<impl LoginStore>, kept: Passkey) -> SessionToken {
    let party = party();
    let ceremony = staff.begin_login(&party).await.expect("begins");
    let response = authenticate(&ceremony.options, ORIGIN, kept, true).await;
    let answer = Answer {
        ceremony: &ceremony.token.to_text(),
        response: &response,
    };
    staff
        .finish_login(&party, answer, &[], &actor())
        .await
        .expect("logs in")
}

/// Ada joins by her link as staff: the options name her as staff, her passkey and session are stored, her invite is
/// used up and its role added at the business, and the business's own record says so, naming her as staff. Then she
/// logs in with her passkey, and her session names her account; nothing of the admins' is touched.
#[tokio::test]
async fn a_staff_member_joins_by_their_link_and_logs_in() {
    let (staff, database, link) = invited().await;
    let (ada, kept) = join(&staff, &link).await;
    let held = "SELECT (SELECT count(*) FROM identity.staff_passkeys)
                     + 10 * (SELECT count(*) FROM identity.staff_sessions)
                     + 100 * (SELECT count(*) FROM identity.staff_invites)
                     + 1000 * (SELECT count(*) FROM identity.staff_roles WHERE tenant_id = 3)";
    assert_eq!(
        number(&database, held).await,
        1011,
        "a passkey, a session, a role, and no invite"
    );
    let joined = events(&database, "outbox", "identity.invite_redeemed").await;
    assert_eq!(
        (joined.len(), &joined[0]["staff_id"], &joined[0]["outcome"]),
        (1, &json!(ada.get()), &json!("changed")),
        "the business records the join"
    );
    let theirs = "SELECT count(*) FROM identity.outbox WHERE tenant_id = 3
                  AND event_type = 'identity.invite_redeemed'";
    assert_eq!(number(&database, theirs).await, 1, "in its own outbox");
    assert_eq!(
        events(&database, "outbox_global", "identity.invite_redeemed").await,
        Vec::<Value>::new()
    );

    let token = log_in(&staff, kept).await;
    let session = staff
        .session([cookie(&token).as_str()])
        .await
        .expect("reads")
        .expect("a live session");
    let account = staff
        .account(&session)
        .await
        .expect("reads")
        .expect("an account");
    assert_eq!((account.admin, account.name.as_str()), (ada, "Ada Park"));
    let logins = events(&database, "outbox_global", "identity.logged_in").await;
    assert_eq!(
        (logins.len(), &logins[0]["staff_id"], &logins[0]["admin_id"]),
        (1, &json!(ada.get()), &Value::Null)
    );
    let admins = "SELECT (SELECT count(*) FROM identity.admin_sessions) + (SELECT count(*) FROM identity.passkeys)";
    assert_eq!(number(&database, admins).await, 0);
}

/// A link to someone who has a passkey already joins no one: they log in and accept it instead.
#[tokio::test]
async fn a_link_to_someone_with_a_passkey_joins_no_one() {
    let (staff, database, link) = invited().await;
    as_db_admin(
        &database,
        "INSERT INTO identity.staff_passkeys (credential_id, staff_id, passkey, created_at)
           SELECT sha256('ada'::bytea), id, '{}', now() FROM identity.staff",
    )
    .await;
    assert_eq!(
        staff
            .begin_join(&party(), &link)
            .await
            .map(drop)
            .map_err(|error| error.to_string()),
        Err(
            "invalid: this invite has ended. Log in, or ask the business's owner for a new one"
                .to_string()
        )
    );
}

/// Begins Ada's join and registers her passkey, then has the database administrator run `meanwhile`, as a change
/// while she registers, and finishes it; says what failed, if anything.
async fn joined_after(
    staff: &Accounts<impl LoginStore>,
    database: &TestDatabase,
    link: &str,
    meanwhile: &str,
) -> Result<(), String> {
    let party = party();
    let ceremony = staff.begin_join(&party, link).await.expect("begins");
    let response = crate::join_tests::register(&ceremony.options, ORIGIN, true).await;
    as_db_admin(database, meanwhile).await;
    let answer = Answer {
        ceremony: &ceremony.token.to_text(),
        response: &response,
    };
    staff
        .finish_join(&party, answer, &[], &actor())
        .await
        .map(drop)
        .map_err(|error| error.to_string())
}

/// An invite used up or cancelled while its person registers joins no one: redeeming finds no business, and runs
/// where no business's rows are seen, so it stores nothing and is refused as an ended link, recorded for Tetsu.
#[tokio::test]
async fn an_invite_gone_before_its_join_finishes_joins_no_one() {
    let (staff, database, link) = invited().await;
    let gone = "DELETE FROM identity.staff_invites";
    assert!(joined_after(&staff, &database, &link, gone).await.is_err());
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.staff_passkeys").await,
        0
    );
    let refused = events(&database, "outbox_global", "identity.invite_redeemed").await;
    assert_eq!(
        (refused.len(), &refused[0]["reason"]),
        (1, &json!("expired"))
    );
}

/// A business that cannot be looked up fails the join as storage, storing nothing.
#[tokio::test]
async fn a_join_whose_business_cannot_be_found_fails_as_storage() {
    let (staff, database, link) = invited().await;
    let refused = "REVOKE EXECUTE ON FUNCTION identity.staff_invitee(bytea) FROM tetsu_identity";
    assert_eq!(
        joined_after(&staff, &database, &link, refused).await,
        Err("storage failed: permission denied for function staff_invitee".to_string())
    );
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.staff_passkeys").await,
        0
    );
}

/// A passkey as webauthn-rs stores one, as far as the account's list reads it: whether it may be, and is, synced.
const STORED: &str = r#"{"cred": {"backup_eligible": false, "backup_state": false}}"#;

/// Gives Ada a second passkey, then adds a third through her fresh session: more than an admin may hold, fewer
/// than staff's ten.
async fn add_a_third(
    store: &PostgresStaff,
    database: &TestDatabase,
    session: &[u8; 32],
    ada: AccountId,
) {
    as_db_admin(
        database,
        &format!(
            "INSERT INTO identity.staff_passkeys (credential_id, staff_id, passkey, created_at)
               SELECT sha256('second'::bytea), id, '{STORED}', now() FROM identity.staff"
        ),
    )
    .await;
    assert!(
        store.adder(session).await.expect("reads").is_some(),
        "two of ten"
    );
    let addition = [5; 32];
    assert!(
        store
            .begin_addition(&addition, session, ada, "{}")
            .await
            .expect("begins")
    );
    let held = store
        .take_addition(&addition, session)
        .await
        .expect("takes")
        .expect("an addition");
    let third = NewPasskey {
        credential_id: vec![9; 16],
        passkey: STORED.to_string(),
        transports: Vec::new(),
    };
    assert_eq!(
        store.add_passkey(&held, &third, &actor()).await,
        Ok(Some(Addition::Added))
    );
}

/// Every statement of staff's store runs against staff's own functions and tables, end to end from a real join and
/// login: the time left, the list of sessions, ending the others, adding a third passkey, which only staff's cap
/// of ten allows, and logging out, each recorded naming the staff member.
#[tokio::test]
async fn every_staff_statement_runs_on_staff_tables() {
    let (staff, database, link) = invited().await;
    let (ada, kept) = join(&staff, &link).await;
    let token = log_in(&staff, kept).await;
    let hash = token.hash();
    let store = PostgresStaff::new(app_pool(&database));
    assert!(store.session_left(&hash).await.expect("reads").is_some());
    assert_eq!(store.sessions_of(&hash).await.expect("lists").len(), 2);
    assert_eq!(
        store.end_other_sessions(&hash, None, &actor()).await,
        Ok(Ending::Ended(1))
    );
    add_a_third(&store, &database, &hash, ada).await;
    assert_eq!(store.delete_session(&hash, &actor()).await, Ok(true));
    let named = "SELECT count(*) FROM identity.outbox_global
                 WHERE event_type IN ('identity.other_sessions_ended', 'identity.passkey_added',
                                      'identity.logged_out')
                   AND (payload ->> 'staff_id')::bigint = (SELECT id FROM identity.staff)
                   AND NOT payload ? 'admin_id'";
    assert_eq!(number(&database, named).await, 3);
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.staff_passkeys").await,
        3
    );
}

/// A link `invite_staff` issues joins the person it names, and the link it replaced joins no one.
#[tokio::test]
async fn an_issued_invite_joins_its_staff_member_and_a_replaced_one_does_not() {
    let database = migrated_database(&[&Identity])
        .await
        .expect("a migrated test database");
    let staff = crate::postgres_staff_store(app_pool(&database));
    let issue = || async {
        staff
            .invite_staff(
                crate::BusinessId::new(BUSINESS),
                &crate::PersonName::parse("Ada Park").expect("a name"),
                &tetsu_kernel::Address::parse("ada@example.com").expect("an address"),
                crate::Role::Owner,
                &actor(),
            )
            .await
            .expect("issues")
    };
    let old = issue().await;
    let new = issue().await;
    assert!(
        staff
            .begin_join(&party(), &old.token.to_text())
            .await
            .is_err()
    );
    let (ada, _) = join(&staff, &new.token.to_text()).await;
    assert_eq!(ada, new.staff);
    let role =
        "SELECT count(*) FROM identity.staff_roles WHERE tenant_id = 3 AND role_name = 'owner'";
    assert_eq!(number(&database, role).await, 1);
}
