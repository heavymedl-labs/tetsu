//! The driven port Identity calls out through.

use crate::domain::{
    Actor, Disabling, Error, Issue, OperatorId, OperatorName, OperatorSummary, Realm, Session,
};
use std::future::Future;

/// Where operators and their sessions are kept. A session is reached only by its token's hash and realm, one at a
/// time (ADR-0048), and the store keeps each realm's limits beside its own clock, so no caller can stretch a
/// session. The Postgres adapter implements it, and tests may use their own.
pub trait OperatorStore {
    /// Stores a new operator, with no passkey yet. Each change below records an audit event naming `actor`, with
    /// its outcome, in the same transaction.
    fn insert_operator(
        &self,
        name: &OperatorName,
        actor: &Actor,
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

    /// Stores an operator's enrollment link under its token's hash, good for an hour, replacing any earlier one,
    /// and says what came of it: an operator who is disabled or holds a passkey is issued none, and one who does
    /// not exist is none.
    fn issue_enrollment(
        &self,
        hash: &[u8; 32],
        operator: OperatorId,
        actor: &Actor,
    ) -> impl Future<Output = Result<Option<Issue>, Error>> + Send;

    /// Disables an operator, ending every session and enrollment link they hold, and says what it did, or none for
    /// an operator who does not exist.
    fn disable_operator(
        &self,
        operator: OperatorId,
        actor: &Actor,
    ) -> impl Future<Output = Result<Option<Disabling>, Error>> + Send;

    /// Enables a disabled operator again, and says whether they were disabled, or none for an operator who does not
    /// exist. Their sessions stay ended.
    fn enable_operator(
        &self,
        operator: OperatorId,
        actor: &Actor,
    ) -> impl Future<Output = Result<Option<bool>, Error>> + Send;

    /// Ends every session one operator holds in a realm, and says how many, or none for an operator who does not
    /// exist.
    fn end_operator_sessions(
        &self,
        operator: OperatorId,
        realm: Realm,
        actor: &Actor,
    ) -> impl Future<Output = Result<Option<u64>, Error>> + Send;

    /// Ends every session in a realm, and says how many.
    fn end_realm_sessions(
        &self,
        realm: Realm,
        actor: &Actor,
    ) -> impl Future<Output = Result<u64, Error>> + Send;

    /// Every operator, oldest first.
    fn list_operators(&self) -> impl Future<Output = Result<Vec<OperatorSummary>, Error>> + Send;
}
