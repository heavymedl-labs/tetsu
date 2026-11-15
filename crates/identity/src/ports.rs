//! The driven port Identity calls out through.

use std::future::Future;
use std::time::SystemTime;

use crate::domain::{Error, OperatorId, OperatorName, Realm, Session};

/// Where operators and their sessions are kept. A session is reached only by its token's hash, one at a time
/// (ADR-0048). The Postgres adapter implements it, and tests may use their own.
pub trait OperatorStore {
    /// Stores a new operator, with no passkey yet.
    fn insert_operator(
        &self,
        name: &OperatorName,
    ) -> impl Future<Output = Result<OperatorId, Error>> + Send;

    /// Stores a session under its token's hash.
    fn insert_session(
        &self,
        hash: &[u8; 32],
        session: &Session,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// The session stored under a token's hash in a realm, if any; one in another realm is none.
    fn find_session(
        &self,
        hash: &[u8; 32],
        realm: Realm,
    ) -> impl Future<Output = Result<Option<Session>, Error>> + Send;

    /// Records a request at `seen`, never moving the last request back.
    fn touch_session(
        &self,
        hash: &[u8; 32],
        seen: SystemTime,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Removes a session, if it is still stored.
    fn delete_session(&self, hash: &[u8; 32]) -> impl Future<Output = Result<(), Error>> + Send;
}
