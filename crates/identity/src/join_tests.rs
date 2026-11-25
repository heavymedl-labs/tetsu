//! Joining an admin's first passkey, end to end, with passkey-rs's authenticator in place of a person's
//! (ADR-0058), against Postgres logged in as the application role. They live in this one build because llvm-cov
//! scores each build of a generic use case on its own, and only here can a test make the random source fail.

use std::future::Future;
use std::pin::Pin;

use passkey::authenticator::{Authenticator, UiHint, UserCheck, UserValidationMethod};
use passkey::client::{Client, DefaultClientData};
use passkey::types::Passkey;
use passkey::types::ctap2::{Aaguid, Ctap2Error};
use passkey::types::webauthn::{CredentialCreationOptions, CredentialRequestOptions};
use serde_json::{Value, json};
use tetsu_postgres::test_support::{TestDatabase, migrated_database};
use tetsu_postgres::{Pool, connect, pool};
use webauthn_rs::prelude::Url;

use sha2::{Digest, Sha256};

use crate::adapters::postgres::PostgresAdmins;
use crate::domain::{NewPasskey, Redemption, SessionStart, SessionToken, TakenCeremony};
use crate::ports::LoginStore;
use crate::{AccountId, Accounts, Actor, Answer, Error, Identity, PersonName, RelyingParty};

pub(crate) const ORIGIN: &str = "https://admin.example.com";

/// Identity over `database` with no secure random source, and what it says when it needs one.
pub(crate) fn without_randomness(database: &TestDatabase) -> (Accounts<PostgresAdmins>, String) {
    let failing = Accounts::with_fill(PostgresAdmins::new(app_pool(database)), |_| {
        Err(getrandom::Error::UNSUPPORTED)
    });
    let unavailable = format!(
        "temporarily unavailable: no secure random source: {}",
        getrandom::Error::UNSUPPORTED
    );
    (failing, unavailable)
}

/// Finishes a join as the pages would, with no session presented, and says whose it was.
async fn joining(
    admins: &Accounts<PostgresAdmins>,
    party: &RelyingParty,
    ceremony: &str,
    response: &str,
) -> Result<AccountId, Error> {
    admins
        .finish_join(party, Answer { ceremony, response }, &[], &actor())
        .await
        .map(|(admin, _)| admin)
}

/// A session for a redemption to start, under a new token's hash, with no browser session to end.
fn starting() -> SessionStart {
    let token = SessionToken::generate(getrandom::fill).expect("a token");
    SessionStart {
        hash: token.hash(),
        replacing: None,
    }
}

/// A person at their device, who confirms they are present, and verifies themselves unless told not to, as a
/// hostile client might.
pub(crate) struct Person {
    pub(crate) verifies: bool,
}

type Checked<'b> = Pin<Box<dyn Future<Output = Result<UserCheck, Ctap2Error>> + Send + 'b>>;

// passkey-rs declares this trait through async-trait; writing out the boxed future it expects keeps that crate out of
// Tetsu's dependencies.
impl UserValidationMethod for Person {
    type PasskeyItem = Passkey;

    fn check_user<'a, 's, 'b>(
        &'s self,
        _hint: UiHint<'a, Passkey>,
        presence: bool,
        verification: bool,
    ) -> Checked<'b>
    where
        'a: 'b,
        's: 'b,
        Self: 'b,
    {
        let verified = verification && self.verifies;
        Box::pin(async move {
            Ok(UserCheck {
                presence,
                verification: verified,
            })
        })
    }

    fn is_presence_enabled(&self) -> bool {
        true
    }

    fn is_verification_enabled(&self) -> Option<bool> {
        Some(self.verifies)
    }
}

/// What a browser at `origin` sends back after its authenticator registers a passkey for the options given, as JSON.
pub(crate) async fn register(options: &str, origin: &str, verifies: bool) -> String {
    register_keeping(options, origin, verifies).await.0
}

/// What [`register`] sends back, as JSON, and the passkey the device keeps, so it can log in with it later.
pub(crate) async fn register_keeping(
    options: &str,
    origin: &str,
    verifies: bool,
) -> (String, Passkey) {
    let authenticator = Authenticator::new(
        Aaguid::new_empty(),
        None::<Passkey>,
        Person { verifies },
        passkey::crypto::AvailableBackend,
    );
    let mut client = Client::new(authenticator);
    let options: CredentialCreationOptions = serde_json::from_str(options).expect("options");
    let origin = Url::parse(origin).expect("an origin");
    let created = client
        .register(&origin, options, DefaultClientData)
        .await
        .expect("registers");
    let kept = client
        .authenticator()
        .store()
        .clone()
        .expect("a passkey kept");
    (serde_json::to_string(&created).expect("JSON"), kept)
}

/// What a browser at `origin` sends back after the device holding `passkey` logs in for the options given, as
/// JSON.
pub(crate) async fn authenticate(
    options: &str,
    origin: &str,
    passkey: Passkey,
    verifies: bool,
) -> String {
    let authenticator = Authenticator::new(
        Aaguid::new_empty(),
        Some(passkey),
        Person { verifies },
        passkey::crypto::AvailableBackend,
    );
    let mut client = Client::new(authenticator);
    let options: CredentialRequestOptions = serde_json::from_str(options).expect("options");
    let origin = Url::parse(origin).expect("an origin");
    let asserted = client
        .authenticate(&origin, options, DefaultClientData)
        .await
        .expect("logs in");
    serde_json::to_string(&asserted).expect("JSON")
}

/// Identity's use cases over a fresh copy of a migrated database, with Kurt Medley as admin 1 and his link issued.
pub(crate) async fn with_link() -> (Accounts<PostgresAdmins>, TestDatabase, AccountId, String) {
    let database = migrated_database(&[&Identity])
        .await
        .expect("a migrated test database");
    let admins = Accounts::new(PostgresAdmins::new(app_pool(&database)));
    let kurt = admins
        .create(
            &PersonName::parse("Kurt Medley").expect("a name"),
            None,
            &actor(),
        )
        .await
        .expect("creates");
    let link = admins.invite(kurt, &actor()).await.expect("issues");
    (admins, database, kurt, link.to_text())
}

pub(crate) fn app_pool(database: &TestDatabase) -> Pool {
    pool(&database.as_app()).expect("a pool")
}

pub(crate) fn party() -> RelyingParty {
    RelyingParty::new(ORIGIN).expect("a relying party")
}

pub(crate) fn actor() -> Actor {
    Actor::parse("admin-1@browser").expect("an actor")
}

/// What a failure says, as `category: message`, or nothing for a success, so a test compares text rather than
/// matching a pattern whose other arm no test takes.
pub(crate) fn failure(result: Result<(), Error>) -> String {
    result
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default()
}

fn not_joining() -> Result<AccountId, Error> {
    Err(Error::Invalid(
        "this invite has ended. Log in, or ask an admin for a new one".to_string(),
    ))
}

/// One number the database answers, read as the test's own superuser.
pub(crate) async fn number(database: &TestDatabase, sql: &str) -> i64 {
    connect(database)
        .await
        .expect("connects")
        .query_one(sql, &[])
        .await
        .expect("reads")
        .try_get(0)
        .expect("a number")
}

pub(crate) async fn as_db_admin(database: &TestDatabase, sql: &str) {
    connect(database)
        .await
        .expect("connects")
        .batch_execute(sql)
        .await
        .expect("runs");
}

/// What each event of a kind recorded, oldest first: its outcome and reason.
pub(crate) async fn recorded(database: &TestDatabase, kind: &str) -> Vec<Value> {
    connect(database)
        .await
        .expect("connects")
        .query(
            "SELECT payload::text FROM identity.outbox_global
             WHERE event_type = $1 ORDER BY occurred_at, id",
            &[&kind],
        )
        .await
        .expect("reads")
        .iter()
        .map(|row| {
            let payload: Value =
                serde_json::from_str(row.try_get::<_, &str>(0).expect("a payload")).expect("JSON");
            json!([payload["outcome"], payload["reason"]])
        })
        .collect()
}

/// What each join recorded, oldest first: its outcome and reason.
async fn redemptions(database: &TestDatabase) -> Vec<Value> {
    recorded(database, "identity.invite_redeemed").await
}

/// How many passkeys, links, ceremonies, and sessions are stored, in that order.
pub(crate) async fn held(database: &TestDatabase) -> [i64; 4] {
    let mut counts = [0; 4];
    for (count, table) in
        counts
            .iter_mut()
            .zip(["passkeys", "invites", "ceremonies", "admin_sessions"])
    {
        *count = number(database, &format!("SELECT count(*) FROM identity.{table}")).await;
    }
    counts
}

/// What creation options ask of a passkey and show of its account: whether it must be discoverable, both ways
/// `WebAuthn` asks, whether it must verify the user, and the account's name and display name.
fn asked_for(options: &str) -> [Value; 5] {
    let options: Value = serde_json::from_str(options).expect("JSON");
    let selection = &options["publicKey"]["authenticatorSelection"];
    let user = &options["publicKey"]["user"];
    [
        selection["residentKey"].clone(),
        selection["requireResidentKey"].clone(),
        selection["userVerification"].clone(),
        user["name"].clone(),
        user["displayName"].clone(),
    ]
}

/// The link opens a ceremony without being used up; finishing stores the passkey, uses up the link and the
/// ceremony, records it, and logs no one in. The options ask for a discoverable passkey that verifies the user.
#[tokio::test]
async fn an_admin_joins_their_first_passkey_once() {
    let (admins, database, kurt, link) = with_link().await;
    let party = party();
    let ceremony = admins.begin_join(&party, &link).await.expect("begins");
    assert_eq!(
        asked_for(&ceremony.options),
        [
            json!("required"),
            json!(true),
            json!("required"),
            json!("Tetsu admin · Kurt Medley"),
            json!("Kurt Medley"),
        ]
    );
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.invites").await,
        1,
        "beginning uses nothing up"
    );
    let response = register(&ceremony.options, ORIGIN, true).await;
    let token = ceremony.token.to_text();
    assert_eq!(joining(&admins, &party, &token, &response).await, Ok(kurt));
    // The passkey, and the session joining began; the link and every ceremony are gone.
    assert_eq!(held(&database).await, [1, 0, 0, 1]);
    assert_eq!(
        joining(&admins, &party, &token, &response).await,
        not_joining(),
        "a ceremony finishes once"
    );
    assert_eq!(
        admins.begin_join(&party, &link).await.map(drop),
        not_joining().map(drop),
        "a link is good once"
    );
    assert_eq!(redemptions(&database).await, [json!(["changed", null])]);
}

/// Options as a hostile client would rewrite them, asking its authenticator not to verify the user.
fn without_verification(options: &str) -> String {
    let mut options: Value = serde_json::from_str(options).expect("JSON");
    options["publicKey"]["authenticatorSelection"]["userVerification"] = json!("discouraged");
    options.to_string()
}

/// A registration is refused when it does not verify the user, or comes from another port or a subdomain of the
/// realm's origin. Each refusal leaves the link open, so the admin can try again, and is recorded with its
/// reason. These are honest clients set up wrong: with no attestation, the flags and origin a registration reports
/// are the client's word, which a hostile client could forge, so the guarantees that bind a person to a passkey
/// come at login, where the authenticator signs.
#[tokio::test]
async fn hostile_clients_are_refused_and_the_link_stays_open() {
    let (admins, database, _, link) = with_link().await;
    let party = party();
    for (origin, verifies, rewrite) in [
        (ORIGIN, false, true),
        ("https://admin.example.com:8443", true, false),
        ("https://evil.admin.example.com", true, false),
    ] {
        let ceremony = admins.begin_join(&party, &link).await.expect("begins");
        let options = if rewrite {
            without_verification(&ceremony.options)
        } else {
            ceremony.options.clone()
        };
        let response = register(&options, origin, verifies).await;
        assert_eq!(
            joining(&admins, &party, &ceremony.token.to_text(), &response).await,
            not_joining(),
            "{origin}, verifies: {verifies}"
        );
    }
    assert_eq!(
        redemptions(&database).await,
        vec![json!(["refused", "not_verified"]); 3]
    );
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.passkeys").await,
        0
    );
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.invites").await,
        1
    );
}

/// A registration that is not one at all, or not even JSON, is refused (#109), recorded as malformed, and leaves
/// the link open.
#[tokio::test]
async fn a_malformed_registration_is_refused_and_recorded() {
    let (admins, database, _, link) = with_link().await;
    let party = party();
    for response in ["not a registration", "{}"] {
        let ceremony = admins.begin_join(&party, &link).await.expect("begins");
        assert_eq!(
            joining(&admins, &party, &ceremony.token.to_text(), response).await,
            not_joining()
        );
    }
    assert_eq!(
        redemptions(&database).await,
        vec![json!(["refused", "malformed"]); 2]
    );
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.invites").await,
        1
    );
}

/// A registration answers one ceremony's challenge: sent with another admin's ceremony token it is refused,
/// and that ceremony is spent, while its own still finishes.
#[tokio::test]
async fn a_registration_finishes_only_its_own_ceremony() {
    let (admins, _database, kurt, link) = with_link().await;
    let party = party();
    let lee = admins
        .create(
            &PersonName::parse("Lee Park").expect("a name"),
            None,
            &actor(),
        )
        .await
        .expect("creates");
    let lee_link = admins.invite(lee, &actor()).await.expect("issues");
    let kurts = admins.begin_join(&party, &link).await.expect("begins");
    let lees = admins
        .begin_join(&party, &lee_link.to_text())
        .await
        .expect("begins");
    let response = register(&kurts.options, ORIGIN, true).await;
    let finished = [
        joining(&admins, &party, &lees.token.to_text(), &response).await,
        joining(&admins, &party, &kurts.token.to_text(), &response).await,
    ];
    assert_eq!(finished, [not_joining(), Ok(kurt)]);
}

/// A link holds one ceremony at a time: beginning again, as a second tab would, replaces the first, which can then
/// no longer finish, so beginning again and again stores nothing more.
#[tokio::test]
async fn a_newer_ceremony_replaces_an_older_one() {
    let (admins, database, kurt, link) = with_link().await;
    let party = party();
    let older = admins.begin_join(&party, &link).await.expect("begins");
    let newer = admins.begin_join(&party, &link).await.expect("begins");
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.ceremonies").await,
        1
    );
    let mut finished = Vec::new();
    for tab in [&older, &newer] {
        let response = register(&tab.options, ORIGIN, true).await;
        finished.push(joining(&admins, &party, &tab.token.to_text(), &response).await);
    }
    assert_eq!(finished, [not_joining(), Ok(kurt)]);
}

/// A ceremony lasts ten minutes, twice what a browser gives its authenticator, but never past its link.
#[tokio::test]
async fn a_ceremony_outlives_the_browsers_wait_but_not_its_link() {
    let (admins, database, _, link) = with_link().await;
    let party = party();
    admins.begin_join(&party, &link).await.expect("begins");
    let lasts = "SELECT extract(EPOCH FROM c.ends_at - e.issued_at)::bigint / 60
                 FROM identity.ceremonies AS c JOIN identity.invites AS e ON e.token_hash = c.link_hash";
    assert_eq!(number(&database, lasts).await, 10);
    as_db_admin(
        &database,
        "UPDATE identity.invites SET ends_at = issued_at + interval '3 minutes'",
    )
    .await;
    admins.begin_join(&party, &link).await.expect("begins");
    assert_eq!(number(&database, lasts).await, 3);
}

/// What changes between a ceremony's two halves still counts: a disable or a new link ends the ceremony, and so does
/// time, so the passkey is refused with nothing left to redeem; and a passkey stored for the admin meanwhile
/// refuses it as joined, recorded.
#[tokio::test]
async fn a_change_while_joining_refuses_the_passkey() {
    for change in ["disable", "new link", "ended", "joined elsewhere"] {
        let (admins, database, kurt, link) = with_link().await;
        let party = party();
        let ceremony = admins.begin_join(&party, &link).await.expect("begins");
        let response = register(&ceremony.options, ORIGIN, true).await;
        match change {
            "disable" => drop(admins.disable(kurt, &actor()).await.expect("disables")),
            "new link" => drop(admins.invite(kurt, &actor()).await.expect("issues")),
            "joined elsewhere" => {
                as_db_admin(
                    &database,
                    "INSERT INTO identity.passkeys (credential_id, admin_id, passkey, created_at)
                     VALUES (sha256('elsewhere'::bytea), 1, '{}', now())",
                )
                .await;
            }
            _ => {
                as_db_admin(
                    &database,
                    "UPDATE identity.ceremonies SET ends_at = now() - interval '1 second'",
                )
                .await;
            }
        }
        assert_eq!(
            joining(&admins, &party, &ceremony.token.to_text(), &response).await,
            not_joining(),
            "{change}"
        );
        let elsewhere = change == "joined elsewhere";
        assert_eq!(
            number(&database, "SELECT count(*) FROM identity.passkeys").await,
            i64::from(elsewhere),
            "{change}"
        );
        let recorded: Vec<Value> = [json!(["refused", "joined"])]
            .into_iter()
            .filter(|_| elsewhere)
            .collect();
        assert_eq!(redemptions(&database).await, recorded, "{change}");
    }
}

/// Begins a join from `link`, registers, and finishes with the response rewritten: `value` replaces the
/// response's transports, or stands in for its extension results under the name `field`.
async fn finish_rewritten(
    admins: &Accounts<PostgresAdmins>,
    party: &RelyingParty,
    link: &str,
    field: &str,
    value: Value,
) -> Result<AccountId, Error> {
    let ceremony = admins.begin_join(party, link).await.expect("begins");
    let mut response: Value =
        serde_json::from_str(&register(&ceremony.options, ORIGIN, true).await).expect("JSON");
    if field == "transports" {
        response["response"][field] = value;
    } else {
        let _ = response
            .as_object_mut()
            .map(|fields| fields.remove("clientExtensionResults"));
        response[field] = value;
    }
    admins
        .finish_join(
            party,
            Answer {
                ceremony: &ceremony.token.to_text(),
                response: &response.to_string(),
            },
            &[],
            &actor(),
        )
        .await
        .map(|(admin, _)| admin)
}

/// A registration whose client says the passkey is not discoverable is refused, under either name webauthn-rs reads
/// it by, since logging in could never find it, and the link stays open for another try. Transports are taken as
/// listed, as `WebAuthn` asks, even ones it does not name or names twice, and the ones `webauthn-rs` names are
/// kept once each, sorted, as a hint for the browser.
#[tokio::test]
async fn an_undiscoverable_registration_is_refused() {
    let (admins, database, _, link) = with_link().await;
    let party = party();
    let mut finished = Vec::new();
    for (field, value) in [
        (
            "clientExtensionResults",
            json!({"credProps": {"rk": false}}),
        ),
        ("extensions", json!({"credProps": {"rk": false}})),
        ("transports", json!(["smart-card", "cable", "usb", "usb"])),
    ] {
        finished.push(finish_rewritten(&admins, &party, &link, field, value).await);
    }
    let refused = not_joining();
    assert_eq!(finished, [refused.clone(), refused, Ok(AccountId::new(1))]);
    let kept = "SELECT count(*) FROM identity.passkeys WHERE transports = 'usb'";
    assert_eq!(number(&database, kept).await, 1);
    assert_eq!(
        redemptions(&database).await,
        [
            json!(["refused", "not_discoverable"]),
            json!(["refused", "not_discoverable"]),
            json!(["changed", null])
        ]
    );
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.passkeys").await,
        1
    );
}

/// Storing a passkey ends every other ceremony of the admin's, such as one begun after this one was taken, which
/// could only be refused now.
#[tokio::test]
async fn a_join_ends_the_admins_other_ceremonies() {
    let (admins, database, _, link) = with_link().await;
    let party = party();
    let store = PostgresAdmins::new(app_pool(&database));
    let first = admins.begin_join(&party, &link).await.expect("begins");
    let response = register(&first.options, ORIGIN, true).await;
    let taken = store
        .take_join(&first.token.hash())
        .await
        .expect("takes")
        .expect("a ceremony");
    admins
        .begin_join(&party, &link)
        .await
        .expect("begins again");
    let passkey = party
        .finish_registration(&response, &taken.state)
        .expect("verifies");
    assert_eq!(
        store
            .redeem_invite(&taken, &passkey, &starting(), &actor())
            .await,
        Ok(Some(Redemption::Redeemed))
    );
    // The passkey, and the session joining began; the link and every ceremony are gone.
    assert_eq!(held(&database).await, [1, 0, 0, 1]);
}

/// A new link ends any ceremony begun from the one it replaces.
#[tokio::test]
async fn a_new_link_ends_the_old_links_ceremony() {
    let (admins, database, kurt, link) = with_link().await;
    admins.begin_join(&party(), &link).await.expect("begins");
    admins.invite(kurt, &actor()).await.expect("issues");
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.ceremonies").await,
        0
    );
}

/// Beginning checks the link again as it stores the ceremony, so a link that ends after it was read keeps none.
#[tokio::test]
async fn beginning_checks_the_link_again_as_it_stores() {
    let (admins, database, _, link) = with_link().await;
    as_db_admin(
        &database,
        "UPDATE identity.invites SET ends_at = now() - interval '1 second';
         DROP FUNCTION identity.invitee(bytea);
         CREATE FUNCTION identity.invitee(bytea)
         RETURNS TABLE (admin_id bigint, user_handle bytea, name text)
         LANGUAGE sql AS $$ SELECT 1::bigint, uuid_send(gen_random_uuid()), 'Kurt Medley'::text $$;
         GRANT EXECUTE ON FUNCTION identity.invitee(bytea) TO tetsu_identity",
    )
    .await;
    assert_eq!(
        admins.begin_join(&party(), &link).await.map(drop),
        not_joining().map(drop)
    );
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.ceremonies").await,
        0
    );
}

/// A link or ceremony token that is malformed, or well formed but unknown, is refused the same way.
#[tokio::test]
async fn unknown_tokens_are_refused_alike() {
    let (admins, _database, _, _) = with_link().await;
    let party = party();
    let unknown = "ab".repeat(32);
    for link in ["not a token", unknown.as_str()] {
        assert_eq!(
            admins.begin_join(&party, link).await.map(drop),
            not_joining().map(drop),
            "{link}"
        );
        assert_eq!(
            joining(&admins, &party, link, "{}").await,
            not_joining(),
            "{link}"
        );
    }
}

/// Failures that are not the browser's are not hidden as one: no random source, a stored state that cannot be
/// read, and a stored admin a registration cannot start for.
#[tokio::test]
async fn failures_of_the_server_are_told_apart() {
    let (admins, database, _, link) = with_link().await;
    let party = party();
    let (failing, unavailable) = without_randomness(&database);
    assert_eq!(
        failure(failing.begin_join(&party, &link).await.map(drop)),
        unavailable
    );
    // A join whose session cannot be given a token stores nothing: the session and passkey come in one step.
    let ceremony = admins.begin_join(&party, &link).await.expect("begins");
    let (response, _) = register_keeping(&ceremony.options, ORIGIN, true).await;
    let finished = joining(&failing, &party, &ceremony.token.to_text(), &response).await;
    assert_eq!(failure(finished.map(drop)), unavailable);
    assert_eq!(held(&database).await, [0, 1, 0, 0]);
    let ceremony = admins.begin_join(&party, &link).await.expect("begins");
    as_db_admin(
        &database,
        "UPDATE identity.ceremonies SET state = '{\"x\": 1}'",
    )
    .await;
    let finished = joining(&admins, &party, &ceremony.token.to_text(), "{}").await;
    assert!(
        failure(finished.map(drop))
            .starts_with("storage failed: a stored passkey ceremony could not be read")
    );
    as_db_admin(
        &database,
        "ALTER TABLE identity.admins DROP CONSTRAINT admins_name_check;
         UPDATE identity.admins SET name = ''",
    )
    .await;
    let begun = admins.begin_join(&party, &link).await.map(drop);
    assert!(failure(begun).starts_with("storage failed: no ceremony could start"));
}

/// Kurt Medley with a stored passkey, whose credential ID is the SHA-256 of `kurt`, and Lee Park with an open link;
/// the store, a passkey with Kurt's credential ID, and a ceremony for Lee's link and the admin given.
async fn redeeming() -> (
    Accounts<PostgresAdmins>,
    TestDatabase,
    PostgresAdmins,
    NewPasskey,
    [TakenCeremony; 3],
) {
    let (admins, database, kurt, _) = with_link().await;
    let lee = admins
        .create(
            &PersonName::parse("Lee Park").expect("a name"),
            None,
            &actor(),
        )
        .await
        .expect("creates");
    let link_hash = admins.invite(lee, &actor()).await.expect("issues").hash();
    as_db_admin(
        &database,
        "INSERT INTO identity.passkeys (credential_id, admin_id, passkey, created_at)
         VALUES (sha256('kurt'::bytea), 1, '{}', now())",
    )
    .await;
    let store = PostgresAdmins::new(app_pool(&database));
    let passkey = NewPasskey {
        credential_id: Sha256::digest(b"kurt").to_vec(),
        passkey: "{}".to_string(),
        transports: Vec::new(),
    };
    let ceremony = |admin| TakenCeremony {
        admin,
        link_hash,
        state: "{}".to_string(),
    };
    let ceremonies = [ceremony(lee), ceremony(kurt), ceremony(AccountId::new(99))];
    (admins, database, store, passkey, ceremonies)
}

/// Storing a passkey refuses a credential another admin's passkey has, which leaves the link unused, and an
/// admin who does not exist, and records each with its reason.
#[tokio::test]
async fn a_taken_credential_or_missing_admin_stores_nothing() {
    let (_admins, database, store, passkey, [lees, _, nobodys]) = redeeming().await;
    let answers = [
        store
            .redeem_invite(&lees, &passkey, &starting(), &actor())
            .await,
        store
            .redeem_invite(&nobodys, &passkey, &starting(), &actor())
            .await,
    ];
    assert_eq!(answers, [Ok(Some(Redemption::Taken)), Ok(None)]);
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.invites").await,
        2
    );
    assert_eq!(
        redemptions(&database).await,
        [json!(["refused", "taken"]), json!(["refused", "unknown"])]
    );
}

/// Storing a passkey refuses a link no longer open, and a credential or passkey larger than the table keeps,
/// measured as the table measures it, and records each with its reason.
#[tokio::test]
async fn an_ended_link_or_oversized_passkey_stores_nothing() {
    let (_admins, database, store, _, [lees, _, _]) = redeeming().await;
    let unknown_link = TakenCeremony {
        link_hash: [7; 32],
        ..lees.clone()
    };
    let fresh = |credential_id: Vec<u8>, passkey: &str| NewPasskey {
        credential_id,
        passkey: passkey.to_string(),
        transports: Vec::new(),
    };
    // Compact, this is under 8 KiB; as jsonb writes it back, with a space after each colon and comma, it is over.
    let numbered: Vec<String> = (0..800).map(|n| format!("\"{n:04}\":1")).collect();
    let spaced = format!("{{{}}}", numbered.join(","));
    assert!(spaced.len() <= 8192);
    let mut answers = Vec::new();
    for (link, id, passkey) in [
        (&unknown_link, 16, "{}"),
        (&lees, 1024, "{}"),
        (&lees, 15, "{}"),
        (&lees, 16, spaced.as_str()),
    ] {
        let passkey = fresh(vec![1; id], passkey);
        answers.push(
            store
                .redeem_invite(link, &passkey, &starting(), &actor())
                .await,
        );
    }
    let oversized = Ok(Some(Redemption::Oversized));
    assert_eq!(
        answers,
        [
            Ok(Some(Redemption::Expired)),
            oversized.clone(),
            oversized.clone(),
            oversized
        ]
    );
    assert_eq!(
        redemptions(&database).await,
        [
            json!(["refused", "expired"]),
            json!(["refused", "oversized"]),
            json!(["refused", "oversized"]),
            json!(["refused", "oversized"]),
        ]
    );
}

/// Storing a passkey refuses an admin who holds one already, and one disabled, and records why.
#[tokio::test]
async fn a_joined_or_disabled_admin_stores_nothing() {
    let (admins, database, store, passkey, [lees, kurts, _]) = redeeming().await;
    let joined = store
        .redeem_invite(&kurts, &passkey, &starting(), &actor())
        .await;
    admins
        .disable(lees.admin, &actor())
        .await
        .expect("disables");
    let disabled = store
        .redeem_invite(&lees, &passkey, &starting(), &actor())
        .await;
    assert_eq!(
        [joined, disabled],
        [Ok(Some(Redemption::Joined)), Ok(Some(Redemption::Disabled))]
    );
    assert_eq!(
        redemptions(&database).await,
        [json!(["refused", "joined"]), json!(["refused", "disabled"])]
    );
}

/// A relying party needs an origin whose host is a domain: passkeys cannot belong to an address.
#[test]
fn a_relying_party_needs_an_origin_with_a_domain() {
    // The first three fail as URLs or as relying parties; the rest parse and would pass webauthn-rs, and only the
    // rules for writing an origin as browsers send it refuse them.
    let bad = [
        "not an origin",
        "data:text/plain,x",
        "https://127.0.0.1",
        "http://admin.example.com",
        "https://admin.example.com/some/path",
        "https://ADMIN.example.com",
        "https://admin.example.com:443",
        "https://user@admin.example.com",
    ];
    let refused: Vec<String> = bad
        .into_iter()
        .map(|origin| failure(RelyingParty::new(origin).map(drop)))
        .collect();
    let expected: Vec<String> = bad
        .into_iter()
        .map(|origin| format!("invalid: {origin:?} is not an origin"))
        .collect();
    assert_eq!(refused, expected);
    assert!(RelyingParty::new("http://localhost:8080").is_ok());
}

/// Each step of a join fails as storage when the database refuses its function, never as the browser's
/// mistake.
#[tokio::test]
async fn each_join_step_fails_as_storage_when_refused() {
    let party = party();
    for (function, early) in [
        ("invitee(bytea)", true),
        ("begin_join(bytea, bigint, bytea, jsonb)", true),
        ("take_ceremony(bytea, text)", false),
        (
            "redeem_invite(bytea, bigint, bytea, jsonb, text, bytea, bytea)",
            false,
        ),
    ] {
        let (admins, database, _, link) = with_link().await;
        let revoke = format!("REVOKE EXECUTE ON FUNCTION identity.{function} FROM tetsu_identity");
        let outcome = if early {
            as_db_admin(&database, &revoke).await;
            admins.begin_join(&party, &link).await.map(drop)
        } else {
            let ceremony = admins.begin_join(&party, &link).await.expect("begins");
            let response = register(&ceremony.options, ORIGIN, true).await;
            as_db_admin(&database, &revoke).await;
            joining(&admins, &party, &ceremony.token.to_text(), &response)
                .await
                .map(drop)
        };
        let name = function.split('(').next().unwrap_or_default();
        assert_eq!(
            failure(outcome),
            format!("storage failed: permission denied for function {name}")
        );
    }
}

/// Replaces one of Identity's functions with another of the same arguments, as Identity's role may run it.
pub(crate) async fn replace(database: &TestDatabase, function: &str, returns: &str, body: &str) {
    let arguments = &function[function.find('(').unwrap_or_default()..];
    let name = &function[..function.find('(').unwrap_or_default()];
    as_db_admin(
        database,
        &format!(
            "DROP FUNCTION identity.{function};
             CREATE FUNCTION identity.{name}{arguments} RETURNS {returns} LANGUAGE sql AS '{}';
             GRANT EXECUTE ON FUNCTION identity.{function} TO tetsu_identity",
            body.replace('\'', "''")
        ),
    )
    .await;
}

/// An unreadable answer when beginning, a column of the wrong type, is a storage failure.
#[tokio::test]
async fn unreadable_invitees_are_storage_failures() {
    let party = party();
    for (columns, values) in [
        (
            "admin_id text, user_handle bytea, name text",
            "'1', '\\x00'::bytea, 'Kurt'",
        ),
        (
            "admin_id bigint, user_handle text, name text",
            "1::bigint, 'x', 'Kurt'",
        ),
        (
            "admin_id bigint, user_handle bytea, name integer",
            "1::bigint, uuid_send(gen_random_uuid()), 1",
        ),
    ] {
        let (admins, database, _, link) = with_link().await;
        replace(
            &database,
            "invitee(bytea)",
            &format!("TABLE ({columns})"),
            &format!("SELECT {values}"),
        )
        .await;
        let begun = failure(admins.begin_join(&party, &link).await.map(drop));
        assert!(begun.starts_with("storage failed: "));
    }
    let (admins, database, _, link) = with_link().await;
    replace(
        &database,
        "invitee(bytea)",
        "TABLE (admin_id bigint, user_handle bytea, name text)",
        "SELECT 1::bigint, '\\x00'::bytea, 'Kurt'",
    )
    .await;
    assert_eq!(
        failure(admins.begin_join(&party, &link).await.map(drop)),
        "storage failed: a user handle is not 16 bytes"
    );
}

/// An unreadable ceremony, a column of the wrong type or a join with no link, is a storage failure.
#[tokio::test]
async fn unreadable_ceremonies_are_storage_failures() {
    let party = party();
    let link = "sha256('x'::bytea)";
    for (columns, values, said) in [
        (
            "admin_id text, link_hash bytea, state text",
            format!("'1', {link}, '{{}}'"),
            "storage failed: ",
        ),
        (
            "admin_id bigint, link_hash text, state text",
            "1::bigint, 'x', '{}'".to_string(),
            "storage failed: ",
        ),
        (
            "admin_id bigint, link_hash bytea, state integer",
            format!("1::bigint, {link}, 1"),
            "storage failed: ",
        ),
        (
            "admin_id bigint, link_hash bytea, state text",
            "1::bigint, null::bytea, '{}'".to_string(),
            "storage failed: a join ceremony has no link",
        ),
    ] {
        let (admins, database, _, link) = with_link().await;
        let ceremony = admins.begin_join(&party, &link).await.expect("begins");
        replace(
            &database,
            "take_ceremony(bytea, text)",
            &format!("TABLE ({columns})"),
            &format!("SELECT {values}"),
        )
        .await;
        let finished = failure(
            joining(&admins, &party, &ceremony.token.to_text(), "{}")
                .await
                .map(drop),
        );
        assert!(finished.starts_with(said));
    }
}

/// An unreadable redemption, of the wrong type or an answer the database never writes, is a storage failure.
#[tokio::test]
async fn unreadable_redemptions_are_storage_failures() {
    let party = party();
    for (returns, value, said) in [
        (
            "text",
            "'maybe'",
            "storage failed: redeeming a link answered \"maybe\"",
        ),
        ("integer", "1", "storage failed: "),
    ] {
        let (admins, database, _, link) = with_link().await;
        let ceremony = admins.begin_join(&party, &link).await.expect("begins");
        let response = register(&ceremony.options, ORIGIN, true).await;
        replace(
            &database,
            "redeem_invite(bytea, bigint, bytea, jsonb, text, bytea, bytea)",
            returns,
            &format!("SELECT {value}"),
        )
        .await;
        let finished = failure(
            joining(&admins, &party, &ceremony.token.to_text(), &response)
                .await
                .map(drop),
        );
        assert!(finished.starts_with(said));
    }
}

/// Joining ends the admin session the browser presented, whoever's it is, as logging in does (ADR-0059,
/// ADR-0069), and starts its own; a refused join leaves the presented one as it was.
#[tokio::test]
async fn joining_ends_the_session_the_browser_presented() {
    let (admins, database, _, link) = with_link().await;
    let party = party();
    let earlier = "aa".repeat(32);
    as_db_admin(
        &database,
        &format!(
            "SELECT identity.start_session(sha256(decode('{earlier}', 'hex')), 'admin', 1, true)"
        ),
    )
    .await;
    let cookie = format!("{}={earlier}", crate::Realm::Admin.cookie_name());
    let held_earlier = format!(
        "SELECT count(*) FROM identity.admin_sessions WHERE token_hash = sha256(decode('{earlier}', 'hex'))"
    );
    let refused = admins
        .finish_join(
            &party,
            Answer {
                ceremony: "not a ceremony",
                response: "{}",
            },
            &[cookie.as_str()],
            &actor(),
        )
        .await;
    assert!(refused.is_err());
    assert_eq!(number(&database, &held_earlier).await, 1);
    let ceremony = admins.begin_join(&party, &link).await.expect("begins");
    let (response, _) = register_keeping(&ceremony.options, ORIGIN, true).await;
    let (_, token) = admins
        .finish_join(
            &party,
            Answer {
                ceremony: &ceremony.token.to_text(),
                response: &response,
            },
            &[cookie.as_str()],
            &actor(),
        )
        .await
        .expect("joins");
    assert_eq!(number(&database, &held_earlier).await, 0);
    let started = format!(
        "SELECT count(*) FROM identity.admin_sessions
         WHERE token_hash = sha256(decode('{}', 'hex')) AND proven_at IS NULL",
        token.to_text()
    );
    assert_eq!(number(&database, &started).await, 1);
}
