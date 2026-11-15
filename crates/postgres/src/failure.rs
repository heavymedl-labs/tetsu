//! Sorting the driver's errors as ADR-0011 asks, for every module's adapter: only a dropped connection, a server
//! that is restarting or full, or a stale cached statement is worth retrying, and every error keeps what Postgres
//! said. Each module turns a [`Failure`] into its own error type.

use deadpool_postgres::{Pool, PoolError};
use tokio_postgres::error::SqlState;

/// Begins the message of a stale cached statement, so the module that meets one can clear every connection's cache
/// with [`clear_if_stale`].
pub const STALE: &str = "stale cached statement: ";
/// The Postgres function that raises it, which unlike the message does not change with the server's language.
const STALE_ROUTINE: &str = "RevalidateCachedQuery";

/// What a driver error means for the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// A unique constraint refused the row, named when Postgres names it, so a module can say what was taken.
    Taken {
        constraint: Option<String>,
        message: String,
    },
    /// Worth retrying.
    Temporary(String),
    /// Not worth retrying.
    Permanent(String),
}

/// Sorts an error from a query or a statement.
#[must_use]
pub fn sort(error: &tokio_postgres::Error) -> Failure {
    let message = message(error);
    if error.code() == Some(&SqlState::UNIQUE_VIOLATION) {
        let constraint = error
            .as_db_error()
            .and_then(|db| db.constraint())
            .map(ToString::to_string);
        Failure::Taken {
            constraint,
            message,
        }
    } else if is_stale(error) {
        Failure::Temporary(format!("{STALE}{message}"))
    } else if error.is_closed() || is_io(error) || is_server_unavailable(error) {
        Failure::Temporary(message)
    } else {
        Failure::Permanent(message)
    }
}

/// Sorts an error from a commit. One Postgres answered was refused and rolled back, so it is sorted like any other,
/// but never as temporary: the work is gone, and only the caller knows whether to do it again. One with no answer
/// may still have been saved, so a retry is not safe either (ADR-0011).
#[must_use]
pub fn sort_commit(error: &tokio_postgres::Error) -> Failure {
    if error.as_db_error().is_none() {
        return Failure::Permanent(format!("commit outcome unknown: {}", message(error)));
    }
    match sort(error) {
        Failure::Taken {
            constraint,
            message,
        } => Failure::Taken {
            constraint,
            message: format!("commit refused: {message}"),
        },
        Failure::Temporary(why) | Failure::Permanent(why) => {
            Failure::Permanent(format!("commit refused: {why}"))
        }
    }
}

/// Sorts an error from taking a connection: a pool timeout or a lost connection is worth retrying, and a refused
/// login or bad settings is not.
#[must_use]
pub fn sort_pool(error: PoolError) -> Failure {
    match error {
        PoolError::Timeout(_) => Failure::Temporary(error.to_string()),
        PoolError::Backend(error) => sort(&error),
        other => Failure::Permanent(other.to_string()),
    }
}

/// A stale statement stays cached until the caches are cleared, and every pooled connection may hold it, so a
/// failure whose message [`sort`] marked stale clears them all before the next request.
pub fn clear_if_stale(pool: &Pool, message: &str) {
    if message.starts_with(STALE) {
        pool.manager().statement_caches.clear();
    }
}

/// The driver's own text is just "db error" or "error connecting to server", so this keeps what Postgres said, or
/// the cause underneath, such as a refused connection.
fn message(error: &tokio_postgres::Error) -> String {
    if let Some(db) = error.as_db_error() {
        return db.message().to_string();
    }
    let causes: Vec<String> =
        std::iter::successors(Some(error as &dyn std::error::Error), |e| e.source())
            .map(ToString::to_string)
            .collect();
    causes.join(": ")
}

/// The server ending the session for a restart, crash, or failover, still starting up, or out of connections. A
/// query running at that moment gets one of these, not a closed connection.
fn is_server_unavailable(error: &tokio_postgres::Error) -> bool {
    error.code().is_some_and(|code| {
        [
            SqlState::ADMIN_SHUTDOWN,
            SqlState::CRASH_SHUTDOWN,
            SqlState::CANNOT_CONNECT_NOW,
            SqlState::TOO_MANY_CONNECTIONS,
        ]
        .contains(code)
    })
}

fn is_stale(error: &tokio_postgres::Error) -> bool {
    error.as_db_error().is_some_and(|db| {
        db.code() == &SqlState::FEATURE_NOT_SUPPORTED && db.routine() == Some(STALE_ROUTINE)
    })
}

fn is_io(error: &tokio_postgres::Error) -> bool {
    std::error::Error::source(error).is_some_and(<dyn std::error::Error>::is::<std::io::Error>)
}
