//! Identity's Postgres adapter. Driver types stay in this file (ADR-0007).

use std::sync::atomic::Ordering::Relaxed;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64};

use tetsu_kernel::Migration;
use tetsu_postgres::{Failure, ModuleTransaction, NewEvent, Pool};
use tokio_postgres::Row;
use tokio_postgres::error::SqlState;
use tokio_postgres::types::{FromSql, ToSql};

use crate::domain::{Error, OperatorId, OperatorName, OperatorSummary, Realm, Session};
use crate::ports::OperatorStore;
use crate::use_cases::Operators;

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: include_str!("../../migrations/0001_operators.sql"),
    },
    Migration {
        version: 2,
        sql: include_str!("../../migrations/0002_operator_controls.sql"),
    },
    // Identity records what the operator command does, for the audit record (ADR-0043, ADR-0045).
    Migration {
        version: 3,
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

/// Identity's lookup functions raise `no_data_found` for an operator who is disabled or unknown, which is the
/// caller's mistake, not storage failing.
fn sorted(error: &tokio_postgres::Error) -> Error {
    match error.as_db_error() {
        Some(db) if db.code() == &SqlState::NO_DATA_FOUND => {
            Error::Invalid(db.message().to_string())
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

fn no_id() -> Error {
    Error::Storage("no operator id returned".to_string())
}

impl OperatorStore for PostgresOperators {
    async fn insert_operator(&self, name: &OperatorName) -> Result<OperatorId, Error> {
        let id = AtomicI64::new(0);
        self.query_recording(INSERT_OPERATOR, &[&name.as_str()], &|rows| {
            id.store(column(rows.first().ok_or_else(no_id)?, "id")?, Relaxed);
            let event = format!(r#"{{"operator_id": {}}}"#, id.load(Relaxed));
            Ok(Some(("identity.operator_created", event)))
        })
        .await?;
        Ok(OperatorId::new(id.load(Relaxed)))
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

    async fn issue_enrollment(&self, hash: &[u8; 32], operator: OperatorId) -> Result<bool, Error> {
        let id = operator.get();
        let parameters: [&(dyn ToSql + Sync); 2] = [&hash.as_slice(), &id];
        let issued = AtomicBool::new(false);
        self.query_recording(ISSUE_ENROLLMENT, &parameters, &|rows| {
            issued.store(flag(rows, "issued")?, Relaxed);
            let event = format!(r#"{{"operator_id": {id}}}"#);
            Ok(issued
                .load(Relaxed)
                .then_some(("identity.enrollment_issued", event)))
        })
        .await?;
        Ok(issued.load(Relaxed))
    }

    async fn disable_operator(&self, operator: OperatorId) -> Result<bool, Error> {
        let id = operator.get();
        let disabled = AtomicBool::new(false);
        self.query_recording(DISABLE_OPERATOR, &[&id], &|rows| {
            disabled.store(flag(rows, "disabled")?, Relaxed);
            let event = format!(r#"{{"operator_id": {id}}}"#);
            Ok(disabled
                .load(Relaxed)
                .then_some(("identity.operator_disabled", event)))
        })
        .await?;
        Ok(disabled.load(Relaxed))
    }

    async fn end_operator_sessions(
        &self,
        operator: OperatorId,
        realm: Realm,
    ) -> Result<u64, Error> {
        let (id, name) = (operator.get(), realm.as_str());
        let parameters: [&(dyn ToSql + Sync); 2] = [&id, &name];
        let ended = AtomicU64::new(0);
        self.query_recording(END_OPERATOR_SESSIONS, &parameters, &|rows| {
            ended.store(count(rows)?, Relaxed);
            let event = format!(
                r#"{{"operator_id": {id}, "realm": "{name}", "ended": {}}}"#,
                ended.load(Relaxed)
            );
            Ok(Some(("identity.sessions_ended", event)))
        })
        .await?;
        Ok(ended.load(Relaxed))
    }

    async fn end_realm_sessions(&self, realm: Realm) -> Result<u64, Error> {
        let name = realm.as_str();
        let ended = AtomicU64::new(0);
        self.query_recording(END_REALM_SESSIONS, &[&name], &|rows| {
            ended.store(count(rows)?, Relaxed);
            let event = format!(r#"{{"realm": "{name}", "ended": {}}}"#, ended.load(Relaxed));
            Ok(Some(("identity.sessions_ended", event)))
        })
        .await?;
        Ok(ended.load(Relaxed))
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

/// The boolean a lookup function returns, which says it did nothing when there is no row, as for `delete_session`.
fn flag(rows: &[Row], name: &str) -> Result<bool, Error> {
    rows.first().map_or(Ok(false), |row| column(row, name))
}

/// The count a lookup function returns. Postgres's count is never negative, so the conversion cannot fail.
fn count(rows: &[Row]) -> Result<u64, Error> {
    let ended: i64 = rows.first().map_or(Ok(0), |row| column(row, "ended"))?;
    Ok(u64::try_from(ended).unwrap_or_default())
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
            .create(&OperatorName::parse("Kurt Medley").expect("a name"))
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
            Err(Error::Invalid("no enabled operator".to_string())),
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

    /// Every path through issuing a link, in this one build, which llvm-cov scores on its own: no random source,
    /// a link issued, an unknown operator, and a database that refuses.
    #[tokio::test]
    async fn every_path_through_issuing_a_link() {
        let (pool, database) = support::migrated().await;
        let failing = Operators::with_fill(PostgresOperators { pool: pool.clone() }, |_| {
            Err(getrandom::Error::UNSUPPORTED)
        });
        let kurt = failing
            .create(&OperatorName::parse("Kurt Medley").expect("a name"))
            .await
            .expect("creates");
        assert_eq!(
            failing.enroll(kurt).await.map(drop),
            Err(Error::Unavailable(format!(
                "no secure random source: {}",
                getrandom::Error::UNSUPPORTED
            )))
        );
        let working = Operators::with_fill(PostgresOperators { pool }, getrandom::fill);
        assert!(working.enroll(kurt).await.is_ok());
        assert_eq!(
            working.enroll(OperatorId::new(i64::MAX)).await.map(drop),
            Err(Error::Invalid(format!("no enabled operator {}", i64::MAX)))
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
            working.enroll(kurt).await.map(drop),
            Err(Error::Storage(
                "permission denied for function issue_enrollment".to_string()
            ))
        );
    }
}
