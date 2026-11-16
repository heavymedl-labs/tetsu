//! Use cases, reached only through [`Operators`] (ADR-0021).

use crate::browser::{Change, ForChange, Origins, SignOut};
use crate::domain::{
    Actor, Ceremony, CeremonyToken, Disabling, EnrollmentToken, Error, Fill, Issue, OperatorId,
    OperatorName, OperatorSummary, Realm, Redemption, Session, SessionToken,
};
use crate::passkeys::RelyingParty;
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

    /// Creates an operator with no passkey, for ADR-0040's enrollment command, and records it for the audit record.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn create(&self, name: &OperatorName, actor: &Actor) -> Result<OperatorId, Error> {
        self.store.insert_operator(name, actor).await
    }

    /// Issues an operator's enrollment link token, good once and for an hour, replacing any earlier link, and
    /// records whether one was issued, never the token (ADR-0040). The command prints it only to its own terminal.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`] for an operator who is disabled, holds a passkey already, or does not exist,
    /// each named as such, if no secure random source can be read, or if storage fails.
    pub async fn enroll(
        &self,
        operator: OperatorId,
        actor: &Actor,
    ) -> Result<EnrollmentToken, Error> {
        let token = EnrollmentToken::generate(self.fill)?;
        match self
            .store
            .issue_enrollment(&token.hash(), operator, actor)
            .await?
        {
            Some(Issue::Issued) => Ok(token),
            Some(Issue::Disabled) => Err(no_enabled(operator)),
            Some(Issue::Enrolled) => Err(Error::Invalid(format!(
                "operator {} has a passkey already, and adds another after signing in",
                operator.get()
            ))),
            None => Err(no_operator(operator)),
        }
    }

    /// Begins enrolling an operator's first passkey from the token their enrollment link carries. The link stays
    /// open: only finishing uses it up, so a scanner that opens the page, or a ceremony the operator abandons, costs
    /// nothing. The ceremony lasts ten minutes, never past the link, replaces any earlier one of the same link, and its
    /// state never leaves the store.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`], the same for every cause, for a link that is malformed, unknown, ended, or
    /// names an operator who is disabled or holds a passkey; if no secure random source can be read; or if storage
    /// fails.
    pub async fn begin_enrollment(
        &self,
        party: &RelyingParty,
        link: &str,
    ) -> Result<Ceremony, Error> {
        let link = EnrollmentToken::parse(link).ok_or_else(not_enrolling)?;
        let enrollee = self
            .store
            .enrollee(&link.hash())
            .await?
            .ok_or_else(not_enrolling)?;
        let (options, state) = party.begin_registration(enrollee.user_handle, &enrollee.name)?;
        let token = CeremonyToken::generate(self.fill)?;
        let begun = self
            .store
            .begin_enrollment(&token.hash(), enrollee.operator, &link.hash(), &state)
            .await?;
        if begun {
            Ok(Ceremony { token, options })
        } else {
            Err(not_enrolling())
        }
    }

    /// Finishes enrolling: verifies the browser's registration against its ceremony, then stores the passkey and
    /// uses up the link in one transaction, and records it. The ceremony is used up whatever happens. The operator
    /// is not signed in: they sign in with the new passkey, which proves it works.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`], the same for every cause, for a ceremony that is malformed, unknown, or ended,
    /// a registration that does not verify, or a link or operator that no longer allows it; or if storage fails.
    pub async fn finish_enrollment(
        &self,
        party: &RelyingParty,
        ceremony: &str,
        response: &str,
        actor: &Actor,
    ) -> Result<OperatorId, Error> {
        let held = CeremonyToken::parse(ceremony).ok_or_else(not_enrolling)?;
        let taken = self
            .store
            .take_enrollment(&held.hash())
            .await?
            .ok_or_else(not_enrolling)?;
        let passkey =
            party
                .finish_registration(response, &taken.state)
                .map_err(|error| match error {
                    Error::Invalid(_) => not_enrolling(),
                    other => other,
                })?;
        match self
            .store
            .redeem_enrollment(&taken, &passkey, actor)
            .await?
        {
            Some(Redemption::Redeemed) => Ok(taken.operator),
            _ => Err(not_enrolling()),
        }
    }

    /// Disables an operator, and then ends every session and enrollment link they hold, so a disabled operator is
    /// signed out at once and signs in again nowhere (`v5.0.0-7.4.2`). Says what it did; an operator already
    /// disabled is left as is.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`] for an operator who does not exist, or if storage fails.
    pub async fn disable(&self, operator: OperatorId, actor: &Actor) -> Result<Disabling, Error> {
        self.store
            .disable_operator(operator, actor)
            .await?
            .ok_or_else(|| no_operator(operator))
    }

    /// Enables a disabled operator again, and says whether they were disabled. Their sessions stay ended, so they
    /// sign in afresh.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`] for an operator who does not exist, or if storage fails.
    pub async fn enable(&self, operator: OperatorId, actor: &Actor) -> Result<bool, Error> {
        self.store
            .enable_operator(operator, actor)
            .await?
            .ok_or_else(|| no_operator(operator))
    }

    /// Ends every session one operator holds, as an administrator may (`v5.0.0-7.4.5`), and says how many.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`] for an operator who does not exist, or if storage fails.
    pub async fn end_sessions(&self, operator: OperatorId, actor: &Actor) -> Result<u64, Error> {
        self.store
            .end_operator_sessions(operator, Realm::Operator, actor)
            .await?
            .ok_or_else(|| no_operator(operator))
    }

    /// Ends every operator session, as an administrator may (`v5.0.0-7.4.5`), and says how many.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn end_all_sessions(&self, actor: &Actor) -> Result<u64, Error> {
        self.store.end_realm_sessions(Realm::Operator, actor).await
    }

    /// Every operator, oldest first.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn list(&self) -> Result<Vec<OperatorSummary>, Error> {
        self.store.list_operators().await
    }

    /// Starts a session for an operator who has just signed in, and returns its new token, which only the
    /// operator's browser keeps (ADR-0040, `v5.0.0-7.2.4`). It ends at the realm's absolute limit at the latest, by
    /// the store's clock. The operator's spent sessions are removed as it starts.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`] for a disabled or unknown operator, if no secure random source can be read, or
    /// if storage fails.
    pub async fn start_session(&self, operator: OperatorId) -> Result<SessionToken, Error> {
        let token = SessionToken::generate(self.fill)?;
        let realm = Realm::Operator;
        self.store
            .insert_session(&token.hash(), realm, operator)
            .await
            .map_err(|error| match error {
                Error::Invalid(_) => no_enabled(operator),
                other => other,
            })?;
        Ok(token)
    }

    /// The live operator session named by the session cookie among a request's `Cookie` headers, for a request
    /// that only reads; this request counts as its latest. A request with no such cookie, one sent twice, a token
    /// that is malformed, unknown, from another realm, or past either limit names none, and a session past a limit
    /// is removed, so it can never be used again (ADR-0048, `v5.0.0-7.3.1`, `v5.0.0-7.3.2`). A request that
    /// changes anything uses [`Self::session_for_change`] instead.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn session<'a>(
        &self,
        cookies: impl IntoIterator<Item = &'a str>,
    ) -> Result<Option<Session>, Error> {
        match Realm::Operator
            .token_in(cookies)
            .and_then(SessionToken::parse)
        {
            Some(token) => self.live(&token).await,
            None => Ok(None),
        }
    }

    /// What a state-changing request may do: act for its live operator session only when it comes from one of the
    /// realm's origins and carries that session's own form token, so no other site's form or script can act for a
    /// signed-in operator (ADR-0048, `v5.0.0-3.5.1`). A refused request leaves the session as it was.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn session_for_change(
        &self,
        origins: &Origins,
        change: &Change<'_>,
    ) -> Result<ForChange, Error> {
        let Some(token) = change.verified(Realm::Operator, origins) else {
            return Ok(ForChange::Refused);
        };
        Ok(match self.live(&token).await? {
            Some(session) => ForChange::Acting(session),
            None => ForChange::NoSession,
        })
    }

    /// Signs out: ends the session on the server, so its token signs no one in again (`v5.0.0-7.4.1`). Signing out
    /// changes state, so it passes the same checks as [`Self::session_for_change`], and a refused request ends
    /// nothing. A session that had already ended is removed too, and reported as nothing to end.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn end_session(
        &self,
        origins: &Origins,
        change: &Change<'_>,
    ) -> Result<SignOut, Error> {
        let Some(token) = change.verified(Realm::Operator, origins) else {
            return Ok(SignOut::Refused);
        };
        let ended = self
            .store
            .delete_session(&token.hash(), Realm::Operator)
            .await?;
        Ok(if ended {
            SignOut::Ended
        } else {
            SignOut::NothingToEnd
        })
    }

    async fn live(&self, token: &SessionToken) -> Result<Option<Session>, Error> {
        self.store.use_session(&token.hash(), Realm::Operator).await
    }
}

/// One answer for every way an enrollment can fail, so a browser learns nothing about which links or operators
/// exist.
fn not_enrolling() -> Error {
    Error::Invalid("this enrollment link has ended or is not valid".to_string())
}

fn no_operator(operator: OperatorId) -> Error {
    Error::Invalid(format!("no operator {}", operator.get()))
}

fn no_enabled(operator: OperatorId) -> Error {
    Error::Invalid(format!("no enabled operator {}", operator.get()))
}
