//! The driven port Identity calls out through.

use crate::domain::{Error, OperatorId, OperatorName, Realm, Session};
use std::future::Future;

/// Where operators and their sessions are kept. A session is reached only by its token's hash and realm, one at a
/// time (ADR-0048), and the store keeps each realm's limits beside its own clock, so no caller can stretch a
/// session. The Postgres adapter implements it, and tests may use their own.
pub trait OperatorStore {
    /// Stores a new operator, with no passkey yet.
    fn insert_operator(
        &self,
        name: &OperatorName,
    ) -> impl Future<Output = Result<OperatorId, Error>> + Send;

    /// Stores a session under its token's hash, starting now, and removes the operator's spent sessions in the
    /// realm.
    fn insert_session(
        &self,
        hash: &[u8; 32],
        realm: Realm,
        operator: OperatorId,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Records a request on the live session stored under a token's hash in a realm, and returns it. A session
    /// past either of the realm's limits is removed instead, and none is returned; one in another realm is none.
    /// Checking and recording are one step, so requests at the same moment cannot end a live session or revive a
    /// spent one.
    fn use_session(
        &self,
        hash: &[u8; 32],
        realm: Realm,
    ) -> impl Future<Output = Result<Option<Session>, Error>> + Send;

    /// Removes a session in a realm, and says whether it was live when it ended.
    fn delete_session(
        &self,
        hash: &[u8; 32],
        realm: Realm,
    ) -> impl Future<Output = Result<bool, Error>> + Send;
}
