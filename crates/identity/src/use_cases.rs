//! Use cases, reached only through [`Operators`] (ADR-0021).

use std::time::SystemTime;

use crate::domain::{Error, Fill, OperatorId, OperatorName, Realm, Session, SessionToken};
use crate::ports::OperatorStore;

/// The operator realm's use cases over a store. It offers nothing that reaches the store, so no caller can read or
/// write a session except through these.
pub struct Operators<S> {
    store: S,
    fill: Fill,
}

impl<S: OperatorStore> Operators<S> {
    /// Wraps a store, such as a test's own or another adapter.
    pub fn new(store: S) -> Self {
        Self {
            store,
            fill: getrandom::fill,
        }
    }

    /// With another random source, so a test can make it fail.
    #[cfg(test)]
    pub(crate) fn with_fill(store: S, fill: Fill) -> Self {
        Self { store, fill }
    }

    /// Creates an operator with no passkey, for ADR-0040's enrollment command.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn create(&self, name: &OperatorName) -> Result<OperatorId, Error> {
        self.store.insert_operator(name).await
    }

    /// Starts a session for an operator who has just signed in, and returns its new token, which only the
    /// operator's browser keeps (ADR-0040, `v5.0.0-7.2.4`).
    ///
    /// # Errors
    /// Fails if no secure random source can be read, or storage fails, as it does for an unknown operator.
    pub async fn start_session(
        &self,
        operator: OperatorId,
        now: SystemTime,
    ) -> Result<SessionToken, Error> {
        let token = SessionToken::generate(self.fill)?;
        let realm = Realm::Operator;
        let session = Session {
            realm,
            operator,
            started_at: now,
            last_seen_at: now,
            ends_at: now + realm.limits().absolute,
        };
        self.store.insert_session(&token.hash(), &session).await?;
        Ok(token)
    }

    /// The live operator session a cookie's token names, as of a request at `now`, which counts as its latest. A
    /// token that is malformed, unknown, or past either limit names none, and a session past a limit is removed,
    /// so it can never be used again (ADR-0048, `v5.0.0-7.3.1`, `v5.0.0-7.3.2`).
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn session(&self, token: &str, now: SystemTime) -> Result<Option<Session>, Error> {
        let Some(token) = SessionToken::parse(token) else {
            return Ok(None);
        };
        let hash = token.hash();
        let Some(mut session) = self.store.find_session(&hash, Realm::Operator).await? else {
            return Ok(None);
        };
        if !session.is_live(now) {
            self.store.delete_session(&hash).await?;
            return Ok(None);
        }
        self.store.touch_session(&hash, now).await?;
        session.last_seen_at = session.last_seen_at.max(now);
        Ok(Some(session))
    }

    /// Ends the session a cookie's token names, on the server, so the token signs no one in again
    /// (`v5.0.0-7.4.1`). A token that names no session ends nothing.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn end_session(&self, token: &str) -> Result<(), Error> {
        match SessionToken::parse(token) {
            Some(token) => self.store.delete_session(&token.hash()).await,
            None => Ok(()),
        }
    }
}
