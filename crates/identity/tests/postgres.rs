//! Operators and their sessions in Postgres (ADR-0040, ADR-0047, ADR-0048), signed in as the application role, as
//! `tetsu serve` is.

use std::time::{Duration, SystemTime};

use tetsu_identity::{Error, Identity, OperatorName, Realm, postgres_store};
use tetsu_postgres::test_support::{TestDatabase, fresh_database};
use tetsu_postgres::{ModuleTransaction, Settings, connect, isolation_violations, migrate, pool};

const MINUTE: Duration = Duration::from_mins(1);
const HOUR: Duration = Duration::from_hours(1);

#[cfg(test)]
mod support {
    use super::*;

    /// A fresh, migrated database.
    pub async fn ready() -> TestDatabase {
        let database = fresh_database().await.expect("a test database");
        migrate(&database, &[&Identity]).await.expect("migrates");
        database
    }

    /// A moment Postgres keeps exactly: whole seconds, well after the epoch.
    pub fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000 + seconds)
    }

    pub fn name(name: &str) -> OperatorName {
        OperatorName::parse(name).expect("a name")
    }

    /// Replaces `session_for` with one whose `wrong` column comes back as an `int`, and the rest as they should.
    pub fn session_for_with_wrong(wrong: &str) -> String {
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
        format!(
            "DROP FUNCTION identity.session_for(bytea, text);
             CREATE FUNCTION identity.session_for(wanted_hash bytea, wanted_realm text)
             RETURNS TABLE ({}) LANGUAGE sql STABLE AS $$ SELECT {} $$;
             GRANT EXECUTE ON FUNCTION identity.session_for(bytea, text) TO tetsu_identity",
            types.join(", "),
            values.join(", ")
        )
    }

    /// Runs SQL as the test server's administrator, to stand in for a bad migration or a corrupted row.
    pub async fn as_admin(database: &TestDatabase, sql: &str) {
        connect(database)
            .await
            .expect("connects")
            .batch_execute(sql)
            .await
            .expect("runs");
    }
}

use support::{as_admin, at, name, ready, session_for_with_wrong};

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

#[tokio::test]
async fn a_started_session_is_found_by_its_token_and_seen_again() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = operators.start_session(kurt, at(0)).await.expect("starts");
    let session = operators
        .session(&token.to_text(), at(60))
        .await
        .expect("looks up")
        .expect("a session");
    assert_eq!(session.realm(), Realm::Operator);
    assert_eq!(session.operator(), kurt);
    assert_eq!(session.started_at(), at(0));
    assert_eq!(session.last_seen_at(), at(60));
    assert_eq!(session.ends_at(), at(0) + 8 * HOUR);
    let again = operators
        .session(&token.to_text(), at(120))
        .await
        .expect("looks up")
        .expect("still a session");
    assert_eq!(again.last_seen_at(), at(120), "the request was recorded");
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
    let token = operators.start_session(kurt, at(0)).await.expect("starts");
    let found = connect(&database)
        .await
        .expect("connects")
        .query_one(
            "SELECT count(*) FILTER (WHERE token_hash = sha256(decode($1, 'hex'))) AS hashed,
                    count(*) FILTER (WHERE token_hash = decode($1, 'hex')) AS plain
             FROM identity.sessions",
            &[&token.to_text()],
        )
        .await
        .expect("counts");
    assert_eq!(
        (
            found.try_get::<_, i64>("hashed").expect("reads"),
            found.try_get::<_, i64>("plain").expect("reads")
        ),
        (1, 0)
    );
}

/// Fifteen minutes without a request ends an operator's session for good (ADR-0048).
#[tokio::test]
async fn an_idle_session_ends_and_stays_ended() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = operators.start_session(kurt, at(0)).await.expect("starts");
    let late = at(0) + 15 * MINUTE;
    assert_eq!(operators.session(&token.to_text(), late).await, Ok(None));
    assert_eq!(
        operators.session(&token.to_text(), at(60)).await,
        Ok(None),
        "an earlier clock does not bring it back"
    );
}

/// Eight hours after it starts, a session ends however busy it is (ADR-0048).
#[tokio::test]
async fn a_busy_session_still_ends_after_eight_hours() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = operators.start_session(kurt, at(0)).await.expect("starts");
    let mut now = at(0);
    while now < at(0) + 8 * HOUR - 10 * MINUTE {
        now += 10 * MINUTE;
        assert!(
            operators
                .session(&token.to_text(), now)
                .await
                .expect("looks up")
                .is_some()
        );
    }
    assert_eq!(
        operators.session(&token.to_text(), at(0) + 8 * HOUR).await,
        Ok(None)
    );
}

/// Requests that finish out of order never move the last request back, which would shorten the session.
#[tokio::test]
async fn an_earlier_request_seen_late_does_not_shorten_the_session() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = operators.start_session(kurt, at(0)).await.expect("starts");
    for minutes in [10, 5] {
        operators
            .session(&token.to_text(), at(0) + minutes * MINUTE)
            .await
            .expect("looks up")
            .expect("a session");
    }
    assert!(
        operators
            .session(&token.to_text(), at(0) + 24 * MINUTE)
            .await
            .expect("looks up")
            .is_some(),
        "still within 15 minutes of the request at 10"
    );
}

#[tokio::test]
async fn a_token_no_session_holds_finds_none() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    for token in ["", "not a token", &"ab".repeat(32), &"AB".repeat(32)] {
        assert_eq!(operators.session(token, at(0)).await, Ok(None), "{token:?}");
    }
}

/// Signing out ends the session on the server, so its token signs no one in again (ADR-0048).
#[tokio::test]
async fn signing_out_ends_the_session() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = operators.start_session(kurt, at(0)).await.expect("starts");
    let other = operators.start_session(kurt, at(0)).await.expect("starts");
    operators.end_session(&token.to_text()).await.expect("ends");
    assert_eq!(operators.session(&token.to_text(), at(60)).await, Ok(None));
    assert!(
        operators
            .session(&other.to_text(), at(60))
            .await
            .expect("looks up")
            .is_some(),
        "the operator's other session goes on"
    );
    assert_eq!(
        operators.end_session(&token.to_text()).await,
        Ok(()),
        "twice"
    );
    assert_eq!(operators.end_session("not a token").await, Ok(()));
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
        operators.start_session(kurt, at(0)).await,
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
/// operator, who reaches every business (ADR-0040).
#[tokio::test]
async fn another_realms_session_is_not_an_operators() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = operators.start_session(kurt, at(0)).await.expect("starts");
    as_admin(
        &database,
        "ALTER TABLE identity.sessions DROP CONSTRAINT sessions_realm_check;
         UPDATE identity.sessions SET realm = 'staff'",
    )
    .await;
    assert_eq!(operators.session(&token.to_text(), at(60)).await, Ok(None));
}

/// A realm Identity does not know, should a lookup return one, is an error, never a session in some default realm.
#[tokio::test]
async fn an_unknown_realm_is_an_error() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    as_admin(
        &database,
        "DROP FUNCTION identity.session_for(bytea, text);
         CREATE FUNCTION identity.session_for(wanted_hash bytea, wanted_realm text)
         RETURNS TABLE (realm text, operator_id bigint, started_at timestamptz, last_seen_at timestamptz,
                        ends_at timestamptz)
         LANGUAGE sql STABLE AS $$ SELECT 'staff', 1::bigint, now(), now(), now() $$;
         GRANT EXECUTE ON FUNCTION identity.session_for(bytea, text) TO tetsu_identity",
    )
    .await;
    assert_eq!(
        operators.session(&"ab".repeat(32), at(60)).await,
        Err(Error::Storage("unknown realm \"staff\"".to_string()))
    );
}

/// A stored row of the wrong type is an error rather than a session.
#[tokio::test]
async fn a_stored_value_of_the_wrong_type_is_an_error() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = operators.start_session(kurt, at(0)).await.expect("starts");
    as_admin(
        &database,
        "DROP FUNCTION identity.session_for(bytea, text);
         CREATE FUNCTION identity.session_for(wanted_hash bytea, wanted_realm text)
         RETURNS TABLE (realm text, operator_id text, started_at timestamptz, last_seen_at timestamptz,
                        ends_at timestamptz)
         LANGUAGE sql STABLE AS $$ SELECT 'operator', 'one', now(), now(), now() $$;
         GRANT EXECUTE ON FUNCTION identity.session_for(bytea, text) TO tetsu_identity",
    )
    .await;
    assert!(matches!(
        operators.session(&token.to_text(), at(60)).await,
        Err(Error::Storage(_))
    ));
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
        .start_session(kurt, at(0))
        .await
        .expect("caches the statement");
    as_admin(
        &database,
        "DROP FUNCTION identity.start_session(bytea, text, bigint, timestamptz, timestamptz);
         CREATE FUNCTION identity.start_session(bytea, text, bigint, timestamptz, timestamptz)
         RETURNS int LANGUAGE sql AS 'SELECT 1';
         GRANT EXECUTE ON FUNCTION identity.start_session(bytea, text, bigint, timestamptz, timestamptz)
         TO tetsu_identity",
    )
    .await;
    assert!(matches!(
        operators.start_session(kurt, at(0)).await,
        Err(Error::Unavailable(why)) if why.starts_with(tetsu_postgres::STALE)
    ));
    assert!(
        operators.start_session(kurt, at(0)).await.is_ok(),
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
        let kurt = operators
            .create(&name("Kurt Medley"))
            .await
            .expect("creates");
        let token = operators.start_session(kurt, at(0)).await.expect("starts");
        as_admin(&database, &session_for_with_wrong(wrong)).await;
        assert!(
            matches!(
                operators.session(&token.to_text(), at(60)).await,
                Err(Error::Storage(_))
            ),
            "{wrong}"
        );
    }
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
    as_admin(&database, "DROP FUNCTION identity.end_session(bytea)").await;
    assert!(matches!(
        operators.end_session(&"ab".repeat(32)).await,
        Err(Error::Storage(_))
    ));
    as_admin(&database, "DROP FUNCTION identity.session_for(bytea, text)").await;
    assert!(matches!(
        operators.session(&"ab".repeat(32), at(0)).await,
        Err(Error::Storage(_))
    ));
}

/// A request that cannot be recorded, or a spent session that cannot be removed, is an error rather than a session
/// that seems live.
#[tokio::test]
async fn a_session_that_cannot_be_touched_or_removed_is_an_error() {
    let database = ready().await;
    let operators = postgres_store(pool(&database.as_app()).expect("a pool"));
    let kurt = operators
        .create(&name("Kurt Medley"))
        .await
        .expect("creates");
    let token = operators.start_session(kurt, at(0)).await.expect("starts");
    as_admin(
        &database,
        "DROP FUNCTION identity.touch_session(bytea, timestamptz);
         DROP FUNCTION identity.end_session(bytea)",
    )
    .await;
    assert!(matches!(
        operators.session(&token.to_text(), at(60)).await,
        Err(Error::Storage(_))
    ));
    assert!(matches!(
        operators.session(&token.to_text(), at(0) + HOUR).await,
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
        operators.start_session(kurt, at(0)).await.map(drop),
        Err(Error::Storage(
            "commit refused: refused at commit".to_string()
        ))
    );
}
