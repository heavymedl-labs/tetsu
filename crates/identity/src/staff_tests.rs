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

use crate::{AccountId, Accounts, Answer, Error, Identity, Realm, Session, SessionToken};

/// The business Ada is invited to.
const BUSINESS: i64 = 3;

/// Staff's use cases over a fresh migrated database, with Ada Park invited to the business as its owner, and her
/// link.
async fn invited() -> (
    Accounts<impl crate::ports::StaffStore>,
    TestDatabase,
    String,
) {
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

/// Ada Park's invite to the business as its owner.
fn ada() -> crate::NewStaffInvite {
    crate::NewStaffInvite {
        business: crate::BusinessId::new(BUSINESS),
        name: crate::PersonName::parse("Ada Park").expect("a name"),
        email: tetsu_kernel::Address::parse("ada@example.com").expect("an address"),
        role: crate::Role::Owner,
    }
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
    assert_eq!(
        account.email,
        tetsu_kernel::Address::parse("ada@example.com")
    );
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
    let issue = || async { staff.invite_staff(&ada(), &actor()).await.expect("issues") };
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

/// Inviting needs a secure random source and storage: without either it fails, saying which, and invites no one.
#[tokio::test]
async fn an_invite_without_randomness_or_storage_invites_no_one() {
    let database = migrated_database(&[&Identity])
        .await
        .expect("a migrated test database");
    let invite = |staff: Accounts<PostgresStaff>| async move {
        staff
            .invite_staff(&ada(), &actor())
            .await
            .map(drop)
            .map_err(|error| error.to_string())
    };
    let failing = Accounts::with_fill(PostgresStaff::new(app_pool(&database)), |_| {
        Err(getrandom::Error::UNSUPPORTED)
    });
    assert_eq!(
        invite(failing).await,
        Err(format!(
            "temporarily unavailable: no secure random source: {}",
            getrandom::Error::UNSUPPORTED
        ))
    );
    as_db_admin(
        &database,
        "REVOKE EXECUTE ON FUNCTION identity.staff_issue_invite(bytea, text, text, text) FROM tetsu_identity",
    )
    .await;
    assert_eq!(
        invite(Accounts::new(PostgresStaff::new(app_pool(&database)))).await,
        Err("storage failed: permission denied for function staff_issue_invite".to_string())
    );
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.staff").await,
        0
    );
}

/// A staff member who holds a role at the business, or is disabled, is refused by name, in this one build, so its
/// copy of the use case covers every path.
#[tokio::test]
async fn an_invite_to_a_member_or_a_disabled_staff_member_is_refused() {
    let (staff, database, _) = invited().await;
    let (ada, by) = (ada(), actor());
    let invite = || staff.invite_staff(&ada, &by);
    as_db_admin(
        &database,
        "INSERT INTO identity.staff_roles (tenant_id, staff_id, role_name) SELECT 3, id, 'owner' FROM identity.staff",
    )
    .await;
    assert_eq!(
        invite().await.map(drop),
        Err(Error::Invalid(
            "the staff member at ada@example.com holds a role at this business already".to_string()
        ))
    );
    as_db_admin(&database, "UPDATE identity.staff SET disabled_at = now()").await;
    assert_eq!(
        invite().await.map(drop),
        Err(Error::Invalid(
            "the staff member at ada@example.com is disabled".to_string()
        ))
    );
}

/// Each realm's passkey cap, as its pages show it, is the one its database enforces.
#[tokio::test]
async fn each_realms_passkey_cap_is_the_databases() {
    let database = migrated_database(&[&Identity])
        .await
        .expect("a migrated test database");
    for realm in [Realm::Admin, Realm::Staff] {
        let enforced = number(
            &database,
            &format!("SELECT identity.passkey_cap('{}')", realm.as_str()),
        )
        .await;
        assert_eq!(
            i64::try_from(realm.passkey_cap()).expect("a small cap"),
            enforced,
            "{realm:?}"
        );
    }
}

/// Staff's router serves staff's pages at its host's root, and none of the admins', not even the mailbox, which it
/// answers as any address it does not serve.
#[tokio::test]
async fn staffs_router_serves_staff_pages_alone() {
    let (staff, _database, link) = invited().await;
    let (_, kept) = join(&staff, &link).await;
    let session = cookie(&log_in(&staff, kept).await);
    let address = crate::http_tests::serve(staff, crate::http_tests::sample_mailbox()).await;
    let login = crate::http_tests::get(address, "/login", "").await;
    assert_eq!(login.status, 200);
    assert!(
        login
            .body
            .contains("Ask the business&#39;s owner for an invite."),
        "{}",
        login.body
    );
    let profile = crate::http_tests::get(address, "/settings/profile", "").await;
    assert_eq!(
        (profile.status, profile.header("location")),
        (303, Some("/login"))
    );
    let mailbox = crate::http_tests::get(address, "/mailbox", "").await;
    assert_eq!(
        (mailbox.status, mailbox.header("location")),
        (303, Some("/login?next=/mailbox"))
    );
    // Logged in, staff see their own pages, and the admins' mailbox is not found, as any address staff do not have.
    let own = crate::http_tests::get(address, "/settings/profile", &session).await;
    assert_eq!(own.status, 200, "{}", own.body);
    let admins = crate::http_tests::get(address, "/mailbox", &session).await;
    assert_eq!(admins.status, 404, "{}", admins.body);
    assert!(!admins.body.contains("Mailbox"), "{}", admins.body);
}

/// An invite's link says, before anyone acts on it, which business and role it gives and whether its person has
/// joined already, so the page asks someone new to make a passkey and someone else to log in and accept.
#[tokio::test]
async fn an_invites_link_says_whom_it_asks_to_join_or_accept() {
    let (staff, _database, link) = invited().await;
    let invitation = staff.invitation(&link).await.expect("reads").expect("open");
    assert_eq!(
        (invitation.business, invitation.role, invitation.joined),
        (crate::BusinessId::new(BUSINESS), crate::Role::Owner, false)
    );
    join(&staff, &link).await;
    assert_eq!(staff.invitation(&link).await.expect("reads"), None, "spent");
    assert_eq!(staff.invitation("not a link").await.expect("reads"), None);
}

/// Ada, joined at the business and freshly logged in, with her session and a second invite, to business 4.
async fn invited_again() -> (
    Accounts<impl crate::ports::StaffStore>,
    TestDatabase,
    Session,
    String,
) {
    let (staff, database, link) = invited().await;
    let (_, kept) = join(&staff, &link).await;
    let token = log_in(&staff, kept).await;
    let session = staff
        .session([cookie(&token).as_str()])
        .await
        .expect("reads")
        .expect("a live session");
    let second = staff
        .invite_staff(
            &crate::NewStaffInvite {
                business: crate::BusinessId::new(4),
                ..ada()
            },
            &actor(),
        )
        .await
        .expect("issues");
    (staff, database, session, second.token.to_text())
}

/// Someone with a passkey, invited to a second business, accepts by their fresh session: the role is added there,
/// the invite is spent, and that business's record says so.
#[tokio::test]
async fn a_staff_member_accepts_an_invite_to_another_business() {
    let (staff, database, session, link) = invited_again().await;
    let invitation = staff.invitation(&link).await.expect("reads").expect("open");
    assert!(invitation.joined);
    let by = actor();
    let accepted = staff.accept_invite(&session, &link, &by).await;
    assert_eq!(accepted, Ok(crate::Acceptance::Accepted));
    assert_eq!(
        number(
            &database,
            "SELECT count(*) FROM identity.staff_roles WHERE tenant_id = 4"
        )
        .await,
        1
    );
    let again = staff.accept_invite(&session, &link, &by).await;
    assert_eq!(again, Ok(crate::Acceptance::Expired));
    let not_a_link = staff.accept_invite(&session, "not a link", &by).await;
    assert_eq!(not_a_link, Ok(crate::Acceptance::Expired));
    // The business records its acceptance; a link with no open invite names no business, so Tetsu's own record
    // keeps those refusals, as it keeps joining's. Text that is no link at all never reaches storage.
    let mut accepted = events(&database, "outbox", "identity.invite_accepted").await;
    accepted.extend(events(&database, "outbox_global", "identity.invite_accepted").await);
    let said: Vec<_> = accepted
        .iter()
        .map(|event| (&event["outcome"], &event["reason"], &event["staff_id"]))
        .collect();
    let ada = json!(session.admin().get());
    let expired = (&json!("refused"), &json!("expired"), &ada);
    assert_eq!(said, [(&json!("changed"), &json!(null), &ada), expired]);
}

/// The link of an invite to business 4, as its owner, for the person with this name and address.
async fn issued(
    staff: &Accounts<impl crate::ports::StaffStore>,
    name: &str,
    email: &str,
) -> String {
    let invite = crate::NewStaffInvite {
        business: crate::BusinessId::new(4),
        name: crate::PersonName::parse(name).expect("a name"),
        email: tetsu_kernel::Address::parse(email).expect("an address"),
        role: crate::Role::Owner,
    };
    let issue = staff.invite_staff(&invite, &actor()).await.expect("issues");
    issue.token.to_text()
}

/// Logging in to accept, as the join page does for someone with a passkey: the login gives a fresh session, and the
/// invite is accepted by it in the same step. Accepting an invite meant for someone else leaves it open.
#[tokio::test]
async fn logging_in_accepts_an_invite_in_the_same_step() {
    let (staff, database, link) = invited().await;
    let (_, kept) = join(&staff, &link).await;
    let by = actor();
    let hers = issued(&staff, "Ada Park", "ada@example.com").await;
    let his = issued(&staff, "Bo Lin", "bo@example.com").await;
    let party = party();
    for (link, expected) in [
        (his.clone(), crate::Acceptance::Other),
        (hers, crate::Acceptance::Accepted),
    ] {
        let ceremony = staff.begin_login(&party).await.expect("begins");
        let response = authenticate(&ceremony.options, ORIGIN, kept.clone(), true).await;
        let answer = Answer {
            ceremony: &ceremony.token.to_text(),
            response: &response,
        };
        let (token, accepted) = staff
            .log_in_to_accept(
                &party,
                crate::Accepting {
                    answer,
                    link: &link,
                },
                &[],
                &by,
            )
            .await
            .expect("logs in");
        assert_eq!(accepted, expected);
        let session = staff
            .session([cookie(&token).as_str()])
            .await
            .expect("reads");
        assert!(session.is_some(), "logged in either way");
    }
    let open = staff.invitation(&his).await.expect("reads");
    assert!(open.is_some(), "left open");
    assert_eq!(
        number(
            &database,
            "SELECT count(*) FROM identity.staff_roles WHERE tenant_id = 4"
        )
        .await,
        1
    );
}

/// Logs in through staff's accept endpoints, as the join page's script does for someone with a passkey, sending the
/// link with both halves, and returns how finishing answered.
async fn accept_by_page(
    address: std::net::SocketAddr,
    kept: Passkey,
    link: &str,
) -> crate::http_tests::Reply {
    let post = crate::http_tests::post;
    let begun = post(address, "/join/accept/begin", "", &json!({ "link": link }))
        .await
        .json();
    let response = authenticate(&begun["options"].to_string(), ORIGIN, kept, true).await;
    let response: Value = serde_json::from_str(&response).expect("JSON");
    let finish = json!({"link": link, "ceremony": begun["ceremony"], "response": response});
    post(address, "/join/accept/finish", "", &finish).await
}

/// Through staff's pages: an invite's link reads as the business, by name, and role it gives, then whether its
/// person has joined; someone who has logs in to accept in one step, and lands on their Profile, logged in.
#[tokio::test]
async fn a_staff_member_joins_then_logs_in_to_accept_through_the_pages() {
    let (staff, _database, link) = invited().await;
    let (_, kept) = join(&staff, &link).await;
    let second = issued(&staff, "Ada Park", "ada@example.com").await;
    let address = crate::http_tests::serve_staff(staff).await;
    let read = |link: String| async move {
        crate::http_tests::post(address, "/join/invitation", "", &json!({ "link": link })).await
    };
    let invitation = read(second.clone()).await;
    assert_eq!(
        (invitation.status, invitation.json()),
        (
            200,
            json!({"business": "Fern Studio", "role": "owner", "joined": true})
        )
    );
    let accepted = accept_by_page(address, kept.clone(), &second).await;
    assert_eq!(
        (accepted.status, accepted.json()),
        (200, json!({"next": "/settings/profile"}))
    );
    assert!(
        accepted
            .header("set-cookie")
            .is_some_and(|set| set.starts_with("__Host-tetsu-staff="))
    );
    let again = accept_by_page(address, kept, &second).await;
    assert_eq!(again.status, 400);
    assert_eq!(
        again.json()["error"],
        json!("this invite has ended. Log in, or ask the business's owner for a new one")
    );
    assert!(
        again.header("set-cookie").is_some(),
        "logged in all the same"
    );
    for gone in [link, second, "not a link".to_string()] {
        assert_eq!(read(gone).await.status, 400);
    }
}

/// Staff's join page says their invites last seven days, and names the element its island fills with what the
/// invite gives; the admins' says theirs last an hour, and names none.
#[tokio::test]
async fn staffs_join_page_names_where_the_invite_is_said() {
    let (staff, _database, _) = invited().await;
    let address = crate::http_tests::serve_staff(staff).await;
    let page = crate::http_tests::get(address, "/join", "").await;
    assert_eq!(page.status, 200);
    assert!(page.body.contains(r#"id="invitation""#), "{}", page.body);
    assert!(
        page.body
            .contains("This invite works once, within seven days."),
        "{}",
        page.body
    );
    assert!(
        page.body.contains(r#"invitation="invitation""#),
        "{}",
        page.body
    );
    let (admins, _admins_database, _) = crate::http_tests::serving().await;
    let page = crate::http_tests::get(admins, "/join", "").await;
    assert!(
        page.body
            .contains("This invite works once, within the hour.")
    );
    assert!(!page.body.contains("invitation="), "{}", page.body);
}

/// An invite whose business's name cannot be read is unavailable, and a login to accept that does not work is
/// refused, and counted as a failed login. Reading is held as a login's start is.
#[tokio::test]
async fn reading_or_logging_in_to_accept_can_fail() {
    let (staff, database, link) = invited().await;
    let address = crate::http_tests::serve_staff(staff).await;
    let post = crate::http_tests::post;
    let read = post(address, "/join/invitation", "", &json!({ "link": link })).await;
    assert_eq!(read.status, 503);
    let finish = json!({"link": link, "ceremony": "not one", "response": {}});
    let refused = post(address, "/join/accept/finish", "", &finish).await;
    assert_eq!(refused.status, 400);
    let failures = "SELECT count(*) FROM identity.limits WHERE kind = 'staff_login_failure'";
    assert_eq!(number(&database, failures).await, 1);
    // Reading spent a login's start, never a join's room.
    let joins = "SELECT count(*) FROM identity.limits WHERE kind = 'staff_join'";
    assert_eq!(number(&database, joins).await, 0);
    let begun = "SELECT count(*) FROM identity.limits WHERE kind = 'staff_login_begin'";
    assert_eq!(number(&database, begun).await, 1);
}

/// Storage that fails while an invite is read, while a fresh login's session is found, or while the invite is
/// accepted leaves logging in to accept unavailable.
#[tokio::test]
async fn logging_in_to_accept_is_unavailable_when_storage_fails() {
    for broken in [
        "REVOKE EXECUTE ON FUNCTION identity.staff_invitee(bytea) FROM tetsu_identity",
        "REVOKE EXECUTE ON FUNCTION identity.staff_use_session(bytea, text) FROM tetsu_identity",
        "REVOKE EXECUTE ON FUNCTION identity.staff_accept_invite(bytea, bytea) FROM tetsu_identity",
    ] {
        let (staff, database, link) = invited().await;
        let (_, kept) = join(&staff, &link).await;
        let second = issued(&staff, "Ada Park", "ada@example.com").await;
        let address = crate::http_tests::serve_staff(staff).await;
        as_db_admin(&database, broken).await;
        let accepted = accept_by_page(address, kept, &second).await;
        assert_eq!(accepted.status, 503, "{broken}");
        let read = crate::http_tests::post;
        let read = read(address, "/join/invitation", "", &json!({ "link": second })).await;
        let expected = if broken.contains("staff_invitee") {
            503
        } else {
            200
        };
        assert_eq!(read.status, expected, "{broken}");
    }
}

/// Reading an invite and logging in to accept it come only from staff's own site, as every step before login does.
#[tokio::test]
async fn reading_and_accepting_an_invite_come_from_the_site() {
    let (staff, _database, _) = invited().await;
    let address = crate::http_tests::serve_staff(staff).await;
    let paths = [
        "/join/invitation",
        "/join/accept/begin",
        "/join/accept/finish",
    ];
    crate::http_tests::refuses_other_sites(address, &paths).await;
}
