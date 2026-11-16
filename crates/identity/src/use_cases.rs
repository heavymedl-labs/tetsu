//! Use cases, reached only through [`Operators`] (ADR-0021).

use crate::browser::{Change, CheckedSession, ForChange, Origins, SignOut};
use crate::domain::{
    Actor, Addition, Answer, Ceremony, CeremonyToken, Disabling, EnrollmentToken, Error, Fill,
    Issue, OperatorId, OperatorName, OperatorSummary, Realm, Redemption, Removal, Session,
    SessionToken, SignIn, Signer,
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
            .await
            .map_err(|error| refusing(error, not_enrolling))?
            .ok_or_else(not_enrolling)?;
        let (options, state) =
            party.begin_registration(enrollee.user_handle, &enrollee.name, &[])?;
        let token = CeremonyToken::generate(self.fill)?;
        let begun = self
            .store
            .begin_enrollment(&token.hash(), enrollee.operator, &link.hash(), &state)
            .await
            .map_err(|error| refusing(error, not_enrolling))?;
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
            .await
            .map_err(|error| refusing(error, not_enrolling))?
            .ok_or_else(not_enrolling)?;
        let passkey = party
            .finish_registration(response, &taken.state)
            .map_err(|error| refusing(error, not_enrolling))?;
        match self
            .store
            .redeem_enrollment(&taken, &passkey, actor)
            .await
            .map_err(|error| refusing(error, not_enrolling))?
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

    /// Begins a passkey sign-in: a ceremony token for the browser to keep, and options that let it offer any of the
    /// person's passkeys. The ceremony lasts ten minutes, and its state never leaves the store.
    ///
    /// # Errors
    /// Never refuses for the number of sign-ins open: past a bound, the oldest end, so a flood of begins can end a
    /// real one before its person answers until step 5's rate limits. Fails with [`Error::Unavailable`] if no
    /// secure random source can be read, or if storage fails.
    pub async fn begin_sign_in(&self, party: &RelyingParty) -> Result<Ceremony, Error> {
        // Starting a discoverable sign-in cannot fail in webauthn-rs, so its failure joins the random source's.
        let (token, (options, state)) = CeremonyToken::generate(self.fill)
            .and_then(|token| party.begin_authentication().map(|begun| (token, begun)))?;
        self.store.begin_sign_in(&token.hash(), &state).await?;
        Ok(Ceremony { token, options })
    }

    /// Finishes a passkey sign-in and starts the operator's session, returning its token. The ceremony is used up
    /// whatever happens. The passkey must belong to an enabled operator whose user handle the answer names
    /// (`WebAuthn` L3 §7.2), and must verify, user verification included; its new counter is stored only if no other
    /// sign-in changed it meanwhile, in the transaction that starts the session, which is recorded.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`], the same for every cause, for a ceremony that is malformed, unknown, or ended,
    /// an answer that names no enabled operator's passkey or does not verify, or a sign-in that lost a race; with
    /// [`Error::Unavailable`] if no secure random source can be read; or if storage fails.
    pub async fn finish_sign_in(
        &self,
        party: &RelyingParty,
        ceremony: &str,
        response: &str,
        actor: &Actor,
    ) -> Result<SessionToken, Error> {
        let held = CeremonyToken::parse(ceremony).ok_or_else(not_signing_in)?;
        let state = self
            .store
            .take_sign_in(&held.hash())
            .await?
            .ok_or_else(not_signing_in)?;
        let (signer, passkey) = self.verified(party, response, &state, actor).await?;
        let token = SessionToken::generate(self.fill)?;
        let outcome = self
            .store
            .sign_in(&signer, &passkey, &token.hash(), actor)
            .await?;
        outcome
            .filter(|answer| *answer == SignIn::SignedIn)
            .map(|_| token)
            .ok_or_else(not_signing_in)
    }

    /// The passkey a sign-in's answer names and verifies, with its state as it should now be stored. Each way it
    /// can be refused once its ceremony is taken is recorded with its reason, and with the operator and credential
    /// where known, before the one answer is given: an answer that is not one, a credential no enabled operator
    /// holds, a user handle that is not the credential's operator's (`WebAuthn` L3 §7.2), or what `webauthn-rs`
    /// refuses, such as a counter that did not move forward (ADR-0047).
    async fn verified(
        &self,
        party: &RelyingParty,
        response: &str,
        state: &str,
        actor: &Actor,
    ) -> Result<(Signer, String), Error> {
        let (operator, credential, reason) = match party.identify(response) {
            None => (None, None, "malformed".to_string()),
            Some(answer) => match self.store.signer(&answer.credential).await? {
                None => (
                    None,
                    Some(answer.credential),
                    "unknown_credential".to_string(),
                ),
                Some(signer) if signer.user_handle != answer.handle => (
                    Some(signer.operator),
                    Some(answer.credential),
                    "handle_mismatch".to_string(),
                ),
                Some(signer) => {
                    match party.finish_authentication(&answer, state, &signer.passkey) {
                        Ok(passkey) => return Ok((signer, passkey)),
                        Err(Error::Invalid(reason)) => {
                            (Some(signer.operator), Some(answer.credential), reason)
                        }
                        Err(other) => return Err(other),
                    }
                }
            },
        };
        self.store
            .refuse_sign_in(operator, credential.as_deref(), &reason, actor)
            .await?;
        Err(not_signing_in())
    }

    /// Begins adding an operator's second passkey (ADR-0057), from a session that began with a passkey sign-in
    /// within ten minutes, by Postgres's clock (ADR-0048, `v5.0.0-7.5.1`). The options list the passkeys they hold,
    /// so a device that has one makes no second, and the ceremony lasts ten minutes. With no email for operators
    /// yet, the audit record is the notice ADR-0040 asks for.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`], the same for every cause, for a session that is not fresh or an operator who
    /// is disabled or holds two passkeys; with [`Error::Unavailable`] if no secure random source can be read; or if
    /// storage fails.
    pub async fn begin_adding_passkey(
        &self,
        party: &RelyingParty,
        checked: &CheckedSession,
    ) -> Result<Ceremony, Error> {
        let session = checked.session();
        let adder = self
            .store
            .adder(&session.hash)
            .await?
            .ok_or_else(not_adding)?;
        let (token, (options, state)) = CeremonyToken::generate(self.fill).and_then(|token| {
            party
                .begin_registration(adder.user_handle, &adder.name, &adder.credentials)
                .map(|begun| (token, begun))
        })?;
        let begun = self
            .store
            .begin_addition(&token.hash(), &session.hash, adder.operator, &state)
            .await?;
        begun
            .then_some(Ceremony { token, options })
            .ok_or_else(not_adding)
    }

    /// Finishes adding a passkey: takes the addition once, for the session that began it, verifies the registration,
    /// and stores the passkey while the session is still live and the operator holds fewer than two, using up the
    /// addition and ending the operator's other sessions, recording it.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`], the same for every cause, for a ceremony that is malformed, unknown, ended, or
    /// another operator's, a registration that does not verify, or a session or operator that no longer allows it;
    /// or if storage fails.
    pub async fn finish_adding_passkey(
        &self,
        party: &RelyingParty,
        checked: &CheckedSession,
        answer: Answer<'_>,
        actor: &Actor,
    ) -> Result<(), Error> {
        let held = CeremonyToken::parse(answer.ceremony).ok_or_else(not_adding)?;
        let taken = self
            .store
            .take_addition(&held.hash(), &checked.session().hash)
            .await?
            .ok_or_else(not_adding)?;
        let passkey = party
            .finish_registration(answer.response, &taken.state)
            .map_err(|error| refusing(error, not_adding))?;
        let outcome = self.store.add_passkey(&taken, &passkey, actor).await?;
        outcome
            .filter(|addition| *addition == Addition::Added)
            .map(drop)
            .ok_or_else(not_adding)
    }

    /// Removes every passkey an operator holds, so they can be issued a link and enroll again, as when their only
    /// passkey is lost or unusable (ADR-0057). Their sessions and any ceremony end too, since a session begun with a
    /// removed passkey must not outlive it (`v5.0.0-7.4.3`). Says how many passkeys it removed and sessions it
    /// ended.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`] for an operator who does not exist, or if storage fails.
    pub async fn remove_passkeys(
        &self,
        operator: OperatorId,
        actor: &Actor,
    ) -> Result<Removal, Error> {
        self.store
            .remove_passkeys(operator, actor)
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
            Some(session) => ForChange::Acting(CheckedSession(session)),
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

/// One answer for every way a sign-in can fail, so a browser learns nothing about which passkeys or operators
/// exist (`v5.0.0-6.3.8`).
fn not_signing_in() -> Error {
    Error::Invalid("this sign-in could not be completed".to_string())
}

/// One answer for every way adding a passkey can fail.
fn not_adding() -> Error {
    Error::Invalid("this passkey could not be added".to_string())
}

/// A refusal on the way through a ceremony reads as that ceremony's one answer, whoever reports it, so a browser
/// learns nothing about which part refused; a failure that is not a refusal stays itself.
fn refusing(error: Error, refusal: fn() -> Error) -> Error {
    match error {
        Error::Invalid(_) => refusal(),
        other => other,
    }
}

fn no_operator(operator: OperatorId) -> Error {
    Error::Invalid(format!("no operator {}", operator.get()))
}

fn no_enabled(operator: OperatorId) -> Error {
    Error::Invalid(format!("no enabled operator {}", operator.get()))
}
