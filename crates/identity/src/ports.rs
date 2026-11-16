//! The driven port Identity calls out through.

use crate::domain::{
    Account, Actor, Addition, CeremonyKind, Disabling, Enrollee, Error, Issue, NewPasskey,
    OperatorId, OperatorName, OperatorSummary, Realm, Redemption, Removal, Session, SessionStart,
    SignIn, Signer, TakenAddition, TakenCeremony,
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

    /// The operator an enrollment link's hash names, while the link is open, the operator enabled, and no passkey
    /// theirs yet. Reading it uses nothing up.
    fn enrollee(
        &self,
        link: &[u8; 32],
    ) -> impl Future<Output = Result<Option<Enrollee>, Error>> + Send;

    /// Keeps an enrollment ceremony's state under its token's hash, with the link it will redeem, for ten minutes
    /// but never past the link, and replacing any earlier ceremony of that link. Says whether it did: not once the
    /// link has ended, or its operator is disabled or holds a passkey.
    fn begin_enrollment(
        &self,
        token: &[u8; 32],
        operator: OperatorId,
        link: &[u8; 32],
        state: &str,
    ) -> impl Future<Output = Result<bool, Error>> + Send;

    /// Takes an enrollment ceremony by its token's hash, once: it is gone after this, and returned only if it was
    /// still open.
    fn take_enrollment(
        &self,
        token: &[u8; 32],
    ) -> impl Future<Output = Result<Option<TakenCeremony>, Error>> + Send;

    /// Stores an operator's first passkey and uses up the ceremony's link, in one transaction, and says what came of
    /// it, or none for an operator who does not exist.
    fn redeem_enrollment(
        &self,
        ceremony: &TakenCeremony,
        passkey: &NewPasskey,
        actor: &Actor,
    ) -> impl Future<Output = Result<Option<Redemption>, Error>> + Send;

    /// Disables an operator, ending every session and enrollment link they hold, and says what it did, or none for
    /// an operator who does not exist.
    fn disable_operator(
        &self,
        operator: OperatorId,
        actor: &Actor,
    ) -> impl Future<Output = Result<Option<Disabling>, Error>> + Send;

    /// Keeps a sign-in ceremony's state for ten minutes under its token's hash. It never refuses for the number
    /// open: past a bound, the oldest end.
    fn begin_sign_in(
        &self,
        token: &[u8; 32],
        state: &str,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Records a ceremony refused after it was taken but before storage was asked to store its outcome, with its
    /// reason and, where known, the operator and credential, so every refusal after a ceremony was taken is in the
    /// audit record (`v5.0.0-16.3.1`).
    fn refuse(
        &self,
        kind: CeremonyKind,
        operator: Option<OperatorId>,
        credential: Option<&[u8]>,
        reason: &str,
        actor: &Actor,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Takes a sign-in ceremony's state by its token's hash, once, if it was still open.
    fn take_sign_in(
        &self,
        token: &[u8; 32],
    ) -> impl Future<Output = Result<Option<String>, Error>> + Send;

    /// The passkey a credential ID names, with its operator and whether they are disabled, or none for a credential
    /// no operator holds.
    fn signer(
        &self,
        credential: &[u8],
    ) -> impl Future<Output = Result<Option<Signer>, Error>> + Send;

    /// Stores a passkey's verified new state, if no other sign-in changed it meanwhile and it is still the signer's
    /// operator's, and starts the operator's session under a token's hash, in one transaction, recording it; says
    /// what came of it, or none for a passkey that no longer exists or is no longer theirs.
    fn sign_in(
        &self,
        signer: &Signer,
        passkey: &str,
        session: &SessionStart,
        actor: &Actor,
    ) -> impl Future<Output = Result<Option<SignIn>, Error>> + Send;

    /// The account a live session's hash names, if the operator is enabled. Reading it changes nothing.
    fn account(
        &self,
        session: &[u8; 32],
    ) -> impl Future<Output = Result<Option<Account>, Error>> + Send;

    /// The account a live session's hash names, as [`Self::account`] finds it, if the session began within ten
    /// minutes and the operator holds fewer than two passkeys.
    fn adder(
        &self,
        session: &[u8; 32],
    ) -> impl Future<Output = Result<Option<Account>, Error>> + Send;

    /// Keeps a ceremony for adding a passkey for ten minutes, if the session still allows it, replacing the
    /// operator's earlier one; says whether it did.
    fn begin_addition(
        &self,
        token: &[u8; 32],
        session: &[u8; 32],
        operator: OperatorId,
        state: &str,
    ) -> impl Future<Output = Result<bool, Error>> + Send;

    /// Takes an addition by its token's hash for the session that began it, once, if it was still open. Only storing
    /// its passkey then uses it up.
    fn take_addition(
        &self,
        token: &[u8; 32],
        session: &[u8; 32],
    ) -> impl Future<Output = Result<Option<TakenAddition>, Error>> + Send;

    /// Stores an operator's added passkey while their session is live, using up the addition it took and ending
    /// their other sessions, in one transaction, recording it; says what came of it, or none for an operator who
    /// does not exist.
    fn add_passkey(
        &self,
        addition: &TakenAddition,
        passkey: &NewPasskey,
        actor: &Actor,
    ) -> impl Future<Output = Result<Option<Addition>, Error>> + Send;

    /// Removes every passkey an operator holds, ending their sessions and any ceremony, and says what it did, or none
    /// for an operator who does not exist.
    fn remove_passkeys(
        &self,
        operator: OperatorId,
        actor: &Actor,
    ) -> impl Future<Output = Result<Option<Removal>, Error>> + Send;

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
