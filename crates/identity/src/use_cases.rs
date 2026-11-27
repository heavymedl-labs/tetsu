//! Use cases, reached only through [`Accounts`] (ADR-0021).

use crate::browser::{Change, CheckedSession, ForChange, Logout, Origins};
use crate::domain::{
    Acceptance, Accepting, Account, AccountId, Actor, Addition, AdminSummary, Answer, BusinessId,
    Ceremony, CeremonyKind, CeremonyToken, Device, Disabling, Ending, Error, FailedCheck, Fill,
    Invitation, InviteId, InviteToken, Issue, ListedSession, Login, Member, NewPasskey,
    NewStaffInvite, Permission, PersonName, RateLimit, Realm, Redemption, Refusal, Removal, Role,
    Session, SessionId, SessionStart, SessionToken, Signer, StaffInvite, StaffIssue, Team,
    TeamChange, TimeLeft,
};
use crate::passkeys::RelyingParty;
use crate::ports::{AdminDirectory, LoginStore, StaffDirectory, StaffStore, TeamStore};
use tetsu_kernel::{Address, Requester};

/// A realm's use cases over its store, in the realm the store names (ADR-0088). It offers nothing that reaches the
/// store, so no caller can read or write a session except through these. Every realm's store gives the login flow;
/// only the admin realm's gives managing its accounts, so only its use cases offer that.
pub struct Accounts<S> {
    store: S,
    fill: Fill,
}

impl<S> Accounts<S> {
    /// Wraps a realm's store, such as a test's own or another adapter. The store names its realm, so no caller can
    /// pair one realm's store with another realm's sessions.
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
}

impl<S: AdminDirectory> Accounts<S> {
    /// Creates an admin with no passkey, and their email address if given (ADR-0074), for ADR-0040's join
    /// command, and records it for the audit record.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn create(
        &self,
        name: &PersonName,
        email: Option<&Address>,
        actor: &Actor,
    ) -> Result<AccountId, Error> {
        self.store.insert_admin(name, email, actor).await
    }

    /// An admin's email address, for their invite and notices, or none for one who has none or does not exist.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn email(&self, admin: AccountId) -> Result<Option<Address>, Error> {
        self.store.admin_email(admin).await
    }

    /// Sets an admin's email address, where their invites and notices go from now on, and says whether it changed
    /// (ADR-0074).
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`] for an admin who does not exist, or if storage fails.
    pub async fn set_email(
        &self,
        admin: AccountId,
        email: &Address,
        actor: &Actor,
    ) -> Result<bool, Error> {
        self.store
            .set_admin_email(admin, email, actor)
            .await?
            .ok_or_else(|| no_admin(admin))
    }

    /// Issues an admin's invite link token, good once and for an hour, replacing any earlier link, and
    /// records whether one was issued, never the token (ADR-0040). The command prints it only to its own terminal.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`] for an admin who is disabled, holds a passkey already, or does not exist,
    /// each named as such, if no secure random source can be read, or if storage fails.
    pub async fn invite(&self, admin: AccountId, actor: &Actor) -> Result<InviteToken, Error> {
        let token = InviteToken::generate(self.fill)?;
        match self.store.issue_invite(&token.hash(), admin, actor).await? {
            Some(Issue::Issued) => Ok(token),
            Some(Issue::Disabled) => Err(no_enabled(admin)),
            Some(Issue::Joined) => Err(Error::Invalid(format!(
                "admin {} has a passkey already, and adds another after logging in",
                admin.get()
            ))),
            None => Err(no_admin(admin)),
        }
    }

    /// Disables an admin, and then ends every session and invite link they hold, so a disabled admin is
    /// logged out at once and logs in again nowhere (`v5.0.0-7.4.2`). Says what it did; an admin already
    /// disabled is left as is.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`] for an admin who does not exist, or if storage fails.
    pub async fn disable(&self, admin: AccountId, actor: &Actor) -> Result<Disabling, Error> {
        self.store
            .disable_admin(admin, actor)
            .await?
            .ok_or_else(|| no_admin(admin))
    }

    /// Removes every passkey an admin holds, so they can be issued a link and join again, as when their only
    /// passkey is lost or unusable (ADR-0057). Their sessions and any ceremony end too, since a session begun with a
    /// removed passkey must not outlive it (`v5.0.0-7.4.3`). Says how many passkeys it removed and sessions it
    /// ended.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`] for an admin who does not exist, or if storage fails.
    pub async fn remove_passkeys(&self, admin: AccountId, actor: &Actor) -> Result<Removal, Error> {
        self.store
            .remove_passkeys(admin, actor)
            .await?
            .ok_or_else(|| no_admin(admin))
    }

    /// Enables a disabled admin again, and says whether they were disabled. Their sessions stay ended, so they
    /// log in afresh.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`] for an admin who does not exist, or if storage fails.
    pub async fn enable(&self, admin: AccountId, actor: &Actor) -> Result<bool, Error> {
        self.store
            .enable_admin(admin, actor)
            .await?
            .ok_or_else(|| no_admin(admin))
    }

    /// Ends every session one admin holds, as an administrator may (`v5.0.0-7.4.5`), and says how many.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`] for an admin who does not exist, or if storage fails.
    pub async fn end_sessions(&self, admin: AccountId, actor: &Actor) -> Result<u64, Error> {
        self.store
            .end_admin_sessions(admin, Realm::Admin, actor)
            .await?
            .ok_or_else(|| no_admin(admin))
    }

    /// Ends every admin session, as an administrator may (`v5.0.0-7.4.5`), and says how many.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn end_all_sessions(&self, actor: &Actor) -> Result<u64, Error> {
        self.store.end_realm_sessions(Realm::Admin, actor).await
    }

    /// Every admin, oldest first.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn list(&self) -> Result<Vec<AdminSummary>, Error> {
        self.store.list_admins().await
    }
}

impl<S: StaffStore> Accounts<S> {
    /// Logs in with `accepting`'s answer and accepts the invite whose link it holds, in one step, as the join
    /// page does for someone who has a passkey: the login's
    /// fresh session accepts it (ADR-0089). The login stands whatever the invite came to, so an invite for someone
    /// else, or one that ended, leaves the person logged in and the invite as it was.
    ///
    /// # Errors
    /// Fails as [`Self::finish_login`] does, or if storage fails.
    pub async fn log_in_to_accept(
        &self,
        party: &RelyingParty,
        accepting: Accepting<'_>,
        cookies: &[&str],
        actor: &Actor,
    ) -> Result<(SessionToken, Acceptance), Error> {
        let token = self
            .finish_login(party, accepting.answer, cookies, actor)
            .await?;
        // A session that is not live the moment it began is a login that did not hold.
        let live = self.live(&token).await;
        let session = live.and_then(|session| session.ok_or_else(not_logging_in))?;
        let accepted = self.accept_invite(&session, accepting.link, actor).await?;
        Ok((token, accepted))
    }
}

impl<S: StaffDirectory> Accounts<S> {
    /// What an invite link gives, read before anyone acts on it, so its page asks someone new to make a passkey and
    /// someone with one to log in and accept; none for a link that is not one, or that ended.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn invitation(&self, link: &str) -> Result<Option<Invitation>, Error> {
        match InviteToken::parse(link) {
            Some(token) => self.store.invitation(&token.hash()).await,
            None => Ok(None),
        }
    }

    /// Accepts an invite for the staff member a live session names, adding its role at its business, if that
    /// session was proven by a passkey within ten minutes (ADR-0089). A link that is not one has ended.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn accept_invite(
        &self,
        session: &Session,
        link: &str,
        actor: &Actor,
    ) -> Result<Acceptance, Error> {
        match InviteToken::parse(link) {
            Some(token) => {
                self.store
                    .accept_invite(&token.hash(), &session.hash, session.admin, actor)
                    .await
            }
            None => Ok(Acceptance::Expired),
        }
    }

    /// Invites the staff member an email names to a business with a role, by a link good once and for seven days,
    /// making them with no passkey if no staff member has that email, and replacing any open invite of theirs there
    /// (ADR-0089). Records whether one was issued, never the token. The command prints the link only to its own
    /// terminal and emails it only to the invite's email.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`] for a staff member who is disabled or holds a role at the business already,
    /// each named as such, if no secure random source can be read, or if storage fails.
    pub async fn invite_staff(
        &self,
        invite: &NewStaffInvite,
        actor: &Actor,
    ) -> Result<StaffInvite, Error> {
        let token = InviteToken::generate(self.fill)?;
        let issue = self
            .store
            .issue_staff_invite(invite, &token.hash(), actor)
            .await?;
        let to = invite.email.as_str();
        match issue {
            StaffIssue::Issued {
                staff,
                created,
                renamed,
            } => Ok(StaffInvite {
                token,
                staff,
                created,
                renamed,
            }),
            StaffIssue::Disabled => Err(Error::Invalid(format!(
                "the staff member at {to} is disabled"
            ))),
            StaffIssue::Member => Err(Error::Invalid(format!(
                "the staff member at {to} holds a role at this business already"
            ))),
        }
    }
}

/// What inviting someone from the Team page came to, with the link to email when one was issued. It derives no
/// `Debug`, as the link is a secret.
pub struct Invited {
    pub change: TeamChange,
    pub token: Option<InviteToken>,
}

impl<S: TeamStore> Accounts<S> {
    /// The staff member a live session names, acting at `business` with the role they hold there, read on every
    /// request to the business's pages, so a role removed is refused at once (ADR-0096, `v5.0.0-7.4.2`); none for
    /// one who holds no role there.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn member_at(
        &self,
        session: &Session,
        business: BusinessId,
    ) -> Result<Option<Member>, Error> {
        let staff = session.admin();
        let role = self.store.role_at(business, staff).await?;
        Ok(role.map(|role| Member {
            staff,
            business,
            role,
        }))
    }

    /// The business a live session's staff member joined first, which their personal pages name; none for one who
    /// holds no role.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn first_business(&self, session: &Session) -> Result<Option<BusinessId>, Error> {
        self.store.first_business(session.admin()).await
    }

    /// The member's business's staff and open invites, for a member who may see them; none otherwise.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn team(&self, member: &Member) -> Result<Option<Team>, Error> {
        if !member.role.may(Permission::SeeTeam) {
            return Ok(None);
        }
        self.store.team(member.business).await.map(Some)
    }

    /// Invites someone to the member's business as `role` from its Team page, by a link good once and for seven
    /// days, for a member who may invite and reaches the role (ADR-0096). The link comes back only when one was
    /// issued, for the page to email; the answer is the same for a disabled staff member, who is issued none.
    ///
    /// # Errors
    /// Fails with [`Error::Limited`] when the member has sent their hour's invites, if no secure random source can
    /// be read, or if storage fails.
    pub async fn invite_from_team(
        &self,
        member: &Member,
        name: PersonName,
        email: Address,
        role: Role,
        actor: &Actor,
    ) -> Result<Invited, Error> {
        if !member.may_reach(Permission::InviteStaff, role) {
            return Ok(Invited {
                change: TeamChange::Refused,
                token: None,
            });
        }
        if let Some(wait) = self.store.admit_inviter(member.staff).await? {
            return Err(Error::Limited(wait));
        }
        let token = InviteToken::generate(self.fill)?;
        let invite = NewStaffInvite {
            business: member.business,
            name,
            email,
            role,
        };
        let (change, issued) = self
            .store
            .invite_from(member, &invite, &token.hash(), actor)
            .await?;
        Ok(Invited {
            change,
            token: issued.then_some(token),
        })
    }

    /// Changes `staff`'s role at the member's business from `held` to `wanted`, for a member who may change roles
    /// and reaches both, never their own.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn change_role(
        &self,
        member: &Member,
        staff: AccountId,
        held: Role,
        wanted: Role,
        actor: &Actor,
    ) -> Result<TeamChange, Error> {
        let allowed = staff != member.staff
            && member.may_reach(Permission::ChangeRole, held)
            && member.may_reach(Permission::ChangeRole, wanted);
        if !allowed {
            return Ok(TeamChange::Refused);
        }
        self.store
            .change_role(member, staff, held, wanted, actor)
            .await
    }

    /// Removes `staff`, who holds `held`, from the member's business, for a member who may remove staff and reaches
    /// the role, never themselves.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn remove_staff(
        &self,
        member: &Member,
        staff: AccountId,
        held: Role,
        actor: &Actor,
    ) -> Result<TeamChange, Error> {
        if staff == member.staff || !member.may_reach(Permission::RemoveStaff, held) {
            return Ok(TeamChange::Refused);
        }
        self.store.remove_staff(member, staff, held, actor).await
    }

    /// Cancels an open invite at the member's business, which gives `held`, for a member who may cancel invites and
    /// reaches the role.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn cancel_invite(
        &self,
        member: &Member,
        invite: &InviteId,
        held: Role,
        actor: &Actor,
    ) -> Result<TeamChange, Error> {
        if !member.may_reach(Permission::CancelInvite, held) {
            return Ok(TeamChange::Refused);
        }
        self.store.cancel_invite(member, invite, held, actor).await
    }
}

impl<S: LoginStore> Accounts<S> {
    /// Begins joining an admin's first passkey from the token their invite link carries. The link stays
    /// open: only finishing uses it up, so a scanner that opens the page, or a ceremony the admin abandons, costs
    /// nothing. The ceremony lasts ten minutes, never past the link, replaces any earlier one of the same link, and its
    /// state never leaves the store.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`], the same for every cause, for a link that is malformed, unknown, ended, or
    /// names an admin who is disabled or holds a passkey; if no secure random source can be read; or if storage
    /// fails.
    pub async fn begin_join(&self, party: &RelyingParty, link: &str) -> Result<Ceremony, Error> {
        let link = InviteToken::parse(link).ok_or_else(not_joining::<S>)?;
        let invitee = self
            .store
            .invitee(&link.hash())
            .await
            .map_err(|error| refusing(error, not_joining::<S>))?
            .ok_or_else(not_joining::<S>)?;
        let (options, state) =
            party.begin_registration(S::REALM, invitee.user_handle, &invitee.name, &[])?;
        let token = CeremonyToken::generate(self.fill)?;
        let begun = self
            .store
            .begin_join(&token.hash(), invitee.admin, &link.hash(), &state)
            .await
            .map_err(|error| refusing(error, not_joining::<S>))?;
        if begun {
            Ok(Ceremony { token, options })
        } else {
            Err(not_joining::<S>())
        }
    }

    /// Finishes joining: verifies the browser's registration against its ceremony, then stores the passkey and
    /// uses up the link in one transaction, and records it. The ceremony is used up whatever happens. The admin
    /// is not logged in: they log in with the new passkey, which proves it works.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`], the same for every cause, for a ceremony that is malformed, unknown, or ended,
    /// a registration that does not verify, or a link or admin that no longer allows it; or if storage fails.
    pub async fn finish_join(
        &self,
        party: &RelyingParty,
        answer: Answer<'_>,
        cookies: &[&str],
        actor: &Actor,
    ) -> Result<(AccountId, SessionToken), Error> {
        let held = CeremonyToken::parse(answer.ceremony).ok_or_else(not_joining::<S>)?;
        let taken = self
            .store
            .take_join(&held.hash())
            .await
            .map_err(|error| refusing(error, not_joining::<S>))?
            .ok_or_else(not_joining::<S>)?;
        let pending = Pending {
            kind: CeremonyKind::Join,
            admin: taken.admin,
            state: &taken.state,
        };
        let passkey = self
            .registered(party, answer.response, &pending, actor)
            .await
            .map_err(|error| refusing(error, not_joining::<S>))?;
        let (started, session) = self.session_start(cookies)?;
        match self
            .store
            .redeem_invite(&taken, &passkey, &session, actor)
            .await
            .map_err(|error| refusing(error, not_joining::<S>))?
        {
            Some(Redemption::Redeemed) => Ok((taken.admin, started)),
            _ => Err(not_joining::<S>()),
        }
    }

    /// Admits an attempt from `from` under a rate limit (ADR-0072), counting it unless the limit counts only
    /// failures.
    ///
    /// # Errors
    /// Fails with [`Error::Limited`] and how long to wait, when the network has no room; or if storage fails.
    pub async fn admit(&self, limit: RateLimit, from: &Requester) -> Result<(), Error> {
        match self.store.admit(limit, from).await? {
            Some(wait) => Err(Error::Limited(wait)),
            None => Ok(()),
        }
    }

    /// Counts a failed login from `from`, which the limit of failures then checks.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn failed(&self, from: &Requester) -> Result<(), Error> {
        self.store.fail(from).await
    }

    /// Begins a passkey login: a ceremony token for the browser to keep, and options that let it offer any of the
    /// person's passkeys. The ceremony lasts ten minutes, and its state never leaves the store.
    ///
    /// # Errors
    /// Never refuses for the number of logins open: past a bound, the oldest end, behind the rate limits the
    /// HTTP adapter applies first (ADR-0072). Fails with [`Error::Unavailable`] if no
    /// secure random source can be read, and with [`Error::Storage`] if storage fails.
    pub async fn begin_login(&self, party: &RelyingParty) -> Result<Ceremony, Error> {
        // Starting a discoverable login cannot fail in webauthn-rs; were it to, it would be a storage failure.
        let (token, (options, state)) = CeremonyToken::generate(self.fill)
            .and_then(|token| party.begin_authentication().map(|begun| (token, begun)))?;
        self.store.begin_login(&token.hash(), &state).await?;
        Ok(Ceremony { token, options })
    }

    /// Finishes a passkey login and starts the admin's session, returning its token, and ends the admin
    /// session the browser presented among its `cookies`, whoever's it was, in the same transaction (ADR-0059,
    /// `v5.0.0-7.2.4`). The ceremony is used up whatever happens. The passkey must belong to an enabled admin whose user handle the answer names
    /// (`WebAuthn` L3 §7.2), and must verify, user verification included; its new counter is stored only if no other
    /// login changed it meanwhile, in the transaction that starts the session, which is recorded.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`], the same for every cause, for a ceremony that is malformed, unknown, or ended,
    /// an answer that names no enabled admin's passkey or does not verify, or a login that lost a race; with
    /// [`Error::Unavailable`] if no secure random source can be read; or if storage fails.
    pub async fn finish_login(
        &self,
        party: &RelyingParty,
        answer: Answer<'_>,
        cookies: &[&str],
        actor: &Actor,
    ) -> Result<SessionToken, Error> {
        let held = CeremonyToken::parse(answer.ceremony).ok_or_else(not_logging_in)?;
        let state = self
            .store
            .take_login(&held.hash())
            .await?
            .ok_or_else(not_logging_in)?;
        let (signer, passkey) = self.verified(party, answer.response, &state, actor).await?;
        let (token, session) = self.session_start(cookies)?;
        let outcome = self.store.login(&signer, &passkey, &session, actor).await?;
        outcome
            .filter(|answer| *answer == Login::SignedIn)
            .map(|_| token)
            .ok_or_else(not_logging_in)
    }

    /// The passkey a login's answer names and verifies, with its state as it should now be stored. Each way it
    /// can be refused once its ceremony is taken is recorded with its reason, and with the admin and credential
    /// where known, before the one answer is given: an answer that is not one, a credential no enabled admin
    /// holds, a user handle that is not the credential's admin's (`WebAuthn` L3 §7.2), or what `webauthn-rs`
    /// refuses, such as a counter that did not move forward (ADR-0047).
    async fn verified(
        &self,
        party: &RelyingParty,
        response: &str,
        state: &str,
        actor: &Actor,
    ) -> Result<(Signer, String), Error> {
        let (admin, credential, reason) = match party.identify(response) {
            Err(credential) => (None, credential, "malformed".to_string()),
            Ok(answer) => match self.store.signer(&answer.credential).await? {
                Some(signer) if !signer.disabled && signer.user_handle == answer.handle => {
                    match party.finish_authentication(&answer, state, &signer.passkey) {
                        Ok(passkey) => return Ok((signer, passkey)),
                        Err(Error::Invalid(reason)) => {
                            (Some(signer.admin), Some(answer.credential), reason)
                        }
                        Err(other) => return Err(other),
                    }
                }
                found => {
                    let (admin, reason) = refused_before_verifying(found.as_ref());
                    (admin, Some(answer.credential), reason.to_string())
                }
            },
        };
        let refusal = Refusal {
            kind: CeremonyKind::Login,
            admin,
            credential: credential.as_deref(),
            reason: &reason,
        };
        self.store.refuse(&refusal, actor).await?;
        Err(not_logging_in())
    }

    /// The passkey a registration answers with, verified against its ceremony's state. A refusal is recorded with
    /// its reason and the admin whose ceremony it was, then returned for the caller to answer as one refusal.
    async fn registered(
        &self,
        party: &RelyingParty,
        response: &str,
        pending: &Pending<'_>,
        actor: &Actor,
    ) -> Result<NewPasskey, Error> {
        let reason = match party.finish_registration(response, pending.state) {
            Err(Error::Invalid(reason)) => reason,
            verified => return verified,
        };
        let refusal = Refusal {
            kind: pending.kind,
            admin: Some(pending.admin),
            credential: None,
            reason: &reason,
        };
        self.store.refuse(&refusal, actor).await?;
        Err(Error::Invalid(reason))
    }

    /// The account a live session names: the admin, their user handle and name, and every credential they hold,
    /// as the admin's page and the browser's list of their passkeys need; none if the session has ended or the
    /// admin is disabled. Reading it changes nothing, not even the session's last request.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn account(&self, session: &Session) -> Result<Option<Account>, Error> {
        self.store.account(&session.hash).await
    }

    /// The live sessions of the admin `session` belongs to, as their page lists them, that one first (ADR-0059,
    /// `v5.0.0-7.5.2`). They are found from the session alone, so no request lists another admin's, and reading
    /// them renews none.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn sessions(&self, session: &Session) -> Result<Vec<ListedSession>, Error> {
        self.store.sessions_of(&session.hash).await
    }

    /// Ends one of the admin's other sessions, by its id, or every other one when `only` names none (ADR-0059,
    /// `v5.0.0-7.5.2`), and records it. It passes the same checks as [`Self::session_for_change`], and then only a
    /// session a passkey proved within ten minutes may, by Postgres's clock (ADR-0048, `v5.0.0-7.5.1`). The session
    /// asking is never ended here; logging out ends it. The record names the admin and the `device` asking.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn end_others(
        &self,
        origins: &Origins,
        change: &Change<'_>,
        only: Option<&SessionId>,
        device: Option<Device>,
    ) -> Result<Ending, Error> {
        let checked = match self.session_for_change(origins, change).await? {
            ForChange::Acting(checked) => checked,
            ForChange::NoSession => return Ok(Ending::NoSession),
            ForChange::Refused(check) => return Ok(Ending::Refused(check)),
        };
        let session = checked.session();
        let actor = Actor::browser(S::REALM, Some(session.admin()), device);
        self.store
            .end_other_sessions(&session.hash, only, &actor)
            .await
    }

    /// Begins adding an admin's second passkey (ADR-0057), from a session that began with a passkey login
    /// within ten minutes, by Postgres's clock (ADR-0048, `v5.0.0-7.5.1`). The options list the passkeys they hold,
    /// so a device that has one makes no second, and the ceremony lasts ten minutes. With no email for admins
    /// yet, the audit record is the notice ADR-0040 asks for.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`], the same for every cause, for a session that is not fresh or an admin who
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
                .begin_registration(S::REALM, adder.user_handle, &adder.name, &adder.passkeys)
                .map(|begun| (token, begun))
        })?;
        let begun = self
            .store
            .begin_addition(&token.hash(), &session.hash, adder.admin, &state)
            .await?;
        begun
            .then_some(Ceremony { token, options })
            .ok_or_else(not_adding)
    }

    /// Finishes adding a passkey: takes the addition once, for the session that began it, verifies the registration,
    /// and stores the passkey while the session is still live and the admin holds fewer than two, using up the
    /// addition and ending the admin's other sessions, recording it.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`], the same for every cause, for a ceremony that is malformed, unknown, ended, or
    /// another admin's, a registration that does not verify, or a session or admin that no longer allows it;
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
        let pending = Pending {
            kind: CeremonyKind::Addition,
            admin: taken.admin,
            state: &taken.state,
        };
        let passkey = self
            .registered(party, answer.response, &pending, actor)
            .await
            .map_err(|error| refusing(error, not_adding))?;
        let outcome = self.store.add_passkey(&taken, &passkey, actor).await?;
        outcome
            .filter(|addition| *addition == Addition::Added)
            .map(drop)
            .ok_or_else(not_adding)
    }

    /// The live admin session named by the session cookie among a request's `Cookie` headers, for a request
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
        match Self::token(cookies) {
            Some(token) => self.live(&token).await,
            None => Ok(None),
        }
    }

    /// How long the admin session named by a request's cookies has left, read without counting as a request,
    /// so a page can warn before the idle limit without keeping the session alive: only the person's own requests
    /// renew it (NIST SP 800-63B-4). None for a request with no live session, as [`Self::session`] would find.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn time_left<'a>(
        &self,
        cookies: impl IntoIterator<Item = &'a str>,
    ) -> Result<Option<TimeLeft>, Error> {
        match Self::token(cookies) {
            Some(token) => self.store.session_left(&token.hash()).await,
            None => Ok(None),
        }
    }

    /// What a state-changing request may do: act for its live admin session only when it comes from one of the
    /// realm's origins and carries that session's own form token, so no other site's form or script can act for a
    /// logged-in admin (ADR-0048, `v5.0.0-3.5.1`). A refused request leaves the session as it was.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn session_for_change(
        &self,
        origins: &Origins,
        change: &Change<'_>,
    ) -> Result<ForChange, Error> {
        let token = match change.verified(S::REALM, origins) {
            Ok(token) => token,
            Err(check) => return Ok(ForChange::Refused(check)),
        };
        Ok(match self.live(&token).await? {
            Some(session) => ForChange::Acting(CheckedSession(session)),
            None => ForChange::NoSession,
        })
    }

    /// Records that a request to `route` from `from` failed `check` before it reached storage, while its network
    /// has room under the limit on recording refusals (ADR-0081, `v5.0.0-16.3.3`). Past that, the request is still
    /// refused, but not recorded, except that the first such refusal each window says so (ADR-0081). The record
    /// names the route, the check, and `actor`, never the request's token or address. A refused request proves no
    /// session, so its actor names no admin, even when its cookie is a live session's.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn refused(
        &self,
        from: &Requester,
        route: &'static str,
        check: FailedCheck,
        actor: &Actor,
    ) -> Result<(), Error> {
        self.store.record_refusal(from, route, check, actor).await
    }

    /// Logs out: ends the session on the server, so its token logs no one in again (`v5.0.0-7.4.1`). Logging out
    /// changes state, so it passes the same checks as [`Self::session_for_change`], and a refused request ends
    /// nothing. A session that had already ended is removed too, and reported as nothing to end. A live session's
    /// ending is recorded, naming its account at `actor`'s device (#167).
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn end_session(
        &self,
        origins: &Origins,
        change: &Change<'_>,
        actor: &Actor,
    ) -> Result<Logout, Error> {
        let token = match change.verified(S::REALM, origins) {
            Ok(token) => token,
            Err(check) => return Ok(Logout::Refused(check)),
        };
        let ended = self.store.delete_session(&token.hash(), actor).await?;
        Ok(if ended {
            Logout::Ended
        } else {
            Logout::NothingToEnd
        })
    }

    /// A new session's token, and what starting it under its hash needs: the session the browser presented, which
    /// it ends, so no token a browser held before outlives it (ADR-0059).
    fn session_start(&self, cookies: &[&str]) -> Result<(SessionToken, SessionStart), Error> {
        let token = SessionToken::generate(self.fill)?;
        let session = SessionStart {
            hash: token.hash(),
            replacing: Self::token(cookies.iter().copied()).map(|presented| presented.hash()),
        };
        Ok((token, session))
    }

    async fn live(&self, token: &SessionToken) -> Result<Option<Session>, Error> {
        self.store.use_session(&token.hash()).await
    }

    /// The realm's session token among a request's cookies, if there is exactly one and it is well formed.
    fn token<'a>(cookies: impl IntoIterator<Item = &'a str>) -> Option<SessionToken> {
        S::REALM.token_in(cookies).and_then(SessionToken::parse)
    }
}

/// One answer for every way a join can fail, so a browser learns nothing about which links or admins
/// exist.
/// One answer for every way a join can fail, naming who issues a new invite in the store's realm.
fn not_joining<S: LoginStore>() -> Error {
    joining_refused(S::REALM)
}

/// [`not_joining`] for a realm, apart from any store, so each realm's wording is tested in one build.
pub(crate) fn joining_refused(realm: Realm) -> Error {
    Error::Invalid(invite_ended(realm))
}

/// What a page says of an invite that has ended, naming who issues a new one in a realm.
pub(crate) fn invite_ended(realm: Realm) -> String {
    let issuer = match realm {
        Realm::Admin => "an admin",
        Realm::Staff => "the business's owner",
    };
    format!("this invite has ended. Log in, or ask {issuer} for a new one")
}

/// One answer for every way a login can fail, so a browser learns nothing about which passkeys or admins
/// exist (`v5.0.0-6.3.8`).
fn not_logging_in() -> Error {
    Error::Invalid("this login didn't work. Try again".to_string())
}

/// A registration whose ceremony was taken: what kind it is, whose, and the state it began with.
struct Pending<'a> {
    kind: CeremonyKind,
    admin: AccountId,
    state: &'a str,
}

/// One answer for every way adding a passkey can fail.
fn not_adding() -> Error {
    Error::Invalid("this passkey couldn't be added".to_string())
}

/// Why a login was refused before its passkey was verified: no admin holds the credential, theirs is
/// disabled, or the answer gives another user handle.
fn refused_before_verifying(found: Option<&Signer>) -> (Option<AccountId>, &'static str) {
    match found {
        None => (None, "unknown_credential"),
        Some(signer) if signer.disabled => (Some(signer.admin), "disabled"),
        Some(signer) => (Some(signer.admin), "handle_mismatch"),
    }
}

/// A refusal on the way through a ceremony reads as that ceremony's one answer, whoever reports it, so a browser
/// learns nothing about which part refused; a failure that is not a refusal stays itself.
fn refusing(error: Error, refusal: fn() -> Error) -> Error {
    match error {
        Error::Invalid(_) => refusal(),
        other => other,
    }
}

fn no_admin(admin: AccountId) -> Error {
    Error::Invalid(format!("no admin {}", admin.get()))
}

fn no_enabled(admin: AccountId) -> Error {
    Error::Invalid(format!("no enabled admin {}", admin.get()))
}
