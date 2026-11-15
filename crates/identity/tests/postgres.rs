//! Operators and their sessions in Postgres (ADR-0040, ADR-0047, ADR-0048), signed in as the application role, as
//! `tetsu serve` is. Postgres's clock is the only one a session follows, so tests move a stored session's times
//! rather than pass a time in.

use std::time::Duration;

use tetsu_identity::{
    Change, Error, ForChange, Identity, OperatorName, Origins, Realm, SessionToken, SignOut,
    postgres_store,
};
use tetsu_postgres::test_support::{TestDatabase, fresh_database};
use tetsu_postgres::{ModuleTransaction, Settings, connect, isolation_violations, migrate, pool};

const ORIGIN: &str = "https://ops.example.com";

#[cfg(test)]
mod support {
    use super::*;

    /// A fresh, migrated database.
    pub async fn ready() -> TestDatabase {
        let database = fresh_database().await.expect("a test database");
        migrate(&database, &[&Identity]).await.expect("migrates");
        database
    }

    pub fn name(name: &str) -> OperatorName {
        OperatorName::parse(name).expect("a name")
    }

    /// The operator realm's idle limit, which the database keeps too; tests age sessions by it, so the two must agree.
    pub fn idle_seconds() -> u64 {
        Realm::Operator.limits().idle.as_secs()
    }

    /// The `Cookie` header a browser sends back with an operator's session token.
    pub fn cookie(token: &str) -> String {
        format!("{}={token}", Realm::Operator.cookie_name())
    }

    pub fn text(token: &SessionToken) -> String {
        cookie(&token.to_text())
    }

    pub fn origins() -> Origins {
        Origins::parse(&[ORIGIN]).expect("origins")
    }

    /// Runs SQL as the test server's administrator, to move a session's times or stand in for a bad migration.
    pub async fn as_admin(database: &TestDatabase, sql: &str) {
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
            ("realm", "text", "'operator'"),
            ("operator_id", "bigint", "1::bigint"),
            ("started_at", "timestamptz", "now()"),
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

use support::{
    as_admin, cookie, count, idle_seconds, name, origins, ready, replaced_use_session, stored,
    text, use_session_with_wrong,
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
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = operators.start_session(kurt).await.expect("starts");
    assert!(
        stored(
            &database,
            &format!(
                "last_seen_at = started_at AND ends_at = started_at + interval '{} seconds'",
                Realm::Operator.limits().absolute.as_secs()
            )
        )
        .await
    );
    as_admin(
        &database,
        "UPDATE identity.sessions SET last_seen_at = last_seen_at - interval '10 minutes'",
    )
    .await;
    let session = operators
        .session([text(&token).as_str()])
        .await
        .expect("looks up")
        .expect("a session");
    assert_eq!(session.realm(), Realm::Operator);
    assert_eq!(session.operator(), kurt);
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
async fn each_operator_gets_their_own_id_and_a_random_user_handle() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let first = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let second = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    assert_ne!(first, second);
    let handles = connect(&database)
        .await
        .expect("connects")
        .query_one(
            "SELECT count(DISTINCT user_handle) AS handles FROM identity.operators",
            &[],
        )
        .await
        .expect("counts");
    assert_eq!(handles.try_get::<_, i64>("handles").expect("reads"), 2);
}

/// Only the token's SHA-256 hash is stored, so the stored row cannot sign anyone in (ADR-0047).
#[tokio::test]
async fn only_the_tokens_hash_is_stored() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = operators.start_session(kurt).await.expect("starts");
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

/// Fifteen minutes without a request ends an operator's session for good, and its row is removed (ADR-0048).
#[tokio::test]
async fn an_idle_session_ends_and_is_removed() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = operators.start_session(kurt).await.expect("starts");
    as_admin(
        &database,
        &format!(
            "UPDATE identity.sessions SET last_seen_at = now() - interval '{} seconds'",
            idle_seconds() - 10
        ),
    )
    .await;
    assert!(
        operators
            .session([text(&token).as_str()])
            .await
            .expect("looks up")
            .is_some(),
        "still within 15 minutes"
    );
    as_admin(
        &database,
        &format!(
            "UPDATE identity.sessions SET last_seen_at = now() - interval '{} seconds'",
            idle_seconds()
        ),
    )
    .await;
    assert_eq!(operators.session([text(&token).as_str()]).await, Ok(None));
    assert!(
        !stored(&database, "true").await,
        "the spent session is removed"
    );
}

/// Eight hours after it starts, a session ends however busy it is, and its row is removed (ADR-0048).
#[tokio::test]
async fn a_busy_session_still_ends_at_its_absolute_limit() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = operators.start_session(kurt).await.expect("starts");
    as_admin(&database, "UPDATE identity.sessions SET ends_at = now()").await;
    assert_eq!(operators.session([text(&token).as_str()]).await, Ok(None));
    assert!(
        !stored(&database, "true").await,
        "the spent session is removed"
    );
}

/// A request handled after a later one never moves the last request back, which would shorten the session.
#[tokio::test]
async fn the_last_request_never_moves_back() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = operators.start_session(kurt).await.expect("starts");
    as_admin(
        &database,
        "UPDATE identity.sessions SET last_seen_at = now() + interval '1 minute'",
    )
    .await;
    operators
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
    let operators = std::sync::Arc::new(postgres_store(pool(&database.as_app()).expect("a pool")));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = text(&operators.start_session(kurt).await.expect("starts"));
    let requests: Vec<_> = (0..16)
        .map(|_| {
            let (operators, token) = (operators.clone(), token.clone());
            tokio::spawn(async move { operators.session([token.as_str()]).await })
        })
        .collect();
    for request in requests {
        assert!(request.await.expect("runs").expect("looks up").is_some());
    }
    assert!(stored(&database, "true").await);
}

#[tokio::test]
async fn a_token_no_session_holds_finds_none() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    for token in ["", "not a token", &"ab".repeat(32), &"AB".repeat(32)] {
        assert_eq!(
            operators.session([cookie(token).as_str()]).await,
            Ok(None),
            "{token:?}"
        );
    }
}

/// Signing out ends the session on the server, so its token signs no one in again; as a change of state, it passes
/// the origin and form-token checks first, and one that fails them ends nothing (ADR-0048).
#[tokio::test]
async fn signing_out_ends_the_session_once_its_checks_pass() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = operators.start_session(kurt).await.expect("starts");
    let (jar, form) = (text(&token), token.form_token().expect("a form token"));
    let sign_out = |origin| Change::new([jar.as_str()], origin, &form);
    let from_elsewhere = sign_out(Some("https://evil.example"));
    assert_eq!(
        operators.end_session(&origins(), &from_elsewhere).await,
        Ok(SignOut::Refused)
    );
    assert!(
        operators
            .session([jar.as_str()])
            .await
            .expect("looks up")
            .is_some(),
        "not ended"
    );
    let own = sign_out(Some(ORIGIN));
    assert_eq!(
        operators.end_session(&origins(), &own).await,
        Ok(SignOut::Ended)
    );
    assert_eq!(operators.session([jar.as_str()]).await, Ok(None));
    assert_eq!(
        operators.end_session(&origins(), &own).await,
        Ok(SignOut::NothingToEnd),
        "nothing left to end"
    );
}

/// Signing out ends that session alone; the operator's others go on.
#[tokio::test]
async fn signing_out_leaves_the_operators_other_sessions() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = operators.start_session(kurt).await.expect("starts");
    let other = operators.start_session(kurt).await.expect("starts");
    let (jar, form) = (text(&token), token.form_token().expect("a form token"));
    let sign_out = Change::new([jar.as_str()], Some(ORIGIN), &form);
    assert_eq!(
        operators.end_session(&origins(), &sign_out).await,
        Ok(SignOut::Ended)
    );
    assert!(
        operators
            .session([text(&other).as_str()])
            .await
            .expect("looks up")
            .is_some()
    );
}

#[tokio::test]
async fn a_session_for_no_operator_is_refused() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    as_admin(&database, "DELETE FROM identity.operators").await;
    assert!(matches!(
        operators.start_session(kurt).await,
        Err(Error::Storage(_))
    ));
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

/// A session in another realm is never an operator's, so a token from another realm's cookie signs no one in as an
/// operator, who reaches every business; nor does the operator realm extend or end it (ADR-0040).
#[tokio::test]
async fn another_realms_session_is_neither_used_nor_ended() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = operators.start_session(kurt).await.expect("starts");
    as_admin(
        &database,
        "ALTER TABLE identity.sessions DROP CONSTRAINT sessions_realm_check;
         UPDATE identity.sessions SET realm = 'staff', last_seen_at = now() - interval '1 hour'",
    )
    .await;
    assert_eq!(operators.session([text(&token).as_str()]).await, Ok(None));
    let (jar, form) = (text(&token), token.form_token().expect("a form token"));
    let sign_out = Change::new([jar.as_str()], Some(ORIGIN), &form);
    assert_eq!(
        operators.end_session(&origins(), &sign_out).await,
        Ok(SignOut::NothingToEnd),
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
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    as_admin(
        &database,
        &replaced_use_session(
            "realm text, operator_id bigint, started_at timestamptz, last_seen_at timestamptz, ends_at timestamptz",
            "'staff', 1::bigint, now(), now(), now()",
        ),
    )
    .await;
    assert_eq!(
        operators.session([cookie(&"ab".repeat(32)).as_str()]).await,
        Err(Error::Storage("unknown realm \"staff\"".to_string()))
    );
}

/// Each stored column of the wrong type is an error rather than a session, whichever column it is.
#[tokio::test]
async fn each_stored_column_of_the_wrong_type_is_an_error() {
    let columns = [
        "realm",
        "operator_id",
        "started_at",
        "last_seen_at",
        "ends_at",
    ];
    for wrong in columns {
        let database = ready().await;
        let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
        as_admin(&database, &use_session_with_wrong(wrong)).await;
        assert!(
            matches!(
                operators.session([cookie(&"ab".repeat(32)).as_str()]).await,
                Err(Error::Storage(_))
            ),
            "{wrong}"
        );
    }
}

#[tokio::test]
async fn an_insert_that_returns_no_id_is_an_error() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    as_admin(
        &database,
        "CREATE FUNCTION identity.skip() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NULL; END $$;
         CREATE TRIGGER skip BEFORE INSERT ON identity.operators FOR EACH ROW EXECUTE FUNCTION identity.skip()",
    )
    .await;
    assert_eq!(
        operators.create(&name("Kurt Medley")).await,
        Err(Error::Storage("no operator id returned".to_string()))
    );
}

/// A cached statement a migration made stale is temporary, and once the caches are cleared the next request
/// succeeds (ADR-0011).
#[tokio::test]
async fn a_stale_cached_statement_is_temporary_and_cleared() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    operators
        .start_session(kurt)
        .await
        .expect("caches the statement");
    as_admin(
        &database,
        "DROP FUNCTION identity.start_session(bytea, text, bigint);
         CREATE FUNCTION identity.start_session(bytea, text, bigint)
         RETURNS int LANGUAGE sql AS 'SELECT 1';
         GRANT EXECUTE ON FUNCTION identity.start_session(bytea, text, bigint) TO tetsu_identity",
    )
    .await;
    assert!(matches!(
        operators.start_session(kurt).await,
        Err(Error::Unavailable(why)) if why.starts_with(tetsu_postgres::STALE)
    ));
    assert!(
        operators.start_session(kurt).await.is_ok(),
        "the next request succeeds"
    );
}

#[tokio::test]
async fn an_unreachable_database_is_an_error() {
    let database = ready().await;
    let url = database
        .url()
        .replacen(database.name(), "no_such_database", 1);
    let operators =
        postgres_store(pool(&Settings::from_values(&url, Some("disable"))).expect("a pool"));
    assert!(matches!(
        operators.create(&name("Kurt Medley")).await,
        Err(Error::Storage(_))
    ));
}

#[tokio::test]
async fn an_operator_id_of_the_wrong_type_is_an_error() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    as_admin(
        &database,
        "ALTER TABLE identity.sessions DROP CONSTRAINT sessions_operator_id_fkey;
         DROP TABLE identity.operators;
         CREATE TABLE identity.operators (id text NOT NULL DEFAULT 'one', name text NOT NULL);
         GRANT SELECT, INSERT ON identity.operators TO tetsu_identity",
    )
    .await;
    assert!(matches!(
        operators.create(&name("Kurt Medley")).await,
        Err(Error::Storage(_))
    ));
}

/// A transaction Identity's role cannot begin, as for a server login that may not act as it, is an error. The login
/// is this test's own, so no other test's roles change.
#[tokio::test]
async fn a_transaction_that_cannot_begin_is_an_error() {
    let database = ready().await;
    let outsider = format!("outsider_{}", database.name());
    as_admin(&database, &format!("CREATE ROLE {outsider} LOGIN")).await;
    let url = database
        .url()
        .replacen("postgres@", &format!("{outsider}@"), 1);
    let operators =
        postgres_store(pool(&Settings::from_values(&url, Some("disable"))).expect("a pool"));
    let refused = operators.create(&name("Kurt Medley")).await;
    drop(operators);
    as_admin(&database, &format!("DROP ROLE {outsider}")).await;
    assert!(matches!(refused, Err(Error::Storage(_))), "{refused:?}");
}

#[tokio::test]
async fn a_statement_that_cannot_be_prepared_is_an_error() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = operators.start_session(kurt).await.expect("starts");
    let (jar, form) = (text(&token), token.form_token().expect("a form token"));
    let sign_out = Change::new([jar.as_str()], Some(ORIGIN), &form);
    as_admin(&database, "DROP FUNCTION identity.end_session(bytea, text)").await;
    assert!(matches!(
        operators.end_session(&origins(), &sign_out).await,
        Err(Error::Storage(_))
    ));
    as_admin(&database, "DROP FUNCTION identity.use_session(bytea, text)").await;
    assert!(matches!(
        operators.session([text(&token).as_str()]).await,
        Err(Error::Storage(_))
    ));
    assert!(matches!(
        operators.session_for_change(&origins(), &sign_out).await,
        Err(Error::Storage(_))
    ));
}

/// A commit Postgres refuses was rolled back, so no session starts, and the refusal is never worth retrying.
#[tokio::test]
async fn a_refused_commit_starts_no_session() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    as_admin(
        &database,
        "CREATE FUNCTION identity.refuse() RETURNS trigger LANGUAGE plpgsql
         AS $$ BEGIN RAISE EXCEPTION 'refused at commit'; END $$;
         CREATE CONSTRAINT TRIGGER refuse AFTER INSERT ON identity.sessions
         DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION identity.refuse()",
    )
    .await;
    assert_eq!(
        operators.start_session(kurt).await.map(drop),
        Err(Error::Storage(
            "commit refused: refused at commit".to_string()
        ))
    );
}

/// A state-changing request acts for its session only from the realm's own origin and with that session's own form
/// token; failing either finds no session and leaves it as it was (ADR-0048).
#[tokio::test]
async fn a_change_needs_the_realms_origin_and_the_sessions_form_token() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = operators.start_session(kurt).await.expect("starts");
    let other = operators.start_session(kurt).await.expect("starts");
    as_admin(
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
    let change = |origin, form_token| Change::new([jar.as_str()], origin, form_token);
    for refused in [
        change(None, &form),
        change(Some("null"), &form),
        change(Some("https://evil.example"), &form),
        change(Some(ORIGIN), ""),
        change(Some(ORIGIN), &others),
        change(Some(ORIGIN), &session),
    ] {
        assert_eq!(
            operators.session_for_change(&origins(), &refused).await,
            Ok(ForChange::Refused)
        );
    }
    assert!(
        stored(&database, "last_seen_at < now() - interval '9 minutes'").await,
        "no refused request was recorded"
    );
    let accepted = operators
        .session_for_change(&origins(), &change(Some(ORIGIN), &form))
        .await;
    assert!(
        matches!(&accepted, Ok(ForChange::Acting(session)) if session.operator() == kurt),
        "{accepted:?}"
    );
}

/// A cookie whose token is not one cannot carry its session's form token, so the request is refused.
#[tokio::test]
async fn a_change_with_a_malformed_cookie_finds_no_session() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let form = "ab".repeat(32);
    let jar = cookie("not a token");
    let malformed = Change::new([jar.as_str()], Some(ORIGIN), &form);
    assert_eq!(
        operators.session_for_change(&origins(), &malformed).await,
        Ok(ForChange::Refused)
    );
    assert_eq!(
        operators.end_session(&origins(), &malformed).await,
        Ok(SignOut::Refused)
    );
}

/// A spent session stays spent, however many requests present it at once: none revives it, and it is removed.
#[tokio::test]
async fn requests_at_the_same_moment_never_revive_a_spent_session() {
    let database = ready().await;
    let operators = std::sync::Arc::new(postgres_store(pool(&database.as_app()).expect("a pool")));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = text(&operators.start_session(kurt).await.expect("starts"));
    as_admin(
        &database,
        &format!(
            "UPDATE identity.sessions SET last_seen_at = now() - interval '{} seconds'",
            idle_seconds()
        ),
    )
    .await;
    let requests: Vec<_> = (0..16)
        .map(|_| {
            let (operators, token) = (operators.clone(), token.clone());
            tokio::spawn(async move { operators.session([token.as_str()]).await })
        })
        .collect();
    for request in requests {
        assert_eq!(request.await.expect("runs"), Ok(None));
    }
    assert!(!stored(&database, "true").await, "removed");
}

/// Starting a session removes that operator's spent sessions, spent by either limit, and nothing else: not their
/// live ones, so signing in elsewhere signs no one out, and not another operator's.
#[tokio::test]
async fn starting_a_session_removes_only_the_operators_spent_ones() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let ana = operators.create(&name("Ana Ortiz")).await.expect("creates");
    for operator in [kurt, kurt, kurt, ana] {
        operators.start_session(operator).await.expect("starts");
    }
    as_admin(
        &database,
        &format!(
            "WITH kurts AS (
                 SELECT token_hash, row_number() OVER (ORDER BY token_hash) AS n
                 FROM identity.sessions WHERE operator_id = {kurt}
             )
             UPDATE identity.sessions AS s
             SET ends_at = CASE WHEN k.n = 1 THEN now() ELSE s.ends_at END,
                 last_seen_at = CASE WHEN k.n = 2
                     THEN now() - interval '{idle} seconds' ELSE s.last_seen_at END
             FROM kurts AS k WHERE s.token_hash = k.token_hash;
             UPDATE identity.sessions SET ends_at = now() WHERE operator_id = {ana}",
            kurt = kurt.get(),
            ana = ana.get(),
            idle = idle_seconds()
        ),
    )
    .await;
    operators.start_session(kurt).await.expect("starts");
    assert_eq!(
        count(&database, &format!("operator_id = {}", kurt.get())).await,
        2,
        "the live one and the new one"
    );
    assert_eq!(
        count(&database, &format!("operator_id = {}", ana.get())).await,
        1,
        "another operator's sessions are not this sign-in's to remove"
    );
}

/// A request that read a session as spent waits for a request refreshing it, and removes it only if it is still
/// spent once that request has finished, so a session that came back to life is never removed.
#[tokio::test]
async fn a_spent_session_refreshed_meanwhile_is_not_removed() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = text(&operators.start_session(kurt).await.expect("starts"));
    as_admin(
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
        let operators = std::sync::Arc::new(operators);
        let token = token.clone();
        tokio::spawn(async move { operators.session([token.as_str()]).await })
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

/// Signing out a session that had already ended removes it, and says there was nothing to end.
#[tokio::test]
async fn signing_out_a_spent_session_has_nothing_to_end() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = operators.start_session(kurt).await.expect("starts");
    as_admin(&database, "UPDATE identity.sessions SET ends_at = now()").await;
    let (jar, form) = (text(&token), token.form_token().expect("a form token"));
    let sign_out = Change::new([jar.as_str()], Some(ORIGIN), &form);
    assert_eq!(
        operators.session_for_change(&origins(), &sign_out).await,
        Ok(ForChange::NoSession),
        "the checks pass, but the session has ended"
    );
    operators.start_session(kurt).await.expect("starts");
    let token = operators.start_session(kurt).await.expect("starts");
    as_admin(
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
    let sign_out = Change::new([jar.as_str()], Some(ORIGIN), &form);
    assert_eq!(
        operators.end_session(&origins(), &sign_out).await,
        Ok(SignOut::NothingToEnd)
    );
    assert_eq!(
        count(&database, "true").await,
        1,
        "removed; one live session left"
    );
}

/// A lookup that, against its contract, returns no row says no session ended.
#[tokio::test]
async fn an_end_that_returns_nothing_ended_nothing() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = operators.start_session(kurt).await.expect("starts");
    as_admin(
        &database,
        "DROP FUNCTION identity.end_session(bytea, text);
         CREATE FUNCTION identity.end_session(bytea, text) RETURNS SETOF boolean
         LANGUAGE sql AS 'SELECT true WHERE false';
         GRANT EXECUTE ON FUNCTION identity.end_session(bytea, text) TO tetsu_identity",
    )
    .await;
    let (jar, form) = (text(&token), token.form_token().expect("a form token"));
    let sign_out = Change::new([jar.as_str()], Some(ORIGIN), &form);
    assert_eq!(
        operators.end_session(&origins(), &sign_out).await,
        Ok(SignOut::NothingToEnd)
    );
}

/// Each session has its own random public id, apart from its token, for a list of sessions to show.
#[tokio::test]
async fn each_session_has_a_random_public_id() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    for _ in 0..3 {
        operators.start_session(kurt).await.expect("starts");
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

/// The longest name the domain accepts fits the database, and a longer one is refused there too (ADR-0060).
#[tokio::test]
async fn the_database_takes_the_longest_name_the_domain_takes() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let longest = "é".repeat(100);
    operators.create(&name(&longest)).await.expect("fits");
    for refused in [String::new(), "é".repeat(101)] {
        let error = connect(&database)
            .await
            .expect("connects")
            .execute(
                "INSERT INTO identity.operators (name) VALUES ($1)",
                &[&refused],
            )
            .await
            .expect_err("refused");
        assert_eq!(
            error.as_db_error().and_then(|db| db.constraint()),
            Some("operators_name_check"),
            "{} characters",
            refused.chars().count()
        );
    }
}
