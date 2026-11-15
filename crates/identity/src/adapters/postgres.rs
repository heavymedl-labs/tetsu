//! Identity's Postgres adapter. Driver types stay in this file (ADR-0007).

use std::sync::OnceLock;

use serde_json::{Value, json};
use tetsu_kernel::Migration;
use tetsu_postgres::{Failure, ModuleTransaction, NewEvent, Pool};
use tokio_postgres::Row;
use tokio_postgres::types::{FromSql, ToSql};

use crate::domain::{
    Actor, Disabling, Error, OperatorId, OperatorName, OperatorSummary, Realm, Session,
};
use crate::ports::OperatorStore;
use crate::use_cases::Operators;

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: include_str!("../../migrations/0001_operators.sql"),
    },
    // Identity records what the operator command does, for the audit record (ADR-0043, ADR-0045).
    Migration {
        version: 2,
        sql: tetsu_postgres::OUTBOX,
    },
];
/// Identity's schema, which names the role its transactions act as (ADR-0042).
const SCHEMA: &str = "identity";

const INSERT_OPERATOR: &str = include_str!("../../sql/insert_operator.sql");
const START_SESSION: &str = include_str!("../../sql/start_session.sql");
const USE_SESSION: &str = include_str!("../../sql/use_session.sql");
const END_SESSION: &str = include_str!("../../sql/end_session.sql");
const ISSUE_ENROLLMENT: &str = include_str!("../../sql/issue_enrollment.sql");
const DISABLE_OPERATOR: &str = include_str!("../../sql/disable_operator.sql");
const END_OPERATOR_SESSIONS: &str = include_str!("../../sql/end_operator_sessions.sql");
const END_REALM_SESSIONS: &str = include_str!("../../sql/end_realm_sessions.sql");
const LIST_OPERATORS: &str = include_str!("../../sql/list_operators.sql");
const ENABLE_OPERATOR: &str = include_str!("../../sql/enable_operator.sql");

/// An event for the audit record: its type, and a payload of IDs and facts, never names or tokens (ADR-0045).
type Recorded = Option<(&'static str, String)>;
/// What makes a statement's event of the rows it returned.
type MakesEvent<'a> = &'a (dyn Fn(&[Row]) -> Result<Recorded, Error> + Sync);

/// Identity's operator realm over Postgres, for the composition root. It returns the use cases, never the store
/// itself (ADR-0021).
///
/// The use cases are there:
///
/// ```no_run
/// # async fn reach(pool: tetsu_postgres::Pool) {
/// let _ = tetsu_identity::postgres_store(pool).session(["x"]).await;
/// # }
/// ```
///
/// The handle is not a store, whatever the port's methods are called:
///
/// ```compile_fail,E0277
/// # fn bypass(pool: tetsu_postgres::Pool) {
/// fn store(_: &impl tetsu_identity::OperatorStore) {}
/// store(&tetsu_identity::postgres_store(pool));
/// # }
/// ```
///
/// It derefs to nothing, so no store's methods are reached through it:
///
/// ```compile_fail,E0277
/// # fn bypass(pool: tetsu_postgres::Pool) {
/// fn derefs(_: &impl std::ops::Deref) {}
/// derefs(&tetsu_identity::postgres_store(pool));
/// # }
/// ```
///
/// And the store inside it is private:
///
/// ```compile_fail,E0616
/// # fn bypass(pool: tetsu_postgres::Pool) {
/// let _ = &tetsu_identity::postgres_store(pool).store;
/// # }
/// ```
#[must_use]
pub fn postgres_store(pool: Pool) -> Operators<impl OperatorStore> {
    Operators::new(PostgresOperators { pool })
}

struct PostgresOperators {
    pool: Pool,
}

/// Every statement runs alone in a transaction as Identity's role, which reaches sessions only through its lookup
/// functions (ADR-0042).
impl PostgresOperators {
    async fn query(
        &self,
        statement: &str,
        parameters: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<Row>, Error> {
        self.query_recording(statement, parameters, &|_| Ok(None))
            .await
    }

    /// Runs a lookup function that answers yes, no, or nothing in its `done` column, and records the event `event`
    /// makes of the answer, whatever it is, in the same transaction.
    async fn change(
        &self,
        statement: &str,
        parameters: &[&(dyn ToSql + Sync)],
        event: impl Fn(Option<bool>) -> (&'static str, String) + Sync,
    ) -> Result<Option<bool>, Error> {
        let done = OnceLock::new();
        self.query_recording(statement, parameters, &|rows| {
            let answer = answered(rows, "done")?;
            let _ = done.set(answer);
            Ok(Some(event(answer)))
        })
        .await?;
        Ok(done.get().copied().flatten())
    }

    /// Runs a lookup function that ends sessions and counts them, or answers nothing for an unknown operator, and
    /// records the event `event` makes of the count.
    async fn ended(
        &self,
        statement: &str,
        parameters: &[&(dyn ToSql + Sync)],
        event: impl Fn(Option<u64>) -> (&'static str, String) + Sync,
    ) -> Result<Option<u64>, Error> {
        let ended = OnceLock::new();
        self.query_recording(statement, parameters, &|rows| {
            let count = counted(rows, "ended")?;
            let _ = ended.set(count);
            Ok(Some(event(count)))
        })
        .await?;
        Ok(ended.get().copied().flatten())
    }

    /// Runs a statement, and records the event `event` makes of what it returned, if any, in the same transaction,
    /// so a change is never left unrecorded, nor an event recorded for a change that rolled back. One build of this
    /// serves every caller, so every path through it is tested once.
    async fn query_recording(
        &self,
        statement: &str,
        parameters: &[&(dyn ToSql + Sync)],
        event: MakesEvent<'_>,
    ) -> Result<Vec<Row>, Error> {
        let mut client = self.pool.get().await.map_err(pool_failed)?;
        let result = run(&mut client, statement, parameters, event).await;
        if let Err(Error::Unavailable(why)) = &result {
            tetsu_postgres::clear_if_stale(&self.pool, why);
        }
        result
    }
}

async fn run(
    client: &mut tetsu_postgres::Object,
    statement: &str,
    parameters: &[&(dyn ToSql + Sync)],
    event: MakesEvent<'_>,
) -> Result<Vec<Row>, Error> {
    let transaction = ModuleTransaction::begin(client, SCHEMA)
        .await
        .map_err(|e| sorted(&e))?;
    let prepared = transaction
        .prepare_cached(statement)
        .await
        .map_err(|e| sorted(&e))?;
    let rows = transaction
        .query(&prepared, parameters)
        .await
        .map_err(|e| sorted(&e))?;
    if let Some((kind, payload)) = event(&rows)? {
        let event = NewEvent {
            kind,
            version: 1,
            payload: &payload,
            keep: false,
        };
        transaction.record(&event).await.map_err(|e| sorted(&e))?;
    }
    transaction.commit().await.map_err(|e| commit_failed(&e))?;
    Ok(rows)
}

/// The SQLSTATE Identity's own functions raise for an operator who is disabled or unknown: a code of Identity's,
/// so no other error is ever taken for it.
const NO_ENABLED_OPERATOR: &str = "TD001";

/// An operator who is disabled or unknown is the caller's mistake, not storage failing.
fn sorted(error: &tokio_postgres::Error) -> Error {
    match error.as_db_error() {
        Some(db) if db.code().code() == NO_ENABLED_OPERATOR => {
            Error::Invalid("no enabled operator".to_string())
        }
        _ => failed(tetsu_postgres::sort(error)),
    }
}

fn commit_failed(error: &tokio_postgres::Error) -> Error {
    failed(tetsu_postgres::sort_commit(error))
}

fn pool_failed(error: deadpool_postgres::PoolError) -> Error {
    failed(tetsu_postgres::sort_pool(error))
}

/// Identity holds no row a caller may take by its value, so a taken row is a storage failure like any other.
fn failed(failure: Failure) -> Error {
    match failure {
        Failure::Temporary(why) => Error::Unavailable(why),
        Failure::Taken { message, .. } | Failure::Permanent(message) => Error::Storage(message),
    }
}

fn column<'a, T: FromSql<'a>>(row: &'a Row, name: &str) -> Result<T, Error> {
    row.try_get(name).map_err(|e| sorted(&e))
}

impl OperatorStore for PostgresOperators {
    async fn insert_operator(
        &self,
        name: &OperatorName,
        actor: &Actor,
    ) -> Result<OperatorId, Error> {
        let added = OnceLock::new();
        self.query_recording(INSERT_OPERATOR, &[&name.as_str()], &|rows| {
            creation(rows, actor, &added)
        })
        .await?;
        created(added.get().copied().flatten())
    }

    async fn insert_session(
        &self,
        hash: &[u8; 32],
        realm: Realm,
        operator: OperatorId,
    ) -> Result<(), Error> {
        let parameters: [&(dyn ToSql + Sync); 3] =
            [&hash.as_slice(), &realm.as_str(), &operator.get()];
        self.query(START_SESSION, &parameters).await.map(drop)
    }

    async fn use_session(&self, hash: &[u8; 32], realm: Realm) -> Result<Option<Session>, Error> {
        let rows = self
            .query(USE_SESSION, &[&hash.as_slice(), &realm.as_str()])
            .await?;
        rows.first().map(session_from).transpose()
    }

    async fn delete_session(&self, hash: &[u8; 32], realm: Realm) -> Result<bool, Error> {
        let rows = self
            .query(END_SESSION, &[&hash.as_slice(), &realm.as_str()])
            .await?;
        rows.first().map_or(Ok(false), |row| column(row, "ended"))
    }

    async fn issue_enrollment(
        &self,
        hash: &[u8; 32],
        operator: OperatorId,
        actor: &Actor,
    ) -> Result<Option<bool>, Error> {
        let id = operator.get();
        let parameters: [&(dyn ToSql + Sync); 2] = [&hash.as_slice(), &id];
        self.change(ISSUE_ENROLLMENT, &parameters, |issued| {
            let outcome = if issued == Some(true) {
                "changed"
            } else {
                "refused"
            };
            event(
                "identity.enrollment_issued",
                actor,
                outcome,
                json!({"operator_id": id}),
            )
        })
        .await
    }

    async fn disable_operator(
        &self,
        operator: OperatorId,
        actor: &Actor,
    ) -> Result<Option<Disabling>, Error> {
        let id = operator.get();
        let disabling = OnceLock::new();
        self.query_recording(DISABLE_OPERATOR, &[&id], &|rows| {
            let found = rows.first().map(disabling_from).transpose()?.flatten();
            let _ = disabling.set(found);
            let payload = found.map_or_else(
                || json!({"operator_id": id}),
                |done| {
                    json!({
                        "operator_id": id,
                        "sessions_ended": done.sessions_ended,
                        "links_ended": done.links_ended,
                    })
                },
            );
            let outcome = outcome(found.map(|done| done.done));
            Ok(Some(event(
                "identity.operator_disabled",
                actor,
                outcome,
                payload,
            )))
        })
        .await?;
        Ok(disabling.get().copied().flatten())
    }

    async fn enable_operator(
        &self,
        operator: OperatorId,
        actor: &Actor,
    ) -> Result<Option<bool>, Error> {
        let id = operator.get();
        self.change(ENABLE_OPERATOR, &[&id], |enabled| {
            let payload = json!({"operator_id": id});
            event(
                "identity.operator_enabled",
                actor,
                outcome(enabled),
                payload,
            )
        })
        .await
    }

    async fn end_operator_sessions(
        &self,
        operator: OperatorId,
        realm: Realm,
        actor: &Actor,
    ) -> Result<Option<u64>, Error> {
        let (id, name) = (operator.get(), realm.as_str());
        let parameters: [&(dyn ToSql + Sync); 2] = [&id, &name];
        self.ended(END_OPERATOR_SESSIONS, &parameters, |ended| {
            let payload = json!({"operator_id": id, "realm": name, "ended": ended});
            event(
                "identity.sessions_ended",
                actor,
                outcome(ended.map(|ended| ended > 0)),
                payload,
            )
        })
        .await
    }

    async fn end_realm_sessions(&self, realm: Realm, actor: &Actor) -> Result<u64, Error> {
        let name = realm.as_str();
        let ended = self
            .ended(END_REALM_SESSIONS, &[&name], |ended| {
                let payload = json!({"realm": name, "ended": ended});
                event(
                    "identity.sessions_ended",
                    actor,
                    outcome(ended.map(|ended| ended > 0)),
                    payload,
                )
            })
            .await?;
        Ok(ended.unwrap_or_default())
    }

    async fn list_operators(&self) -> Result<Vec<OperatorSummary>, Error> {
        self.query(LIST_OPERATORS, &[])
            .await?
            .iter()
            .map(|row| {
                Ok(OperatorSummary {
                    id: OperatorId::new(column(row, "id")?),
                    name: column(row, "name")?,
                    created_at: column(row, "created_at")?,
                    disabled_at: column(row, "disabled_at")?,
                })
            })
            .collect()
    }
}

/// An audit event: its type, and a payload of IDs, counts, and facts, never names or tokens, with who ran the
/// command and what came of it: `changed`, `unchanged`, or `refused` (ADR-0045, `v5.0.0-16.2.1`).
fn event(
    kind: &'static str,
    actor: &Actor,
    outcome: &str,
    mut payload: Value,
) -> (&'static str, String) {
    payload["actor"] = json!(actor.as_str());
    payload["outcome"] = json!(outcome);
    (kind, payload.to_string())
}

/// The yes, no, or nothing a lookup function answers in a column; no row at all is nothing too.
fn answered(rows: &[Row], name: &str) -> Result<Option<bool>, Error> {
    rows.first().map_or(Ok(None), |row| column(row, name))
}

/// The count a lookup function answers, or nothing for an operator who does not exist. Postgres's count is never
/// negative, so the conversion cannot fail.
fn counted(rows: &[Row], name: &str) -> Result<Option<u64>, Error> {
    let ended: Option<i64> = rows.first().map_or(Ok(None), |row| column(row, name))?;
    Ok(ended.map(|ended| u64::try_from(ended).unwrap_or_default()))
}

/// Reads what a create found, keeps it in `added`, and makes the event it records: changed when it added the
/// operator, and refused when the name was taken, naming
/// who holds it if the statement could see them.
fn creation(
    rows: &[Row],
    actor: &Actor,
    added: &OnceLock<Option<(i64, bool)>>,
) -> Result<Option<(&'static str, String)>, Error> {
    let found = rows.first().map(added_from).transpose()?;
    let _ = added.set(found);
    Ok(Some(creation_event(found, actor)))
}

/// The event a create records for what it found.
fn creation_event(found: Option<(i64, bool)>, actor: &Actor) -> (&'static str, String) {
    let outcome = if matches!(found, Some((_, true))) {
        "changed"
    } else {
        "refused"
    };
    event(
        "identity.operator_created",
        actor,
        outcome,
        json!({"operator_id": found.map(|(id, _)| id)}),
    )
}

/// What a create returns: the new operator, or why there is none.
fn created(found: Option<(i64, bool)>) -> Result<OperatorId, Error> {
    match found {
        Some((id, true)) => Ok(OperatorId::new(id)),
        Some((id, false)) => Err(Error::Invalid(format!(
            "operator {id} already has that name"
        ))),
        None => Err(Error::Invalid(
            "another operator already has that name".to_string(),
        )),
    }
}

/// The operator a create added, or the one already holding its name.
fn added_from(row: &Row) -> Result<(i64, bool), Error> {
    column(row, "id").and_then(|id| column(row, "created").map(|created| (id, created)))
}

/// What a disable did, or nothing for an operator who does not exist.
fn disabling_from(row: &Row) -> Result<Option<Disabling>, Error> {
    let done: Option<bool> = column(row, "done")?;
    let (sessions, links): (i64, i64) =
        (column(row, "sessions_ended")?, column(row, "links_ended")?);
    Ok(done.map(|done| Disabling {
        done,
        sessions_ended: u64::try_from(sessions).unwrap_or_default(),
        links_ended: u64::try_from(links).unwrap_or_default(),
    }))
}

/// The outcome of a change: it acted, it found nothing to do, or there was no such operator.
fn outcome(acted: Option<bool>) -> &'static str {
    match acted {
        Some(true) => "changed",
        Some(false) => "unchanged",
        None => "refused",
    }
}

/// A stored realm Identity does not know is an error, never a session in some default realm.
fn session_from(row: &Row) -> Result<Session, Error> {
    let realm: String = column(row, "realm")?;
    let Some(known) = Realm::parse(&realm) else {
        return Err(Error::Storage(format!("unknown realm {realm:?}")));
    };
    Ok(Session {
        realm: known,
        operator: OperatorId::new(column(row, "operator_id")?),
        started_at: column(row, "started_at")?,
        last_seen_at: column(row, "last_seen_at")?,
        ends_at: column(row, "ends_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(test)]
    mod support {
        use tetsu_postgres::Pool;
        use tetsu_postgres::test_support::{TestDatabase, fresh_database};

        /// A pool signed in as the application role, over a fresh database Identity has migrated, kept while
        /// the test runs.
        pub async fn migrated() -> (Pool, TestDatabase) {
            let database = fresh_database().await.expect("a test database");
            tetsu_postgres::migrate(&database, &[&crate::Identity])
                .await
                .expect("migrates");
            let pool = tetsu_postgres::pool(&database.as_app()).expect("a pool");
            (pool, database)
        }

        /// Who runs the operator command in these tests.
        pub fn actor() -> crate::Actor {
            crate::Actor::parse("kurt@workstation").expect("an actor")
        }
    }

    /// A commit Postgres refused was rolled back, so it is never sorted as worth retrying.
    #[tokio::test]
    async fn a_refused_commit_is_a_storage_failure() {
        let database = tetsu_postgres::test_support::fresh_database()
            .await
            .expect("a test database");
        let client = tetsu_postgres::connect(&database).await.expect("connects");
        let error = client.batch_execute("SELEC").await.expect_err("refused");
        assert_eq!(
            commit_failed(&error),
            Error::Storage("commit refused: syntax error at or near \"SELEC\"".to_string())
        );
    }

    /// With no secure random source, no session starts, and nothing is stored.
    #[tokio::test]
    async fn no_session_starts_without_randomness() {
        let (pool, _database) = support::migrated().await;
        let operators = Operators::with_fill(PostgresOperators { pool: pool.clone() }, |_| {
            Err(getrandom::Error::UNSUPPORTED)
        });
        let kurt = operators
            .create(
                &OperatorName::parse("Kurt Medley").expect("a name"),
                &support::actor(),
            )
            .await
            .expect("creates");
        assert_eq!(
            operators.start_session(kurt).await,
            Err(Error::Unavailable(format!(
                "no secure random source: {}",
                getrandom::Error::UNSUPPORTED
            )))
        );
        // The same build, with the operating system's source, starts one: llvm-cov scores each build of a
        // function on its own, so this one must take both paths.
        let working = Operators::with_fill(PostgresOperators { pool }, getrandom::fill);
        assert!(working.start_session(kurt).await.is_ok());
        assert_eq!(
            working.start_session(OperatorId::new(i64::MAX)).await,
            Err(Error::Invalid(format!("no enabled operator {}", i64::MAX))),
            "an unknown operator starts no session"
        );
    }

    /// Identity holds no row a caller takes by its value, so nothing it stores can be taken today; were one, it
    /// would be a storage failure. Only a temporary failure is worth retrying.
    #[test]
    fn each_failure_has_its_error() {
        assert_eq!(
            failed(Failure::Temporary("dropped".to_string())),
            Error::Unavailable("dropped".to_string())
        );
        assert_eq!(
            failed(Failure::Permanent("refused".to_string())),
            Error::Storage("refused".to_string())
        );
        assert_eq!(
            failed(Failure::Taken {
                constraint: None,
                message: "taken".to_string()
            }),
            Error::Storage("taken".to_string())
        );
    }

    /// A create says what it found and records it, in this one build, which llvm-cov scores on its own: an
    /// operator added, a name another holds, or a name taken by one it cannot yet see.
    #[test]
    fn a_create_reports_and_records_what_it_found() {
        assert_eq!(created(Some((1, true))), Ok(OperatorId::new(1)));
        assert_eq!(
            created(Some((1, false))),
            Err(Error::Invalid(
                "operator 1 already has that name".to_string()
            ))
        );
        assert_eq!(
            created(None),
            Err(Error::Invalid(
                "another operator already has that name".to_string()
            ))
        );
        let recorded = |found| {
            let (kind, payload) = creation_event(found, &support::actor());
            let payload: serde_json::Value = serde_json::from_str(&payload).expect("JSON");
            (
                kind,
                payload["outcome"].clone(),
                payload["operator_id"].clone(),
            )
        };
        assert_eq!(
            recorded(Some((1, true))),
            ("identity.operator_created", json!("changed"), json!(1))
        );
        assert_eq!(
            recorded(Some((1, false))),
            ("identity.operator_created", json!("refused"), json!(1))
        );
        assert_eq!(
            recorded(None),
            ("identity.operator_created", json!("refused"), json!(null))
        );
    }

    /// A create's answer with a column of the wrong type is a storage failure, whichever column it is.
    #[tokio::test]
    async fn a_create_answer_of_the_wrong_type_is_a_storage_failure() {
        let (pool, _database) = support::migrated().await;
        let client = pool.get().await.expect("a connection");
        for (answer, column, rust, postgres) in [
            ("SELECT 1::integer AS id, true AS created", 0, "i64", "int4"),
            ("SELECT 1::bigint AS id, 1 AS created", 1, "bool", "int4"),
        ] {
            let rows = client.query(answer, &[]).await.expect("answers");
            assert_eq!(
                added_from(&rows[0]),
                Err(Error::Storage(format!(
                    "error deserializing column {column}: cannot convert between the Rust type `{rust}` and the \
                     Postgres type `{postgres}`"
                )))
            );
        }
    }

    /// A create whose answer cannot be read is a storage failure, in this one build, which llvm-cov scores on its
    /// own, and the operator it added is undone.
    #[tokio::test]
    async fn a_create_answer_that_cannot_be_read_is_a_storage_failure() {
        let (pool, database) = support::migrated().await;
        let admin = tetsu_postgres::connect(&database).await.expect("connects");
        admin
            .batch_execute("ALTER TABLE identity.operators ALTER COLUMN id TYPE integer")
            .await
            .expect("alters");
        let operators = Operators::new(PostgresOperators { pool });
        let failed = operators
            .create(
                &OperatorName::parse("Kurt Medley").expect("a name"),
                &support::actor(),
            )
            .await;
        assert_eq!(
            failed,
            Err(Error::Storage(
                "error deserializing column 0: cannot convert between the Rust type `i64` and the Postgres type \
                 `int4`"
                    .to_string()
            ))
        );
        let left = admin
            .query_one("SELECT count(*) FROM identity.operators", &[])
            .await
            .expect("counts");
        assert_eq!(left.try_get::<_, i64>(0).expect("a count"), 0);
    }

    /// A disabled operator is issued no link, and told apart from one who does not exist.
    #[tokio::test]
    async fn a_disabled_operator_is_issued_no_link() {
        let (pool, _database) = support::migrated().await;
        let operators = Operators::new(PostgresOperators { pool });
        let kurt = operators
            .create(
                &OperatorName::parse("Kurt Medley").expect("a name"),
                &support::actor(),
            )
            .await
            .expect("creates");
        operators
            .disable(kurt, &support::actor())
            .await
            .expect("disables");
        assert_eq!(
            operators.enroll(kurt, &support::actor()).await.map(drop),
            Err(Error::Invalid(format!(
                "no enabled operator {}",
                kurt.get()
            )))
        );
    }

    /// Every path through issuing a link, in this one build, which llvm-cov scores on its own: no random source,
    /// a link issued, an unknown operator, and a database that refuses.
    #[tokio::test]
    async fn every_path_through_issuing_a_link() {
        let (pool, database) = support::migrated().await;
        let failing = Operators::with_fill(PostgresOperators { pool: pool.clone() }, |_| {
            Err(getrandom::Error::UNSUPPORTED)
        });
        let kurt = failing
            .create(
                &OperatorName::parse("Kurt Medley").expect("a name"),
                &support::actor(),
            )
            .await
            .expect("creates");
        assert_eq!(
            failing.enroll(kurt, &support::actor()).await.map(drop),
            Err(Error::Unavailable(format!(
                "no secure random source: {}",
                getrandom::Error::UNSUPPORTED
            )))
        );
        let working = Operators::with_fill(PostgresOperators { pool }, getrandom::fill);
        assert!(working.enroll(kurt, &support::actor()).await.is_ok());
        assert_eq!(
            working
                .enroll(OperatorId::new(i64::MAX), &support::actor())
                .await
                .map(drop),
            Err(Error::Invalid(format!("no operator {}", i64::MAX)))
        );
        tetsu_postgres::connect(&database)
            .await
            .expect("connects")
            .batch_execute(
                "REVOKE EXECUTE ON FUNCTION identity.issue_enrollment(bytea, bigint) FROM tetsu_identity",
            )
            .await
            .expect("revokes");
        assert_eq!(
            working.enroll(kurt, &support::actor()).await.map(drop),
            Err(Error::Storage(
                "permission denied for function issue_enrollment".to_string()
            ))
        );
    }
}
