//! Admins and their sessions in Postgres (ADR-0040, ADR-0047, ADR-0048), logged in as the application role, as
//! `tetsu serve` is. Postgres's clock is the only one a session follows, so tests move a stored session's times
//! rather than pass a time in. Their shared setup is in `common`.

use std::time::Duration;

use tetsu_identity::{
    AccountId, Change, Error, FailedCheck, ForChange, Identity, Logout, Origins, Realm,
    RelyingParty, SessionToken, postgres_store,
};
use tetsu_kernel::Address;
use tetsu_postgres::test_support::TestDatabase;
use tetsu_postgres::{ModuleTransaction, Settings, connect, isolation_violations, pool};

const ORIGIN: &str = "https://admin.example.com";

#[cfg(test)]
mod support {
    use super::*;

    /// The admin realm's idle limit, which the database keeps too; tests age sessions by it, so the two must agree.
    pub fn idle_seconds() -> u64 {
        Realm::Admin.limits().idle.as_secs()
    }

    /// The `Cookie` header a browser sends back with an admin's session token.
    pub fn cookie(token: &str) -> String {
        format!("{}={token}", Realm::Admin.cookie_name())
    }

    pub fn text(token: &SessionToken) -> String {
        cookie(&token.to_text())
    }

    pub fn origins() -> Origins {
        Origins::parse(&[ORIGIN]).expect("origins")
    }

    /// Runs SQL as the test server's database administrator, to move a session's times or stand in for a bad migration.
    pub async fn as_db_admin(database: &TestDatabase, sql: &str) {
        connect(database)
            .await
            .expect("connects")
            .batch_execute(sql)
            .await
            .expect("runs");
    }

    /// Whether every stored session meets a condition, such as `last_seen_at > now() - interval '1 minute'`; false
    /// when none is stored.
    pub async fn stored(database: &TestDatabase, condition: &str) -> bool {
        connect(database)
            .await
            .expect("connects")
            .query_one(
                &format!(
                    "SELECT count(*) > 0 AND bool_and({condition}) AS met FROM identity.sessions"
                ),
                &[],
            )
            .await
            .expect("checks")
            .try_get("met")
            .expect("reads")
    }

    /// How many logouts the outbox holds.
    pub async fn logouts_recorded(database: &TestDatabase) -> i64 {
        connect(database)
            .await
            .expect("connects")
            .query_one(
                "SELECT count(*) AS n FROM identity.outbox_global WHERE event_type = 'identity.logged_out'",
                &[],
            )
            .await
            .expect("counts logouts")
            .try_get("n")
            .expect("reads")
    }

    /// How many stored sessions meet a condition.
    pub async fn count(database: &TestDatabase, condition: &str) -> i64 {
        connect(database)
            .await
            .expect("connects")
            .query_one(
                &format!("SELECT count(*) AS n FROM identity.sessions WHERE {condition}"),
                &[],
            )
            .await
            .expect("counts")
            .try_get("n")
            .expect("reads")
    }

    /// Replaces `use_session` with one whose `wrong` column comes back as an `int`, and the rest as they should.
    pub fn use_session_with_wrong(wrong: &str) -> String {
        let columns = [
            ("realm", "text", "'admin'"),
            ("admin_id", "bigint", "1::bigint"),
            ("started_at", "timestamptz", "now()"),
            ("proven_at", "timestamptz", "now()"),
            ("last_seen_at", "timestamptz", "now()"),
            ("ends_at", "timestamptz", "now() + interval '1 hour'"),
        ];
        let (types, values): (Vec<String>, Vec<&str>) = columns
            .iter()
            .map(|&(column, kind, value)| {
                if column == wrong {
                    (format!("{column} int"), "1")
                } else {
                    (format!("{column} {kind}"), value)
                }
            })
            .unzip();
        replaced_use_session(&types.join(", "), &values.join(", "))
    }

    /// SQL that replaces `use_session` with one returning `values` as `columns`.
    pub fn replaced_use_session(columns: &str, values: &str) -> String {
        format!(
            "DROP FUNCTION identity.use_session(bytea, text);
             CREATE FUNCTION identity.use_session(wanted_hash bytea, wanted_realm text)
             RETURNS TABLE ({columns}) LANGUAGE sql AS $$ SELECT {values} $$;
             GRANT EXECUTE ON FUNCTION identity.use_session(bytea, text) TO tetsu_identity"
        )
    }
}

#[cfg(test)]
mod common;

use common::{actor, logged_in, migrated, name, ready, with_kurt};
use support::{
    as_db_admin, cookie, count, idle_seconds, logouts_recorded, origins, replaced_use_session,
    stored, text, use_session_with_wrong,
};

/// Identity keeps its tables in the one schema it owns, under its own name (ADR-0003, ADR-0042).
#[test]
fn identity_registers_its_name_and_schema() {
    use tetsu_kernel::Module;
    assert_eq!(Identity.name(), "identity");
    assert_eq!(
        Identity.storage().map(|storage| storage.schema),
        Some("identity")
    );
}

#[tokio::test]
async fn identity_follows_the_isolation_rules() {
    let database = ready().await;
    let mut client = connect(&database).await.expect("connects");
    assert_eq!(
        isolation_violations(&mut client, &["identity"])
            .await
            .expect("checks"),
        Vec::<String>::new()
    );
}

/// A session starts now by Postgres's clock, lasts eight hours at most, and records each request on it.
#[tokio::test]
async fn a_started_session_is_found_by_its_token_and_records_each_request() {
    let (database, admins, kurt) = with_kurt().await;
    let token = logged_in(&database, kurt).await;
    assert!(
        stored(
            &database,
            &format!(
                "last_seen_at = started_at AND ends_at = started_at + interval '{} seconds'",
                Realm::Admin.limits().absolute.as_secs()
            )
        )
        .await
    );
    as_db_admin(
        &database,
        "UPDATE identity.sessions SET last_seen_at = last_seen_at - interval '10 minutes'",
    )
    .await;
    let session = admins
        .session([text(&token).as_str()])
        .await
        .expect("looks up")
        .expect("a session");
    assert_eq!(session.realm(), Realm::Admin);
    assert_eq!(session.admin(), kurt);
    assert_eq!(
        session.ends_at(),
        session.started_at() + Duration::from_hours(8)
    );
    assert!(session.last_seen_at() > session.started_at(), "recorded");
    assert!(
        stored(&database, "last_seen_at > now() - interval '1 minute'").await,
        "the request was stored"
    );
}

#[tokio::test]
async fn each_admin_gets_their_own_id_and_a_random_user_handle() {
    let (database, admins) = migrated().await;
    let first = admins
        .create(&name("Kurt Medley"), None, &actor())
        .await
        .expect("creates");
    let second = admins
        .create(&name("Lee Park"), None, &actor())
        .await
        .expect("creates");
    assert_ne!(first, second);
    let handles = connect(&database)
        .await
        .expect("connects")
        .query_one(
            "SELECT count(DISTINCT user_handle) AS handles FROM identity.admins",
            &[],
        )
        .await
        .expect("counts");
    assert_eq!(handles.try_get::<_, i64>("handles").expect("reads"), 2);
}

/// Only the token's SHA-256 hash is stored, so the stored row cannot log anyone in (ADR-0047).
#[tokio::test]
async fn only_the_tokens_hash_is_stored() {
    let (database, _, kurt) = with_kurt().await;
    let token = logged_in(&database, kurt).await;
    let text = token.to_text();
    assert!(
        stored(
            &database,
            &format!("token_hash = sha256(decode('{text}', 'hex'))")
        )
        .await
    );
    assert!(!stored(&database, &format!("token_hash = decode('{text}', 'hex')")).await);
}

/// Fifteen minutes without a request ends an admin's session for good, and its row is removed (ADR-0048).
#[tokio::test]
async fn an_idle_session_ends_and_is_removed() {
    let (database, admins, kurt) = with_kurt().await;
    let token = logged_in(&database, kurt).await;
    as_db_admin(
        &database,
        &format!(
            "UPDATE identity.sessions SET last_seen_at = now() - interval '{} seconds'",
            idle_seconds() - 10
        ),
    )
    .await;
    assert!(
        admins
            .session([text(&token).as_str()])
            .await
            .expect("looks up")
            .is_some(),
        "still within 15 minutes"
    );
    as_db_admin(
        &database,
        &format!(
            "UPDATE identity.sessions SET last_seen_at = now() - interval '{} seconds'",
            idle_seconds()
        ),
    )
    .await;
    assert_eq!(admins.session([text(&token).as_str()]).await, Ok(None));
    assert!(
        !stored(&database, "true").await,
        "the spent session is removed"
    );
}

/// Eight hours after it starts, a session ends however busy it is, and its row is removed (ADR-0048).
#[tokio::test]
async fn a_busy_session_still_ends_at_its_absolute_limit() {
    let (database, admins, kurt) = with_kurt().await;
    let token = logged_in(&database, kurt).await;
    as_db_admin(&database, "UPDATE identity.sessions SET ends_at = now()").await;
    assert_eq!(admins.session([text(&token).as_str()]).await, Ok(None));
    assert!(
        !stored(&database, "true").await,
        "the spent session is removed"
    );
}

/// A request handled after a later one never moves the last request back, which would shorten the session.
#[tokio::test]
async fn the_last_request_never_moves_back() {
    let (database, admins, kurt) = with_kurt().await;
    let token = logged_in(&database, kurt).await;
    as_db_admin(
        &database,
        "UPDATE identity.sessions SET last_seen_at = now() + interval '1 minute'",
    )
    .await;
    admins
        .session([text(&token).as_str()])
        .await
        .expect("looks up")
        .expect("a session");
    assert!(stored(&database, "last_seen_at > now()").await);
}

/// Requests arriving together all find a live session, and none ends it: checking and recording are one step.
#[tokio::test]
async fn requests_at_the_same_moment_never_end_a_live_session() {
    let database = ready().await;
    let admins = std::sync::Arc::new(postgres_store(pool(&database.as_app()).expect("a pool")));
    let kurt = admins
        .create(&name("Kurt Medley"), None, &actor())
        .await
        .expect("creates");
    let token = text(&logged_in(&database, kurt).await);
    let requests: Vec<_> = (0..16)
        .map(|_| {
            let (admins, token) = (admins.clone(), token.clone());
            tokio::spawn(async move { admins.session([token.as_str()]).await })
        })
        .collect();
    for request in requests {
        assert!(request.await.expect("runs").expect("looks up").is_some());
    }
    assert!(stored(&database, "true").await);
}

#[tokio::test]
async fn a_token_no_session_holds_finds_none() {
    let (_database, admins) = migrated().await;
    for token in ["", "not a token", &"ab".repeat(32), &"AB".repeat(32)] {
        assert_eq!(
            admins.session([cookie(token).as_str()]).await,
            Ok(None),
            "{token:?}"
        );
    }
}

/// Logging out ends the session on the server, so its token logs no one in again; as a change of state, it passes
/// the origin and form-token checks first, and one that fails them ends nothing (ADR-0048).
#[tokio::test]
async fn logging_out_ends_the_session_once_its_checks_pass() {
    let (database, admins, kurt) = with_kurt().await;
    let token = logged_in(&database, kurt).await;
    let (jar, form) = (text(&token), token.form_token().expect("a form token"));
    let logout = |origin| Change::new([jar.as_str()], origin, None, &form);
    let from_elsewhere = logout(Some("https://evil.example"));
    assert_eq!(
        admins
            .end_session(&origins(), &from_elsewhere, &actor())
            .await,
        Ok(Logout::Refused(FailedCheck::Origin))
    );
    assert!(
        admins
            .session([jar.as_str()])
            .await
            .expect("looks up")
            .is_some(),
        "not ended"
    );
    let own = logout(Some(ORIGIN));
    assert_eq!(
        admins.end_session(&origins(), &own, &actor()).await,
        Ok(Logout::Ended)
    );
    assert_eq!(admins.session([jar.as_str()]).await, Ok(None));
    assert_eq!(
        admins.end_session(&origins(), &own, &actor()).await,
        Ok(Logout::NothingToEnd),
        "nothing left to end"
    );
}

/// Logging out ends that session alone; the admin's others go on.
#[tokio::test]
async fn logging_out_leaves_the_admins_other_sessions() {
    let (database, admins, kurt) = with_kurt().await;
    let token = logged_in(&database, kurt).await;
    let other = logged_in(&database, kurt).await;
    let (jar, form) = (text(&token), token.form_token().expect("a form token"));
    let logout = Change::new([jar.as_str()], Some(ORIGIN), None, &form);
    assert_eq!(
        admins.end_session(&origins(), &logout, &actor()).await,
        Ok(Logout::Ended)
    );
    assert!(
        admins
            .session([text(&other).as_str()])
            .await
            .expect("looks up")
            .is_some()
    );
}

/// Only a passkey login starts a session, inside Identity's own functions, so Identity's role cannot start one
/// directly: no bug in the application can make a session that may add a passkey (ADR-0048).
#[tokio::test]
async fn identitys_role_cannot_start_a_session() {
    let (database, _, kurt) = with_kurt().await;
    let pool = pool(&database.as_app()).expect("a pool");
    let mut client = pool.get().await.expect("a connection");
    let transaction = ModuleTransaction::begin(&mut client, "identity")
        .await
        .expect("begins");
    let refused = transaction
        .execute(
            "SELECT identity.start_session(sha256('a'::bytea), 'admin', $1, true)",
            &[&kurt.get()],
        )
        .await
        .expect_err("refused");
    assert_eq!(
        refused.code(),
        Some(&tokio_postgres::error::SqlState::INSUFFICIENT_PRIVILEGE)
    );
}

/// Identity's own role reaches sessions only through its functions, one token's hash at a time, so no request can
/// list them or read another's (ADR-0042, ADR-0048).
#[tokio::test]
async fn identitys_role_cannot_list_sessions() {
    let database = ready().await;
    let pool = pool(&database.as_app()).expect("a pool");
    let mut client = pool.get().await.expect("a connection");
    let transaction = ModuleTransaction::begin(&mut client, "identity")
        .await
        .expect("begins");
    let refused = transaction
        .query("SELECT token_hash FROM identity.sessions", &[])
        .await
        .expect_err("refused");
    assert_eq!(
        refused.code(),
        Some(&tokio_postgres::error::SqlState::INSUFFICIENT_PRIVILEGE)
    );
}

/// A session in another realm is never an admin's, so a token from another realm's cookie logs no one in as an
/// admin, who reaches every business; nor does the admin realm extend or end it (ADR-0040).
#[tokio::test]
async fn another_realms_session_is_neither_used_nor_ended() {
    let (database, admins, kurt) = with_kurt().await;
    let token = logged_in(&database, kurt).await;
    as_db_admin(
        &database,
        "ALTER TABLE identity.sessions DROP CONSTRAINT sessions_realm_check;
         UPDATE identity.sessions SET realm = 'staff', last_seen_at = now() - interval '1 hour'",
    )
    .await;
    assert_eq!(admins.session([text(&token).as_str()]).await, Ok(None));
    let (jar, form) = (text(&token), token.form_token().expect("a form token"));
    let logout = Change::new([jar.as_str()], Some(ORIGIN), None, &form);
    assert_eq!(
        admins.end_session(&origins(), &logout, &actor()).await,
        Ok(Logout::NothingToEnd),
        "not this realm's to end"
    );
    assert!(
        stored(
            &database,
            "realm = 'staff' AND last_seen_at < now() - interval '59 minutes'"
        )
        .await,
        "left as it was"
    );
}

/// A realm Identity does not know, should a lookup return one, is an error, never a session in some default realm.
#[tokio::test]
async fn an_unknown_realm_is_an_error() {
    let (database, admins) = migrated().await;
    as_db_admin(
        &database,
        &replaced_use_session(
            "realm text, admin_id bigint, started_at timestamptz, proven_at timestamptz, last_seen_at timestamptz, \
             ends_at timestamptz",
            "'staff', 1::bigint, now(), now(), now(), now()",
        ),
    )
    .await;
    assert_eq!(
        admins.session([cookie(&"ab".repeat(32)).as_str()]).await,
        Err(Error::Storage("unknown realm \"staff\"".to_string()))
    );
}

/// Each stored column of the wrong type is an error rather than a session, whichever column it is.
#[tokio::test]
async fn each_stored_column_of_the_wrong_type_is_an_error() {
    let columns = [
        "realm",
        "admin_id",
        "started_at",
        "proven_at",
        "last_seen_at",
        "ends_at",
    ];
    for wrong in columns {
        let (database, admins) = migrated().await;
        as_db_admin(&database, &use_session_with_wrong(wrong)).await;
        assert!(
            matches!(
                admins.session([cookie(&"ab".repeat(32)).as_str()]).await,
                Err(Error::Storage(_))
            ),
            "{wrong}"
        );
    }
}

/// A create whose answer cannot be read is a storage failure.
#[tokio::test]
async fn a_create_answer_that_cannot_be_read_is_a_storage_failure() {
    let (database, admins) = migrated().await;
    as_db_admin(
        &database,
        "ALTER TABLE identity.admins ALTER COLUMN id TYPE integer",
    )
    .await;
    assert_eq!(
        admins.create(&name("Kurt Medley"), None, &actor()).await,
        Err(Error::Storage(
            "error deserializing column 0: cannot convert between the Rust type `i64` and the Postgres type `int4`"
                .to_string()
        ))
    );
}

/// A cached statement a migration made stale is temporary, and once the caches are cleared the next request
/// succeeds (ADR-0011).
#[tokio::test]
async fn a_stale_cached_statement_is_temporary_and_cleared() {
    let (database, admins) = migrated().await;
    let party = RelyingParty::new(ORIGIN).expect("a relying party");
    admins.begin_login(&party).await.expect("begins");
    as_db_admin(
        &database,
        "DROP FUNCTION identity.begin_login(bytea, jsonb);
         CREATE FUNCTION identity.begin_login(bytea, jsonb)
         RETURNS int LANGUAGE sql AS 'SELECT 1';
         GRANT EXECUTE ON FUNCTION identity.begin_login(bytea, jsonb) TO tetsu_identity",
    )
    .await;
    assert!(matches!(
        admins.begin_login(&party).await,
        Err(Error::Unavailable(why)) if why.starts_with(tetsu_postgres::STALE)
    ));
    assert!(
        admins.begin_login(&party).await.is_ok(),
        "the next request succeeds"
    );
}

#[tokio::test]
async fn an_unreachable_database_is_an_error() {
    let database = ready().await;
    let url = database
        .url()
        .replacen(database.name(), "no_such_database", 1);
    let admins =
        postgres_store(pool(&Settings::from_values(&url, Some("disable"))).expect("a pool"));
    assert!(matches!(
        admins.create(&name("Kurt Medley"), None, &actor()).await,
        Err(Error::Storage(_))
    ));
}

#[tokio::test]
async fn an_admin_id_of_the_wrong_type_is_an_error() {
    let (database, admins) = migrated().await;
    as_db_admin(
        &database,
        "ALTER TABLE identity.sessions DROP CONSTRAINT sessions_admin_id_fkey;
         ALTER TABLE identity.invites DROP CONSTRAINT invites_admin_id_fkey;
         ALTER TABLE identity.passkeys DROP CONSTRAINT passkeys_admin_id_fkey;
         ALTER TABLE identity.ceremonies DROP CONSTRAINT ceremonies_admin_id_fkey;
         DROP TABLE identity.admins;
         CREATE TABLE identity.admins (id text NOT NULL DEFAULT 'one', name text NOT NULL);
         GRANT SELECT, INSERT ON identity.admins TO tetsu_identity",
    )
    .await;
    assert!(matches!(
        admins.create(&name("Kurt Medley"), None, &actor()).await,
        Err(Error::Storage(_))
    ));
}

/// A transaction Identity's role cannot begin, as for a server login that may not act as it, is an error. The login
/// is this test's own, so no other test's roles change.
#[tokio::test]
async fn a_transaction_that_cannot_begin_is_an_error() {
    let database = ready().await;
    let outsider = format!("outsider_{}", database.name());
    as_db_admin(&database, &format!("CREATE ROLE {outsider} LOGIN")).await;
    let url = database
        .url()
        .replacen("postgres@", &format!("{outsider}@"), 1);
    let admins =
        postgres_store(pool(&Settings::from_values(&url, Some("disable"))).expect("a pool"));
    let refused = admins.create(&name("Kurt Medley"), None, &actor()).await;
    drop(admins);
    as_db_admin(&database, &format!("DROP ROLE {outsider}")).await;
    assert!(matches!(refused, Err(Error::Storage(_))), "{refused:?}");
}

#[tokio::test]
async fn a_statement_that_cannot_be_prepared_is_an_error() {
    let (database, admins, kurt) = with_kurt().await;
    let token = logged_in(&database, kurt).await;
    let (jar, form) = (text(&token), token.form_token().expect("a form token"));
    let logout = Change::new([jar.as_str()], Some(ORIGIN), None, &form);
    as_db_admin(&database, "DROP FUNCTION identity.log_out(bytea, text)").await;
    assert!(matches!(
        admins.end_session(&origins(), &logout, &actor()).await,
        Err(Error::Storage(_))
    ));
    as_db_admin(&database, "DROP FUNCTION identity.use_session(bytea, text)").await;
    assert!(matches!(
        admins.session([text(&token).as_str()]).await,
        Err(Error::Storage(_))
    ));
    assert!(matches!(
        admins.session_for_change(&origins(), &logout).await,
        Err(Error::Storage(_))
    ));
}

/// A commit Postgres refuses was rolled back, so logging out ended nothing, and the refusal is never worth retrying.
#[tokio::test]
async fn a_refused_commit_ends_no_session() {
    let (database, admins, kurt) = with_kurt().await;
    let token = logged_in(&database, kurt).await;
    as_db_admin(
        &database,
        "CREATE FUNCTION identity.refuse() RETURNS trigger LANGUAGE plpgsql
         AS $$ BEGIN RAISE EXCEPTION 'refused at commit'; END $$;
         CREATE CONSTRAINT TRIGGER refuse AFTER DELETE ON identity.sessions
         DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION identity.refuse()",
    )
    .await;
    let (jar, form) = (text(&token), token.form_token().expect("a form token"));
    assert_eq!(
        admins
            .end_session(
                &origins(),
                &Change::new([jar.as_str()], Some(ORIGIN), None, &form),
                &actor(),
            )
            .await,
        Err(Error::Storage(
            "commit refused: refused at commit".to_string()
        ))
    );
    assert_eq!(
        count(&database, "true").await,
        1,
        "the session is still stored"
    );
}

/// A state-changing request acts for its session only from the realm's own origin and with that session's own form
/// token; failing either finds no session and leaves it as it was (ADR-0048).
#[tokio::test]
async fn a_change_needs_the_realms_origin_and_the_sessions_form_token() {
    let (database, admins, kurt) = with_kurt().await;
    let token = logged_in(&database, kurt).await;
    let other = logged_in(&database, kurt).await;
    as_db_admin(
        &database,
        "UPDATE identity.sessions SET last_seen_at = now() - interval '10 minutes'",
    )
    .await;
    let (session, form, others) = (
        token.to_text(),
        token.form_token().expect("a form token"),
        other.form_token().expect("a form token"),
    );
    let jar = cookie(&session);
    let change = |origin, form_token| Change::new([jar.as_str()], origin, None, form_token);
    for (refused, failed) in [
        (change(None, &form), FailedCheck::Origin),
        (change(Some("null"), &form), FailedCheck::Origin),
        (
            change(Some("https://evil.example"), &form),
            FailedCheck::Origin,
        ),
        (change(Some(ORIGIN), ""), FailedCheck::FormToken),
        (change(Some(ORIGIN), &others), FailedCheck::FormToken),
        (change(Some(ORIGIN), &session), FailedCheck::FormToken),
    ] {
        assert_eq!(
            admins.session_for_change(&origins(), &refused).await,
            Ok(ForChange::Refused(failed))
        );
    }
    assert!(
        stored(&database, "last_seen_at < now() - interval '9 minutes'").await,
        "no refused request was recorded"
    );
    let accepted = admins
        .session_for_change(&origins(), &change(Some(ORIGIN), &form))
        .await;
    assert!(
        matches!(&accepted, Ok(ForChange::Acting(session)) if session.session().admin() == kurt),
        "{accepted:?}"
    );
}

/// A cookie whose token is not one cannot carry its session's form token, so the request is refused.
#[tokio::test]
async fn a_change_with_a_malformed_cookie_finds_no_session() {
    let (_database, admins) = migrated().await;
    let form = "ab".repeat(32);
    let jar = cookie("not a token");
    let malformed = Change::new([jar.as_str()], Some(ORIGIN), None, &form);
    assert_eq!(
        admins.session_for_change(&origins(), &malformed).await,
        Ok(ForChange::Refused(FailedCheck::Cookie))
    );
    assert_eq!(
        admins.end_session(&origins(), &malformed, &actor()).await,
        Ok(Logout::Refused(FailedCheck::Cookie))
    );
}

/// A spent session stays spent, however many requests present it at once: none revives it, and it is removed.
#[tokio::test]
async fn requests_at_the_same_moment_never_revive_a_spent_session() {
    let database = ready().await;
    let admins = std::sync::Arc::new(postgres_store(pool(&database.as_app()).expect("a pool")));
    let kurt = admins
        .create(&name("Kurt Medley"), None, &actor())
        .await
        .expect("creates");
    let token = text(&logged_in(&database, kurt).await);
    as_db_admin(
        &database,
        &format!(
            "UPDATE identity.sessions SET last_seen_at = now() - interval '{} seconds'",
            idle_seconds()
        ),
    )
    .await;
    let requests: Vec<_> = (0..16)
        .map(|_| {
            let (admins, token) = (admins.clone(), token.clone());
            tokio::spawn(async move { admins.session([token.as_str()]).await })
        })
        .collect();
    for request in requests {
        assert_eq!(request.await.expect("runs"), Ok(None));
    }
    assert!(!stored(&database, "true").await, "removed");
}

/// Starting a session removes that admin's spent sessions, spent by either limit, and nothing else: not their
/// live ones, so logging in elsewhere logs no one out, and not another admin's.
#[tokio::test]
async fn starting_a_session_removes_only_the_admins_spent_ones() {
    let (database, admins, kurt) = with_kurt().await;
    let ana = admins
        .create(&name("Ana Ortiz"), None, &actor())
        .await
        .expect("creates");
    for admin in [kurt, kurt, kurt, ana] {
        logged_in(&database, admin).await;
    }
    as_db_admin(
        &database,
        &format!(
            "WITH kurts AS (
                 SELECT token_hash, row_number() OVER (ORDER BY token_hash) AS n
                 FROM identity.sessions WHERE admin_id = {kurt}
             )
             UPDATE identity.sessions AS s
             SET ends_at = CASE WHEN k.n = 1 THEN now() ELSE s.ends_at END,
                 last_seen_at = CASE WHEN k.n = 2
                     THEN now() - interval '{idle} seconds' ELSE s.last_seen_at END
             FROM kurts AS k WHERE s.token_hash = k.token_hash;
             UPDATE identity.sessions SET ends_at = now() WHERE admin_id = {ana}",
            kurt = kurt.get(),
            ana = ana.get(),
            idle = idle_seconds()
        ),
    )
    .await;
    logged_in(&database, kurt).await;
    assert_eq!(
        count(&database, &format!("admin_id = {}", kurt.get())).await,
        2,
        "the live one and the new one"
    );
    assert_eq!(
        count(&database, &format!("admin_id = {}", ana.get())).await,
        1,
        "another admin's sessions are not this login's to remove"
    );
}

/// A request that read a session as spent waits for a request refreshing it, and removes it only if it is still
/// spent once that request has finished, so a session that came back to life is never removed.
#[tokio::test]
async fn a_spent_session_refreshed_meanwhile_is_not_removed() {
    let (database, admins, kurt) = with_kurt().await;
    let token = text(&logged_in(&database, kurt).await);
    as_db_admin(
        &database,
        &format!(
            "UPDATE identity.sessions SET last_seen_at = now() - interval '{} seconds'",
            idle_seconds() + 1
        ),
    )
    .await;
    let mut holder = connect(&database).await.expect("connects");
    let refresh = holder.transaction().await.expect("begins");
    refresh
        .execute(
            "UPDATE identity.sessions SET last_seen_at = clock_timestamp()",
            &[],
        )
        .await
        .expect("holds the row");
    let request = {
        let admins = std::sync::Arc::new(admins);
        let token = token.clone();
        tokio::spawn(async move { admins.session([token.as_str()]).await })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;
    refresh.commit().await.expect("commits");
    assert_eq!(
        request.await.expect("runs"),
        Ok(None),
        "it read the spent one"
    );
    assert_eq!(count(&database, "true").await, 1, "but did not remove it");
}

/// Logging out a session that had already ended removes it, and says there was nothing to end.
#[tokio::test]
async fn logging_out_a_spent_session_has_nothing_to_end() {
    let (database, admins, kurt) = with_kurt().await;
    let token = logged_in(&database, kurt).await;
    as_db_admin(&database, "UPDATE identity.sessions SET ends_at = now()").await;
    let (jar, form) = (text(&token), token.form_token().expect("a form token"));
    let logout = Change::new([jar.as_str()], Some(ORIGIN), None, &form);
    assert_eq!(
        admins.session_for_change(&origins(), &logout).await,
        Ok(ForChange::NoSession),
        "the checks pass, but the session has ended"
    );
    logged_in(&database, kurt).await;
    let token = logged_in(&database, kurt).await;
    as_db_admin(
        &database,
        &format!(
            "UPDATE identity.sessions SET last_seen_at = now() - interval '{} seconds'
             WHERE token_hash = sha256(decode('{}', 'hex'))",
            idle_seconds(),
            token.to_text()
        ),
    )
    .await;
    let (jar, form) = (text(&token), token.form_token().expect("a form token"));
    let logout = Change::new([jar.as_str()], Some(ORIGIN), None, &form);
    assert_eq!(
        admins.end_session(&origins(), &logout, &actor()).await,
        Ok(Logout::NothingToEnd)
    );
    assert_eq!(
        count(&database, "true").await,
        1,
        "removed; one live session left"
    );
    assert_eq!(
        logouts_recorded(&database).await,
        0,
        "nothing ended, so nothing recorded"
    );
}

/// A session just inside its idle limit is live, so logging out of it ends it and records that it did (#167).
#[tokio::test]
async fn logging_out_just_inside_the_idle_limit_is_recorded() {
    let (database, admins, kurt) = with_kurt().await;
    let token = logged_in(&database, kurt).await;
    as_db_admin(
        &database,
        &format!(
            "UPDATE identity.sessions SET last_seen_at = now() - interval '{} seconds'",
            idle_seconds() - 10
        ),
    )
    .await;
    let (jar, form) = (text(&token), token.form_token().expect("a form token"));
    let logout = Change::new([jar.as_str()], Some(ORIGIN), None, &form);
    assert_eq!(
        admins.end_session(&origins(), &logout, &actor()).await,
        Ok(Logout::Ended)
    );
    assert_eq!(logouts_recorded(&database).await, 1);
}

/// A logout that finds no session, as when another request removed it first, ended nothing and records nothing.
#[tokio::test]
async fn an_end_that_returns_nothing_ended_nothing() {
    let (database, admins, kurt) = with_kurt().await;
    let token = logged_in(&database, kurt).await;
    as_db_admin(
        &database,
        "DROP FUNCTION identity.log_out(bytea, text);
         CREATE FUNCTION identity.log_out(bytea, text)
         RETURNS TABLE (live boolean, session_id uuid, account_id bigint)
         LANGUAGE sql AS 'SELECT true, NULL::uuid, NULL::bigint WHERE false';
         GRANT EXECUTE ON FUNCTION identity.log_out(bytea, text) TO tetsu_identity",
    )
    .await;
    let (jar, form) = (text(&token), token.form_token().expect("a form token"));
    let logout = Change::new([jar.as_str()], Some(ORIGIN), None, &form);
    assert_eq!(
        admins.end_session(&origins(), &logout, &actor()).await,
        Ok(Logout::NothingToEnd)
    );
    assert_eq!(logouts_recorded(&database).await, 0);
}

/// A logout whose answer storage gives in the wrong shape fails, and records nothing, rather than guessing what ended.
#[tokio::test]
async fn a_logout_answered_in_the_wrong_shape_fails() {
    for (live, answer) in [
        ("text", "SELECT 'yes', gen_random_uuid(), 1::bigint"),
        ("boolean", "SELECT true, NULL::uuid, 1::bigint"),
        ("boolean", "SELECT true, gen_random_uuid(), NULL::bigint"),
    ] {
        let (database, admins, kurt) = with_kurt().await;
        let token = logged_in(&database, kurt).await;
        as_db_admin(
            &database,
            &format!(
                "DROP FUNCTION identity.log_out(bytea, text);
                 CREATE FUNCTION identity.log_out(bytea, text)
                 RETURNS TABLE (live {live}, session_id uuid, account_id bigint)
                 LANGUAGE sql AS $$ {answer} $$;
                 GRANT EXECUTE ON FUNCTION identity.log_out(bytea, text) TO tetsu_identity"
            ),
        )
        .await;
        let (jar, form) = (text(&token), token.form_token().expect("a form token"));
        let logout = Change::new([jar.as_str()], Some(ORIGIN), None, &form);
        let ended = admins.end_session(&origins(), &logout, &actor()).await;
        assert!(
            matches!(ended, Err(Error::Storage(_))),
            "{answer}: {ended:?}"
        );
        assert_eq!(logouts_recorded(&database).await, 0, "{answer}");
    }
}

/// Each session has its own random public id, apart from its token, for a list of sessions to show.
#[tokio::test]
async fn each_session_has_a_random_public_id() {
    let (database, _, kurt) = with_kurt().await;
    for _ in 0..3 {
        logged_in(&database, kurt).await;
    }
    assert_eq!(count(&database, "true").await, 3);
    assert!(
        stored(
            &database,
            "id IS NOT NULL AND (SELECT count(DISTINCT id) FROM identity.sessions) = 3"
        )
        .await
    );
}

/// The longest name the domain accepts fits the database, and a longer one is refused there too (ADR-0060). Names
/// are unique, so the longest is stored once.
#[tokio::test]
async fn the_database_takes_the_longest_name_the_domain_takes() {
    let (database, admins) = migrated().await;
    let longest = "é".repeat(100);
    admins
        .create(&name(&longest), None, &actor())
        .await
        .expect("fits");
    for refused in [String::new(), "é".repeat(101)] {
        let error = connect(&database)
            .await
            .expect("connects")
            .execute(
                "INSERT INTO identity.admins (name) VALUES ($1)",
                &[&refused],
            )
            .await
            .expect_err("refused");
        assert_eq!(
            error.as_db_error().and_then(|db| db.constraint()),
            Some("admins_name_check"),
            "{} characters",
            refused.chars().count()
        );
    }
}

/// The database keeps a passkey's transports to the names `WebAuthn` gives, all five fitting, and refuses any other
/// value or a stray space (ADR-0060).
#[tokio::test]
async fn the_database_keeps_only_named_transports() {
    let (database, admins) = migrated().await;
    let kurt = admins
        .create(&name("Kurt Medley"), None, &actor())
        .await
        .expect("creates")
        .get();
    let client = connect(&database).await.expect("connects");
    let store =
        "INSERT INTO identity.passkeys (credential_id, admin_id, passkey, transports, created_at)
                 VALUES ($1, $2, '{}', $3, now())";
    let mut refused = Vec::new();
    for (n, transports) in [
        "ble hybrid internal nfc usb",
        "",
        "smart-card",
        "usb ",
        "usb  nfc",
    ]
    .into_iter()
    .enumerate()
    {
        let credential = vec![u8::try_from(n).unwrap_or_default(); 16];
        let error = client
            .execute(store, &[&credential, &kurt, &transports])
            .await
            .err();
        refused.push(error.and_then(|error| {
            error
                .as_db_error()
                .and_then(|db| db.constraint().map(str::to_string))
        }));
    }
    let check = Some("passkeys_transports_check".to_string());
    assert_eq!(refused, [None, None, check.clone(), check.clone(), check]);
}

/// A change whose audit event cannot be recorded is undone with it, so nothing changes without its record
/// (ADR-0043, ADR-0045).
#[tokio::test]
async fn a_change_whose_event_cannot_be_recorded_is_undone() {
    let (database, admins) = migrated().await;
    as_db_admin(
        &database,
        "REVOKE INSERT ON identity.outbox_global FROM tetsu_identity",
    )
    .await;
    assert!(matches!(
        admins.create(&name("Kurt Medley"), None, &actor()).await,
        Err(Error::Storage(why)) if why.contains("outbox_global")
    ));
    let client = connect(&database).await.expect("connects");
    let stored: i64 = client
        .query_one("SELECT count(*) FROM identity.admins", &[])
        .await
        .expect("counts")
        .try_get(0)
        .expect("a count");
    assert_eq!(stored, 0);
}

/// A session whose admin was disabled is never used, however it survived, and is removed when presented.
#[tokio::test]
async fn a_disabled_admins_session_is_refused_and_removed() {
    let (database, admins, kurt) = with_kurt().await;
    let token = logged_in(&database, kurt).await;
    as_db_admin(&database, "UPDATE identity.admins SET disabled_at = now()").await;
    assert_eq!(admins.session([text(&token).as_str()]).await, Ok(None));
    assert_eq!(count(&database, "true").await, 0, "removed");
}

/// An admin keeps the email address they were created with, for their invite and notices (ADR-0074); one made
/// without, or one who does not exist, has none.
#[tokio::test]
async fn an_admin_keeps_the_email_address_they_were_created_with() {
    let (_database, admins) = migrated().await;
    let ada = Address::parse("ada@example.com").expect("an address");
    let with = admins
        .create(&name("Ada Lovelace"), Some(&ada), &actor())
        .await
        .expect("created");
    let without = admins
        .create(&name("Lee Park"), None, &actor())
        .await
        .expect("created");
    assert_eq!(admins.email(with).await, Ok(Some(ada)));
    assert_eq!(admins.email(without).await, Ok(None));
    assert_eq!(
        admins.email(AccountId::parse("999").expect("an ID")).await,
        Ok(None)
    );
}

/// A stored address that is not plain, or one that cannot be read, is a storage failure.
#[tokio::test]
async fn an_email_address_that_cannot_be_read_is_a_storage_failure() {
    let (database, admins) = migrated().await;
    let id = admins
        .create(&name("Ada Lovelace"), None, &actor())
        .await
        .expect("created");
    as_db_admin(
        &database,
        "UPDATE identity.admins SET email = 'ada@localhost'",
    )
    .await;
    assert!(matches!(admins.email(id).await, Err(Error::Storage(_))));
    as_db_admin(
        &database,
        "REVOKE SELECT ON identity.admins FROM tetsu_identity",
    )
    .await;
    assert!(matches!(admins.email(id).await, Err(Error::Storage(_))));
}
