//! A business's Team page on the staff host (ADR-0096), at `/<business>/settings/team`: its staff and open invites,
//! with forms to invite, change a role, remove someone, and cancel or resend an invite. Every request finds the
//! business by its slug and reads the person's role there, so a role removed is refused on the next request, and a
//! business the person holds no role at, or whose team they may not see, answers as a page that does not exist.
//! Each form posts with the session's form token and returns to the page, which says what came of it.

use axum::extract::Path;

use super::{
    Accounts, Group, HeaderMap, Person, REALM, Response, Served, Shared, Shell, State, StatusCode,
    Template, Uri, Web, ago, change, field, page, query, refused_form, return_to, see_other, text,
    unavailable_page,
};
use super::{Bytes, Extension, ForChange, IntoResponse, MethodRouter, NotFoundPage, scheme_of};
use crate::domain::Team;
use crate::domain::{
    AccountId, Actor, Device, Error, InviteId, Member, OpenInvite, Permission, PersonName, Realm,
    Role, RoleChange, Session, TeamChange, TeamInvite, TeamMember,
};
use crate::mail::staff_invite_message;
use crate::ports::{Business, LoginStore, StaffStore};
use crate::use_cases::Invited;
use axum::http::header;
use axum::routing::{get, post};
use std::sync::Arc;
use std::time::SystemTime;
use tetsu_kernel::{Address, Requester};

/// The Team page, and where its forms post.
pub(super) const TEAM: &str = "/{segment}/settings/team";
const INVITE: &str = "/{segment}/settings/team/invite";
const ROLE: &str = "/{segment}/settings/team/role";
const REMOVE: &str = "/{segment}/settings/team/remove";
const CANCEL: &str = "/{segment}/settings/team/cancel";

/// The Team page's routes, added to staff's.
pub(super) fn routes<S: StaffStore + Send + Sync + 'static>(
    routes: axum::Router<Arc<Served<S>>>,
) -> axum::Router<Arc<Served<S>>> {
    routes
        .route(TEAM, get(team_page::<S>))
        .route(INVITE, act_route::<S>(Act::Invite))
        .route(ROLE, act_route::<S>(Act::Role))
        .route(
            REMOVE,
            get(remove_page::<S>).merge(act_route::<S>(Act::Remove)),
        )
        .route(CANCEL, act_route::<S>(Act::Cancel))
}

/// The Team page's own path for a business.
fn team_of(slug: &str) -> String {
    format!("/{slug}/settings/team")
}

/// A role as a form offers it: the name it posts and the name it shows.
struct Choice {
    value: &'static str,
    title: &'static str,
}

impl Choice {
    fn of(role: Role) -> Self {
        Self {
            value: role.as_str(),
            title: role.title(),
        }
    }
}

/// One of the business's staff, as the page lists them, with what the person viewing may do to them: the roles
/// they may move them between, none for someone they do not reach or themselves, and whether they may remove them.
struct Listed {
    id: i64,
    name: String,
    email: String,
    role: &'static str,
    held: &'static str,
    joined: String,
    you: bool,
    roles: Vec<Choice>,
    removable: bool,
}

impl Listed {
    fn of(listed: TeamMember, member: &Member, now: SystemTime) -> Self {
        let you = listed.staff == member.staff();
        let reached = !you && member.may_reach(Permission::ChangeRole, listed.role);
        Self {
            id: listed.staff.get(),
            roles: if reached {
                choices(member, Permission::ChangeRole)
            } else {
                Vec::new()
            },
            removable: !you && member.may_reach(Permission::RemoveStaff, listed.role),
            role: listed.role.title(),
            held: listed.role.as_str(),
            joined: ago(listed.joined_at, now),
            name: listed.name,
            email: listed.email,
            you,
        }
    }
}

/// One of the business's open invites, as the page lists it, with whether the person viewing may resend or cancel
/// it.
struct Pending {
    id: String,
    email: String,
    role: &'static str,
    held: &'static str,
    ends: String,
    actionable: bool,
}

impl Pending {
    fn of(invite: OpenInvite, member: &Member, now: SystemTime) -> Self {
        Self {
            actionable: member.may_reach(Permission::CancelInvite, invite.role),
            role: invite.role.title(),
            held: invite.role.as_str(),
            ends: ends_in(invite.ends_at, now),
            id: invite.id.as_str().to_string(),
            email: invite.email,
        }
    }
}

#[derive(Template)]
#[template(path = "account_team.html")]
struct TeamPage {
    shell: Shell,
    /// The business's name, and the page's own path, which its forms post below.
    called: String,
    team: String,
    /// What the last change came to, as the page says it.
    said: Option<&'static str>,
    /// The roles the person viewing may invite to, highest first; none for one who may not invite.
    invitable: Vec<Choice>,
    members: Vec<Listed>,
    invites: Vec<Pending>,
}

#[derive(Template)]
#[template(path = "account_team_remove.html")]
struct RemovePage {
    shell: Shell,
    called: String,
    team: String,
    id: i64,
    name: String,
    held: &'static str,
}

through_shell!(TeamPage, RemovePage);

/// What a business's page finds for a request: the page itself, someone to send to log in, or nothing to show.
enum Found<T> {
    Shown(T),
    LoggedOut,
    Missing,
}

/// The live session a page's request comes with, the business its slug names, and the member it makes of the
/// session's staff member there, with the person the page's shell names, as a group for that business; missing for a
/// slug no business has, or one the person holds no role at or may not see the team of.
async fn at_business<S: StaffStore>(
    served: &Served<S>,
    headers: &HeaderMap,
    slug: &str,
) -> Result<Found<(Session, Business, Member, Person)>, Error> {
    let Some((session, _, mut person)) = super::viewer(served, headers).await? else {
        return Ok(Found::LoggedOut);
    };
    match member_there(&served.admins, &served.web, &session, slug).await? {
        Some((business, member)) => {
            person.group = Some(group_of(&business));
            Ok(Found::Shown((session, business, member, person)))
        }
        None => Ok(Found::Missing),
    }
}

/// The business a slug names and the session's staff member's place there, for one who may see its team.
async fn member_there<S: StaffStore>(
    staff: &Accounts<S>,
    web: &Web,
    session: &Session,
    slug: &str,
) -> Result<Option<(Business, Member)>, Error> {
    let Some(business) = web.businesses.at(slug).await? else {
        return Ok(None);
    };
    let member = staff.member_at(session, business.id).await?;
    Ok(member
        .filter(|member| member.role().may(Permission::SeeTeam))
        .map(|member| (business, member)))
}

fn group_of(business: &Business) -> Group {
    Group {
        name: business.name.clone(),
        team: team_of(&business.slug),
    }
}

/// The business group a staff member's own pages show: the business they joined first, while they may see its team
/// (ADR-0096).
pub(super) fn first_group<'a, S: StaffStore + Send + Sync>(
    served: &'a Served<S>,
    session: &'a Session,
) -> super::Grouped<'a> {
    Box::pin(async move {
        let staff = &served.admins;
        let Some(first) = staff.first_business(session).await? else {
            return Ok(None);
        };
        let Some(business) = served.web.businesses.found(first).await? else {
            return Ok(None);
        };
        let member = staff.member_at(session, first).await?;
        let sees = member.is_some_and(|member| member.role().may(Permission::SeeTeam));
        Ok(sees.then(|| group_of(&business)))
    })
}

/// The answer to a business's page that is not there for this person: the same page any unknown path gets.
fn missing(web: &Web, headers: &HeaderMap) -> Response {
    let not_found = NotFoundPage {
        demo: web.demo,
        stylesheet: web.stylesheet,
        login: None,
        scheme: scheme_of(headers).as_str(),
    };
    (StatusCode::NOT_FOUND, page(&not_found)).into_response()
}

/// What a business's page shows the request it finds there: the business, the member acting, the person the
/// shell names, and the team.
struct There {
    business: Business,
    member: Member,
    person: Person,
    team: Team,
}

/// A business's page's shell, naming the member's role at the business.
fn shell_for<S: LoginStore>(
    web: &Web,
    headers: &HeaderMap,
    person: Person,
    member: &Member,
    here: &str,
) -> Shell {
    let mut shell = Shell::of::<S>(web, headers, person, here);
    shell.role = member.role().title();
    shell
}

/// What a request finds at a business's page, with its team, which the member may see.
async fn there<S: StaffStore>(
    served: &Served<S>,
    headers: &HeaderMap,
    slug: &str,
) -> Result<Found<There>, Error> {
    let (_, business, member, person) = match at_business(served, headers, slug).await? {
        Found::Shown(found) => found,
        Found::LoggedOut => return Ok(Found::LoggedOut),
        Found::Missing => return Ok(Found::Missing),
    };
    let team = served.admins.team(&member).await?.unwrap_or_default();
    Ok(Found::Shown(There {
        business,
        member,
        person,
        team,
    }))
}

/// A business's page, as `build` makes it of what the request finds there. Anyone without a live session is sent to
/// log in, returning to `back` after.
async fn business_page<S: StaffStore>(
    served: &Served<S>,
    headers: &HeaderMap,
    slug: &str,
    back: &str,
    build: impl FnOnce(There) -> Response,
) -> Response {
    let web = &served.web;
    match there(served, headers, slug).await {
        Ok(Found::Shown(found)) => build(found),
        Ok(Found::LoggedOut) => see_other(&return_to(&REALM, back)),
        Ok(Found::Missing) => missing(web, headers),
        Err(error) => unavailable_page(web, "could not show a business's page", &error),
    }
}

/// The Team page.
async fn team_page<S: StaffStore>(
    State(served): Shared<S>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let said = query(&uri, "said");
    business_page(&served, &headers, &slug, uri.path(), |found| {
        team_shown::<S>(&served.web, &headers, found, said)
    })
    .await
}

fn team_shown<S: StaffStore>(
    web: &Web,
    headers: &HeaderMap,
    found: There,
    said: Option<&str>,
) -> Response {
    let There {
        business,
        member,
        person,
        team,
    } = found;
    let path = team_of(&business.slug);
    let shell = shell_for::<S>(web, headers, person, &member, &path);
    let now = SystemTime::now();
    let members = team
        .members
        .into_iter()
        .map(|listed| Listed::of(listed, &member, now))
        .collect();
    let invites = team
        .invites
        .into_iter()
        .map(|invite| Pending::of(invite, &member, now))
        .collect();
    page(&TeamPage {
        shell,
        team: path,
        called: business.name,
        said: said.and_then(saying),
        invitable: choices(&member, Permission::InviteStaff),
        members,
        invites,
    })
}

/// The roles a member may grant under `permission`, highest first.
fn choices(member: &Member, permission: Permission) -> Vec<Choice> {
    Role::ALL
        .into_iter()
        .filter(|role| member.may_reach(permission, *role))
        .map(Choice::of)
        .collect()
}

/// How long an open invite has left, as the page says it: within a day, or in days, counting a part of one.
fn ends_in(at: SystemTime, now: SystemTime) -> String {
    let left = at.duration_since(now).unwrap_or_default().as_secs();
    if left < 86_400 {
        return "Ends within a day".to_string();
    }
    match left.div_ceil(86_400) {
        1 => "Ends in 1 day".to_string(),
        days => format!("Ends in {days} days"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// An invite's time left reads within a day under one, and in whole days counting a part of one above.
    #[test]
    fn an_invite_ends_in_days_counting_a_part() {
        let now = SystemTime::now();
        let after = |seconds| ends_in(now + Duration::from_secs(seconds), now);
        let said = [0, 86_399, 86_400, 86_401, 6 * 86_400 - 5].map(after);
        assert_eq!(
            said,
            [
                "Ends within a day",
                "Ends within a day",
                "Ends in 1 day",
                "Ends in 2 days",
                "Ends in 6 days"
            ]
        );
        assert_eq!(
            ends_in(now, now + Duration::from_secs(9)),
            "Ends within a day"
        );
    }

    /// Each word a change's answer carries says its own sentence, and any other word says nothing.
    #[test]
    fn only_a_changes_own_words_say_anything() {
        let words = [
            "sent",
            "unsent",
            "member",
            "changed",
            "unchanged",
            "removed",
            "cancelled",
            "stale",
            "last_owner",
            "refused",
            "limited",
            "invalid",
        ];
        let sentences: std::collections::BTreeSet<_> =
            words.iter().filter_map(|word| saying(word)).collect();
        assert_eq!(sentences.len(), words.len());
        for other in ["", "Sent", "<b>", "sent "] {
            assert_eq!(saying(other), None, "{other:?}");
        }
    }

    /// Each change's outcome has its own word, and a change made says the act's.
    #[test]
    fn each_outcome_has_its_word() {
        let outcomes = [
            TeamChange::Done,
            TeamChange::Unchanged,
            TeamChange::Refused,
            TeamChange::Stale,
            TeamChange::LastOwner,
            TeamChange::Member,
        ];
        assert_eq!(
            outcomes.map(|change| said(change, "made")),
            [
                "made",
                "unchanged",
                "refused",
                "stale",
                "last_owner",
                "member"
            ]
        );
        let acts = [Act::Invite, Act::Role, Act::Remove, Act::Cancel];
        assert_eq!(
            acts.map(Act::done),
            ["sent", "changed", "removed", "cancelled"]
        );
        assert_eq!(acts.map(Act::route), [INVITE, ROLE, REMOVE, CANCEL]);
    }
}

/// What the page says for what a change came to, by the word its address carries; none for any other word, so the
/// address never puts text of its own on the page.
fn saying(said: &str) -> Option<&'static str> {
    SAYINGS
        .iter()
        .find(|(word, _)| *word == said)
        .map(|(_, sentence)| *sentence)
}

/// Each word a change's answer carries, and the sentence the page says for it.
const SAYINGS: [(&str, &str); 12] = [
    ("sent", "Invite sent."),
    (
        "unsent",
        "The invite is ready, but its email could not be sent. Resend it in a minute.",
    ),
    ("member", "They're on the team already."),
    ("changed", "Role changed."),
    ("unchanged", "They hold that role already."),
    ("removed", "Removed from the team."),
    ("cancelled", "Invite cancelled."),
    (
        "stale",
        "The team changed meanwhile, so nothing changed. Check it and try again.",
    ),
    ("last_owner", "A business keeps at least one owner."),
    ("refused", "You can't make that change."),
    (
        "limited",
        "You've sent this hour's invites. Try again later.",
    ),
    (
        "invalid",
        "Check the name, email, and role, then try again.",
    ),
];

/// Confirming that someone is to be removed, which a form on this page does, without script. Someone no longer on
/// the team, or whom the person viewing may not remove, goes back to the Team page, which says so.
async fn remove_page<S: StaffStore>(
    State(served): Shared<S>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let who = query(&uri, "staff").and_then(|id| id.parse::<i64>().ok());
    business_page(&served, &headers, &slug, &team_of(&slug), |found| {
        removing::<S>(&served.web, &headers, found, who)
    })
    .await
}

fn removing<S: StaffStore>(
    web: &Web,
    headers: &HeaderMap,
    found: There,
    who: Option<i64>,
) -> Response {
    let There {
        business,
        member,
        person,
        team,
    } = found;
    let path = team_of(&business.slug);
    let listed = team.members.into_iter().find(|listed| {
        Some(listed.staff.get()) == who
            && listed.staff != member.staff()
            && member.may_reach(Permission::RemoveStaff, listed.role)
    });
    let Some(listed) = listed else {
        return see_other(&format!("{path}?said=stale"));
    };
    page(&RemovePage {
        shell: shell_for::<S>(web, headers, person, &member, &path),
        called: business.name,
        team: path,
        id: listed.staff.get(),
        name: listed.name,
        held: listed.role.as_str(),
    })
}

/// The change a Team page form makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Act {
    Invite,
    Role,
    Remove,
    Cancel,
}

impl Act {
    /// The route the form posts to, as a refused request's record names it.
    const fn route(self) -> &'static str {
        match self {
            Self::Invite => INVITE,
            Self::Role => ROLE,
            Self::Remove => REMOVE,
            Self::Cancel => CANCEL,
        }
    }

    /// The word the page's address carries once the change is made.
    const fn done(self) -> &'static str {
        match self {
            Self::Invite => "sent",
            Self::Role => "changed",
            Self::Remove => "removed",
            Self::Cancel => "cancelled",
        }
    }
}

fn act_route<S: StaffStore + Send + Sync + 'static>(act: Act) -> MethodRouter<Arc<Served<S>>> {
    post(
        move |State(served): Shared<S>,
              Extension(from): Extension<Requester>,
              Path(slug): Path<String>,
              headers: HeaderMap,
              body: Bytes| async move {
            let form = Form {
                from: &from,
                headers: &headers,
                slug: &slug,
                body: &body,
            };
            acted(&served, &form, act).await
        },
    )
}

/// A posted Team page form: where it came from, its headers, the business its address names, and its body.
#[derive(Clone, Copy)]
struct Form<'a> {
    from: &'a Requester,
    headers: &'a HeaderMap,
    slug: &'a str,
    body: &'a [u8],
}

/// A Team page form: checked as every change is, so a request from elsewhere or without its session's form token is
/// refused, one whose session has ended goes to log in, and one for a business the person may not see the team of
/// answers as missing. It then returns to the Team page, saying what came of it.
async fn acted<S: StaffStore>(served: &Served<S>, form: &Form<'_>, act: Act) -> Response {
    let Form {
        from,
        headers,
        slug,
        body,
    } = *form;
    let web = &served.web;
    let change = change(headers, field(body, "form").unwrap_or_default());
    let checked = match served
        .admins
        .session_for_change(&web.origins, &change)
        .await
    {
        Ok(ForChange::Acting(checked)) => checked,
        Ok(ForChange::NoSession) => return see_other(&return_to(&REALM, &team_of(slug))),
        Ok(ForChange::Refused(check)) => {
            return refused_form(served, from, headers, act.route(), check).await;
        }
        Err(error) => return unavailable_page(web, "could not change a business's team", &error),
    };
    let session = checked.session();
    let found = member_there(&served.admins, web, session, slug).await;
    let (business, member) = match found {
        Ok(Some(found)) => found,
        Ok(None) => return missing(web, headers),
        Err(error) => return unavailable_page(web, "could not change a business's team", &error),
    };
    let device = Device::from_user_agent(text(headers, &header::USER_AGENT));
    let actor = Actor::browser(Realm::Staff, Some(session.admin()), device);
    let asked = Asked {
        served,
        business: &business,
        member: &member,
        actor: &actor,
        body,
    };
    match asked.made(act).await {
        Ok(said) => see_other(&format!("{}?said={said}", team_of(&business.slug))),
        Err(error) => unavailable_page(web, "could not change a business's team", &error),
    }
}

/// A checked Team page form, with who asks, where, and what it holds.
struct Asked<'a, S> {
    served: &'a Served<S>,
    business: &'a Business,
    member: &'a Member,
    actor: &'a Actor,
    body: &'a [u8],
}

impl<S: StaffStore> Asked<'_, S> {
    /// A form field's value, decoded as browsers encode a form: `+` for a space, and percent escapes.
    fn value(&self, name: &str) -> Option<String> {
        let raw = field(self.body, name)?;
        tetsu_kernel::percent_decoded(&raw.replace('+', " "))
    }

    fn role(&self, name: &str) -> Option<Role> {
        self.value(name).and_then(|role| Role::parse(&role))
    }

    fn staff(&self) -> Option<AccountId> {
        let id = self.value("staff")?.parse::<i64>().ok()?;
        Some(AccountId::new(id))
    }

    /// Makes the change, and says in a word what came of it; a form missing a field, or holding one no form
    /// writes, is invalid, and changes nothing.
    async fn made(&self, act: Act) -> Result<&'static str, Error> {
        let (staff, member, actor) = (&self.served.admins, self.member, self.actor);
        let held = self.role("held");
        let change = match act {
            Act::Invite => return self.invite().await,
            Act::Role => match (self.staff(), held, self.role("role")) {
                (Some(who), Some(held), Some(wanted)) => {
                    let change = RoleChange {
                        staff: who,
                        held,
                        wanted,
                    };
                    staff.change_role(member, change, actor).await?
                }
                _ => return Ok("invalid"),
            },
            Act::Remove => match (self.staff(), held) {
                (Some(who), Some(held)) => staff.remove_staff(member, who, held, actor).await?,
                _ => return Ok("invalid"),
            },
            Act::Cancel => match (
                self.value("invite").and_then(|id| InviteId::parse(&id)),
                held,
            ) {
                (Some(invite), Some(held)) => {
                    staff.cancel_invite(member, &invite, held, actor).await?
                }
                _ => return Ok("invalid"),
            },
        };
        Ok(said(change, act.done()))
    }

    /// Invites the person the form names, and emails them the link if one was issued.
    async fn invite(&self) -> Result<&'static str, Error> {
        // A resend names no one; a name sent must be one.
        let name = self
            .value("name")
            .map(|name| PersonName::parse(name.trim()));
        let email = self
            .value("email")
            .and_then(|email| Address::parse(email.trim()));
        let (Ok(name), Some(email), Some(role)) = (name.transpose(), email, self.role("role"))
        else {
            return Ok("invalid");
        };
        let staff = &self.served.admins;
        let invited = staff
            .invite_from_team(
                self.member,
                TeamInvite {
                    name,
                    email: email.clone(),
                    role,
                },
                self.actor,
            )
            .await;
        let Invited { change, token } = match invited {
            Ok(invited) => invited,
            Err(Error::Limited(_)) => return Ok("limited"),
            Err(error) => return Err(error),
        };
        let Some(token) = token else {
            return Ok(said(change, Act::Invite.done()));
        };
        let web = &self.served.web;
        let link = format!("{}{}#{}", web.links, REALM.join, token.to_text());
        let message = staff_invite_message(email, &self.business.name, role, &link, &token);
        let sent = match message {
            Some(message) => web.post.post(&message).await.is_ok(),
            None => false,
        };
        Ok(if sent { "sent" } else { "unsent" })
    }
}

/// The word for what a change came to, with `done` for one made.
const fn said(change: TeamChange, done: &'static str) -> &'static str {
    match change {
        TeamChange::Done => done,
        TeamChange::Unchanged => "unchanged",
        TeamChange::Refused => "refused",
        TeamChange::Stale => "stale",
        TeamChange::LastOwner => "last_owner",
        TeamChange::Member => "member",
    }
}
