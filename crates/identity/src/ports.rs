//! The driven port Identity calls out through.

use crate::domain::{
    Acceptance, Account, AccountId, Actor, Addition, AdminSummary, BusinessId, Disabling, Ending,
    Error, FailedCheck, Invitation, InviteId, Invitee, Issue, ListedSession, Login, Member,
    NewPasskey, NewStaffInvite, PersonName, RateLimit, Realm, Redemption, Refusal, Removal, Role,
    RoleChange, Session, SessionId, SessionStart, Signer, StaffIssue, TakenAddition, TakenCeremony,
    Team, TeamChange, TeamInvite, TimeLeft,
};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tetsu_kernel::{Address, Delivery, MailError, Mailer, Message, Requester};

/// A business as Identity's pages name and link to it: its id, the slug of its address, and its name, all Tenancy's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Business {
    pub id: BusinessId,
    pub slug: String,
    pub name: String,
}

/// A business as [`BusinessNames`] finds it, boxed so a page can hold any as `dyn BusinessNames`.
pub type Finding<'a> = Pin<Box<dyn Future<Output = Result<Option<Business>, Error>> + Send + 'a>>;

/// Where Identity finds a business, which Tenancy keeps: by id, for a page that knows it only by id, such as a staff
/// invite's, and by slug, for a business's own pages (ADR-0096). The composition root fills it; none for an id or a
/// slug no business has.
pub trait BusinessNames: Send + Sync {
    fn found(&self, business: BusinessId) -> Finding<'_>;
    fn at<'a>(&'a self, slug: &'a str) -> Finding<'a>;
}

/// What sending an email came to, boxed so a page can hold any mailer as `dyn Post`.
pub type Posting<'a> = Pin<Box<dyn Future<Output = Result<Delivery, MailError>> + Send + 'a>>;

/// Sends the email a page asks for, such as an invite from a Team page, as [`Mailer`] does (ADR-0044).
pub trait Post: Send + Sync {
    fn post<'a>(&'a self, message: &'a Message) -> Posting<'a>;
}

impl<M: Mailer> Post for M {
    fn post<'a>(&'a self, message: &'a Message) -> Posting<'a> {
        Box::pin(self.send(message))
    }
}

/// The admin realm's store: its login flow and its directory.
pub trait AdminStore: LoginStore + AdminDirectory {}

impl<T: LoginStore + AdminDirectory> AdminStore for T {}

/// The staff realm's store: its login flow, its directory, and each business's team.
pub trait StaffStore: LoginStore + StaffDirectory + TeamStore {}

impl<T: LoginStore + StaffDirectory + TeamStore> StaffStore for T {}

/// Where a realm's accounts log in: sessions, invites, ceremonies, passkeys, and limits (ADR-0088). A session is reached
/// only by its token's hash and realm, one at a time (ADR-0048), and the store keeps each realm's limits beside its own
/// clock, so no caller can stretch a session. Each realm's store works only on its own tables, and names its realm,
/// which its use cases take from it. Each change that names an actor records an audit event naming it, with its outcome,
/// in the same transaction (`v5.0.0-16.3.1`). The Postgres adapter implements it for admins and for staff, and tests
/// may use their own.
pub trait LoginStore {
    /// The realm whose accounts this store keeps: every session it starts, finds, and ends is in it.
    const REALM: Realm;

    /// Records a request on the live session stored under a token's hash in this store's realm, and returns it. A
    /// session past either of the realm's limits is removed instead, and none is returned; one in another realm is
    /// none. Checking and recording are one step, so requests at the same moment cannot end a live session or revive
    /// a spent one.
    fn use_session(
        &self,
        hash: &[u8; 32],
    ) -> impl Future<Output = Result<Option<Session>, Error>> + Send;

    /// How long the live session stored under a token's hash in this store's realm has left, read without counting
    /// as a request, so it renews nothing and removes nothing; none for a session spent, of another realm, or of a
    /// disabled account, as [`Self::use_session`] would find.
    fn session_left(
        &self,
        hash: &[u8; 32],
    ) -> impl Future<Output = Result<Option<TimeLeft>, Error>> + Send;

    /// Removes a session in this store's realm, and says whether it was live when it ended, recording a live one's
    /// ending, by `actor`'s device, under its account (#167).
    fn delete_session(
        &self,
        hash: &[u8; 32],
        actor: &Actor,
    ) -> impl Future<Output = Result<bool, Error>> + Send;

    /// The admin an invite link's hash names, while the link is open, the admin enabled, and no passkey
    /// theirs yet. Reading it uses nothing up.
    fn invitee(
        &self,
        link: &[u8; 32],
    ) -> impl Future<Output = Result<Option<Invitee>, Error>> + Send;

    /// Keeps a join ceremony's state under its token's hash, with the link it will redeem, for ten minutes
    /// but never past the link, and replacing any earlier ceremony of that link. Says whether it did: not once the
    /// link has ended, or its admin is disabled or holds a passkey.
    fn begin_join(
        &self,
        token: &[u8; 32],
        admin: AccountId,
        link: &[u8; 32],
        state: &str,
    ) -> impl Future<Output = Result<bool, Error>> + Send;

    /// Takes a join ceremony by its token's hash, once: it is gone after this, and returned only if it was
    /// still open.
    fn take_join(
        &self,
        token: &[u8; 32],
    ) -> impl Future<Output = Result<Option<TakenCeremony>, Error>> + Send;

    /// Stores an admin's first passkey and uses up the ceremony's link, in one transaction, and says what came of
    /// it, or none for an admin who does not exist. Redeemed, it ends the session the browser presented and starts
    /// `session`, unproven, since it rests on the link (ADR-0069).
    fn redeem_invite(
        &self,
        ceremony: &TakenCeremony,
        passkey: &NewPasskey,
        session: &SessionStart,
        actor: &Actor,
    ) -> impl Future<Output = Result<Option<Redemption>, Error>> + Send;

    /// Admits an attempt from a network under a limit, or says how long it must wait. A limit of attempts counts
    /// it; the limit of failures only checks.
    fn admit(
        &self,
        limit: RateLimit,
        from: &Requester,
    ) -> impl Future<Output = Result<Option<Duration>, Error>> + Send;

    /// Counts a failed login from a network.
    fn fail(&self, from: &Requester) -> impl Future<Output = Result<(), Error>> + Send;

    /// Keeps a login ceremony's state for ten minutes under its token's hash. It never refuses for the number
    /// open: past a bound, the oldest end.
    fn begin_login(
        &self,
        token: &[u8; 32],
        state: &str,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Records a ceremony refused after it was taken but before storage was asked to store its outcome, so every
    /// refusal after a ceremony was taken is in the audit record (`v5.0.0-16.3.1`).
    fn refuse(
        &self,
        refusal: &Refusal<'_>,
        actor: &Actor,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Takes a login ceremony's state by its token's hash, once, if it was still open.
    fn take_login(
        &self,
        token: &[u8; 32],
    ) -> impl Future<Output = Result<Option<String>, Error>> + Send;

    /// The passkey a credential ID names, with its admin and whether they are disabled, or none for a credential
    /// no admin holds.
    fn signer(
        &self,
        credential: &[u8],
    ) -> impl Future<Output = Result<Option<Signer>, Error>> + Send;

    /// Stores a passkey's verified new state, if no other login changed it meanwhile and it is still the signer's
    /// admin's, and starts the admin's session under a token's hash, in one transaction, recording it; says
    /// what came of it, or none for a passkey that no longer exists or is no longer theirs.
    fn login(
        &self,
        signer: &Signer,
        passkey: &str,
        session: &SessionStart,
        actor: &Actor,
    ) -> impl Future<Output = Result<Option<Login>, Error>> + Send;

    /// The account a live session's hash names, if the admin is enabled. Reading it changes nothing.
    fn account(
        &self,
        session: &[u8; 32],
    ) -> impl Future<Output = Result<Option<Account>, Error>> + Send;

    /// The account a live session's hash names, as [`Self::account`] finds it, if the session began within ten
    /// minutes and the admin holds fewer than two passkeys.
    fn adder(
        &self,
        session: &[u8; 32],
    ) -> impl Future<Output = Result<Option<Account>, Error>> + Send;

    /// The live sessions of whoever's live session this hash names, the presented one first, found from it alone.
    /// Reading them renews nothing.
    fn sessions_of(
        &self,
        session: &[u8; 32],
    ) -> impl Future<Output = Result<Vec<ListedSession>, Error>> + Send;

    /// Records that a request to `route` from `from`'s network failed `check`, naming `actor`, if the network has room
    /// under the limit on recording refusals, and counts it; past that, records once a window that later refusals go
    /// unrecorded (ADR-0081).
    fn record_refusal(
        &self,
        from: &Requester,
        route: &'static str,
        check: FailedCheck,
        actor: &Actor,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Ends the other session `only` of the admin whose session this hash names, or every other one, if that
    /// session is live, its admin enabled, and it was proven by a passkey within ten minutes, and records the
    /// attempt either way. The admin comes from the session alone, never from the caller. It never ends this
    /// session, and never answers [`Ending::Refused`], which the request's own checks decide.
    fn end_other_sessions(
        &self,
        session: &[u8; 32],
        only: Option<&SessionId>,
        actor: &Actor,
    ) -> impl Future<Output = Result<Ending, Error>> + Send;

    /// Keeps a ceremony for adding a passkey for ten minutes, if the session still allows it, replacing the
    /// admin's earlier one; says whether it did.
    fn begin_addition(
        &self,
        token: &[u8; 32],
        session: &[u8; 32],
        admin: AccountId,
        state: &str,
    ) -> impl Future<Output = Result<bool, Error>> + Send;

    /// Takes an addition by its token's hash for the session that began it, once, if it was still open. Only storing
    /// its passkey then uses it up.
    fn take_addition(
        &self,
        token: &[u8; 32],
        session: &[u8; 32],
    ) -> impl Future<Output = Result<Option<TakenAddition>, Error>> + Send;

    /// Stores an admin's added passkey while their session is live, using up the addition it took and ending
    /// their other sessions, in one transaction, recording it; says what came of it, or none for an admin who
    /// does not exist.
    fn add_passkey(
        &self,
        addition: &TakenAddition,
        passkey: &NewPasskey,
        actor: &Actor,
    ) -> impl Future<Output = Result<Option<Addition>, Error>> + Send;
}

/// Managing the admin realm's accounts, which only the admin command does: creating, disabling, and listing admins,
/// their invites and email, and ending their sessions. Each change records an audit event naming its actor, with its
/// outcome, in the same transaction.
pub trait AdminDirectory {
    /// Stores a new admin, with their email address if given and no passkey yet. Each change below records an
    /// audit event naming `actor`, with its outcome, in the same transaction.
    fn insert_admin(
        &self,
        name: &PersonName,
        email: Option<&Address>,
        actor: &Actor,
    ) -> impl Future<Output = Result<AccountId, Error>> + Send;

    /// An admin's email address, or none for one who has none or does not exist (ADR-0074).
    fn admin_email(
        &self,
        admin: AccountId,
    ) -> impl Future<Output = Result<Option<Address>, Error>> + Send;

    /// Sets an admin's email address, and says whether it changed, or none for an admin who does not exist
    /// (ADR-0074).
    fn set_admin_email(
        &self,
        admin: AccountId,
        email: &Address,
        actor: &Actor,
    ) -> impl Future<Output = Result<Option<bool>, Error>> + Send;

    /// Stores an admin's invite link under its token's hash, good for an hour, replacing any earlier one,
    /// and says what came of it: an admin who is disabled or holds a passkey is issued none, and one who does
    /// not exist is none.
    fn issue_invite(
        &self,
        hash: &[u8; 32],
        admin: AccountId,
        actor: &Actor,
    ) -> impl Future<Output = Result<Option<Issue>, Error>> + Send;

    /// Disables an admin, ending every session and invite link they hold, and says what it did, or none for
    /// an admin who does not exist.
    fn disable_admin(
        &self,
        admin: AccountId,
        actor: &Actor,
    ) -> impl Future<Output = Result<Option<Disabling>, Error>> + Send;

    /// Removes every passkey an admin holds, ending their sessions and any ceremony, and says what it did, or none
    /// for an admin who does not exist.
    fn remove_passkeys(
        &self,
        admin: AccountId,
        actor: &Actor,
    ) -> impl Future<Output = Result<Option<Removal>, Error>> + Send;

    /// Enables a disabled admin again, and says whether they were disabled, or none for an admin who does not
    /// exist. Their sessions stay ended.
    fn enable_admin(
        &self,
        admin: AccountId,
        actor: &Actor,
    ) -> impl Future<Output = Result<Option<bool>, Error>> + Send;

    /// Ends every session one admin holds in a realm, and says how many, or none for an admin who does not
    /// exist.
    fn end_admin_sessions(
        &self,
        admin: AccountId,
        realm: Realm,
        actor: &Actor,
    ) -> impl Future<Output = Result<Option<u64>, Error>> + Send;

    /// Ends every session in a realm, and says how many.
    fn end_realm_sessions(
        &self,
        realm: Realm,
        actor: &Actor,
    ) -> impl Future<Output = Result<u64, Error>> + Send;

    /// Every admin, oldest first.
    fn list_admins(&self) -> impl Future<Output = Result<Vec<AdminSummary>, Error>> + Send;
}

/// Managing the staff realm's accounts, which only the staff command does so far (ADR-0089). Each change records an
/// audit event naming its actor, with its outcome, in the business's transaction but Tetsu's record, since whether
/// the person was staff already, or disabled, may be from another business.
pub trait StaffDirectory {
    /// Issues `invite` under an invite link's hash, good once and for seven days, making the staff member its email
    /// names, with its name and no passkey, if there is none. A new invite replaces any open one of theirs at the
    /// business. A disabled staff member, or one who holds a role there already, is issued none.
    fn issue_staff_invite(
        &self,
        invite: &NewStaffInvite,
        hash: &[u8; 32],
        actor: &Actor,
    ) -> impl Future<Output = Result<StaffIssue, Error>> + Send;

    /// What an open invite link gives, read before anyone acts on it, or none for a link that ended, was replaced,
    /// or names someone disabled.
    fn invitation(
        &self,
        link: &[u8; 32],
    ) -> impl Future<Output = Result<Option<Invitation>, Error>> + Send;

    /// Accepts an open invite for the staff member `staff`, whose fresh session a token's hash names, adding its role
    /// at its business and spending it, in that business's transaction, and records what came of it there. An invite
    /// that names someone else stays open.
    fn accept_invite(
        &self,
        link: &[u8; 32],
        session: &[u8; 32],
        staff: AccountId,
        actor: &Actor,
    ) -> impl Future<Output = Result<Acceptance, Error>> + Send;
}

/// A business's team, as its Team page reads and changes it (ADR-0096). Every call runs in the business's own
/// transaction, under its row security, so it never reaches another business. Each change takes the [`Member`]
/// acting and the roles the use case decided on, and changes nothing unless they still hold, while the business's
/// owners are locked; it records an audit event naming its actor, with its outcome, in the business's own record.
pub trait TeamStore {
    /// The role a staff member holds at a business, or none.
    fn role_at(
        &self,
        business: BusinessId,
        staff: AccountId,
    ) -> impl Future<Output = Result<Option<Role>, Error>> + Send;

    /// A business's staff and open invites.
    fn team(&self, business: BusinessId) -> impl Future<Output = Result<Team, Error>> + Send;

    /// The business a staff member joined first, or none for one who holds no role.
    fn first_business(
        &self,
        staff: AccountId,
    ) -> impl Future<Output = Result<Option<BusinessId>, Error>> + Send;

    /// Issues `invite` at the member's business under an invite link's hash, as
    /// [`StaffDirectory::issue_staff_invite`] does, and says whether it was issued. It answers
    /// [`TeamChange::Done`] for a disabled staff member too, issuing them nothing, so the business learns nothing
    /// another gave them, and records that only as not issued. Over an open invite to a role the member does not
    /// reach, it answers [`TeamChange::Refused`].
    fn invite_from(
        &self,
        member: &Member,
        invite: &TeamInvite,
        hash: &[u8; 32],
        actor: &Actor,
    ) -> impl Future<Output = Result<(TeamChange, bool), Error>> + Send;

    /// Changes a staff member's role at the member's business, as `change` names it.
    fn change_role(
        &self,
        member: &Member,
        change: RoleChange,
        actor: &Actor,
    ) -> impl Future<Output = Result<TeamChange, Error>> + Send;

    /// Removes `staff`, who holds `held`, from the member's business, with any invite of theirs there.
    fn remove_staff(
        &self,
        member: &Member,
        staff: AccountId,
        held: Role,
        actor: &Actor,
    ) -> impl Future<Output = Result<TeamChange, Error>> + Send;

    /// Cancels the open invite `invite`, which gives `held`, at the member's business.
    fn cancel_invite(
        &self,
        member: &Member,
        invite: &InviteId,
        held: Role,
        actor: &Actor,
    ) -> impl Future<Output = Result<TeamChange, Error>> + Send;

    /// Admits one more invite from a staff member under their limit, or says how long they must wait.
    fn admit_inviter(
        &self,
        staff: AccountId,
    ) -> impl Future<Output = Result<Option<Duration>, Error>> + Send;
}
