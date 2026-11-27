//! A business's Team page over Postgres (ADR-0096): who holds which role, the checks each change names, and the
//! changes themselves, each in its business's own transaction and record.

use std::time::SystemTime;

use serde_json::{Value, json};
use tetsu_kernel::Address;
use tetsu_postgres::connect;
use tetsu_postgres::test_support::{TestDatabase, migrated_database};

use crate::join_tests::{actor, app_pool, as_db_admin, number};
use crate::ports::TeamStore;
use crate::{
    AccountId, Accounts, BusinessId, Error, Identity, InviteId, Member, PersonName, Realm, Role,
    RoleChange, Session, TeamChange, TeamInvite,
};

/// The business the team works at, Fern Studio to the pages' tests, and another, which nothing here may reach.
pub(crate) const BUSINESS: i64 = 4;
const OTHER: i64 = 5;

/// Its staff, by id: Olu owns it, Mae manages it, Fen works its front desk, and Ike teaches; Zed works only at the
/// other business, where Olu teaches too.
pub(crate) const OLU: i64 = 1;
pub(crate) const MAE: i64 = 2;
pub(crate) const FEN: i64 = 3;
pub(crate) const IKE: i64 = 4;
const ZED: i64 = 5;

/// Staff's use cases over a fresh migrated database holding the team, an open invite to the front desk for Pip, an
/// ended one for Rue, and one at the other business for Zed.
pub(crate) async fn team() -> (Accounts<impl crate::ports::StaffStore>, TestDatabase) {
    let database = migrated_database(&[&Identity])
        .await
        .expect("a migrated test database");
    as_db_admin(
        &database,
        &format!(
            "INSERT INTO identity.staff (id, name, email) OVERRIDING SYSTEM VALUE VALUES
               ({OLU}, 'Olu Bello', 'olu@example.com'), ({MAE}, 'Mae Lin', 'mae@example.com'),
               ({FEN}, 'Fen Ito', 'fen@example.com'), ({IKE}, 'Ike Obi', 'ike@example.com'),
               ({ZED}, 'Zed Ray', 'zed@example.com'), (6, 'Pip Moss', 'pip@example.com'),
               (7, 'Rue Hale', 'rue@example.com');
             INSERT INTO identity.staff_roles (tenant_id, staff_id, role_name, created_at) VALUES
               ({BUSINESS}, {OLU}, 'owner', now() - interval '4 days'),
               ({BUSINESS}, {MAE}, 'manager', now() - interval '3 days'),
               ({BUSINESS}, {FEN}, 'front_desk', now() - interval '2 days'),
               ({BUSINESS}, {IKE}, 'instructor', now() - interval '1 day'),
               ({OTHER}, {ZED}, 'owner', now() - interval '9 days'),
               ({OTHER}, {OLU}, 'instructor', now() - interval '8 days');
             INSERT INTO identity.staff_invites
               (tenant_id, id, token_hash, staff_id, role_name, issued_at, ends_at) VALUES
               ({BUSINESS}, '01900000-0000-7000-8000-000000000001', decode(repeat('01', 32), 'hex'), 6,
                'front_desk', now() - interval '1 hour', now() + interval '6 days'),
               ({BUSINESS}, '01900000-0000-7000-8000-000000000002', decode(repeat('02', 32), 'hex'), 7,
                'manager', now() - interval '8 days', now() - interval '1 day'),
               ({OTHER}, '01900000-0000-7000-8000-000000000003', decode(repeat('03', 32), 'hex'), 7,
                'instructor', now(), now() + interval '7 days');
             SELECT setval('identity.staff_id_seq', 7);"
        ),
    )
    .await;
    (crate::postgres_staff_store(app_pool(&database)), database)
}

/// Pip's open invite at the business.
pub(crate) const PIP_INVITE: &str = "01900000-0000-7000-8000-000000000001";

/// A member of the business, as [`Accounts::member_at`] would read them.
fn member(staff: i64, role: Role) -> Member {
    Member {
        staff: AccountId::new(staff),
        business: BusinessId::new(BUSINESS),
        role,
    }
}

/// A live staff session for `staff`, as a request would carry.
fn session_of(staff: i64) -> Session {
    let now = SystemTime::now();
    Session {
        hash: [7; 32],
        realm: Realm::Staff,
        admin: AccountId::new(staff),
        started_at: now,
        proven_at: Some(now),
        last_seen_at: now,
        ends_at: now,
    }
}

/// Each staff member's role at the business, by id.
async fn roles(database: &TestDatabase) -> Vec<(i64, String)> {
    let sql = format!(
        "SELECT staff_id, role_name FROM identity.staff_roles WHERE tenant_id = {BUSINESS} ORDER BY staff_id"
    );
    let client = connect(database).await.expect("connects");
    let rows = client.query(&sql, &[]).await.expect("reads");
    rows.iter()
        .map(|row| {
            (
                row.try_get(0).expect("an id"),
                row.try_get(1).expect("a role"),
            )
        })
        .collect()
}

/// Each event of a kind, with the business whose record holds it, oldest first, from both records.
async fn recorded(database: &TestDatabase, kind: &str) -> Vec<(Option<i64>, Value)> {
    let client = connect(database).await.expect("connects");
    let mut found = Vec::new();
    for (outbox, tenant) in [("outbox", "tenant_id"), ("outbox_global", "NULL::bigint")] {
        let sql = format!(
            "SELECT {tenant}, payload::text FROM identity.{outbox} WHERE event_type = $1 ORDER BY occurred_at, id"
        );
        for row in client.query(&sql, &[&kind]).await.expect("reads") {
            let payload: &str = row.try_get(1).expect("a payload");
            let business = row.try_get(0).expect("a business");
            found.push((business, serde_json::from_str(payload).expect("JSON")));
        }
    }
    found
}

/// Each event of a kind's outcome and reason, oldest first.
async fn reasons(database: &TestDatabase, kind: &str) -> Vec<(Value, Value)> {
    let events = recorded(database, kind).await;
    events
        .into_iter()
        .map(|(_, payload)| (payload["outcome"].clone(), payload["reason"].clone()))
        .collect()
}

/// A change's event payload, as the business's own record holds it, by the test's actor.
fn said(mut payload: Value, outcome: &str) -> (Option<i64>, Value) {
    payload["actor"] = json!("admin-1@browser");
    payload["outcome"] = json!(outcome);
    (Some(BUSINESS), payload)
}

/// The table the role checks rest on, as each refused change must leave it.
fn the_team() -> Vec<(i64, String)> {
    [
        (OLU, "owner"),
        (MAE, "manager"),
        (FEN, "front_desk"),
        (IKE, "instructor"),
    ]
    .map(|(id, role)| (id, role.to_string()))
    .to_vec()
}

/// What changing `who`'s role from `held` to `wanted` came to.
async fn change(
    staff: &Accounts<impl TeamStore>,
    by: Member,
    who: i64,
    held: Role,
    wanted: Role,
) -> TeamChange {
    let change = moving(who, held, wanted);
    let changed = staff.change_role(&by, change, &actor()).await;
    changed.expect("answers")
}

/// Moving `who` from `held` to `wanted`.
fn moving(who: i64, held: Role, wanted: Role) -> RoleChange {
    RoleChange {
        staff: AccountId::new(who),
        held,
        wanted,
    }
}

/// Kai Dunn, at `email`, to invite as an instructor.
fn kai_at(email: &str) -> TeamInvite {
    TeamInvite {
        name: Some(PersonName::parse("Kai Dunn").expect("a name")),
        email: Address::parse(email).expect("an address"),
        role: Role::Instructor,
    }
}

/// What removing `who`, who holds `held`, came to.
async fn remove(staff: &Accounts<impl TeamStore>, by: Member, who: i64, held: Role) -> TeamChange {
    let removed = staff
        .remove_staff(&by, AccountId::new(who), held, &actor())
        .await;
    removed.expect("answers")
}

/// What cancelling the invite `id`, which gives `held`, came to.
async fn cancel(staff: &Accounts<impl TeamStore>, by: Member, id: &str, held: Role) -> TeamChange {
    let id = InviteId::parse(id).expect("an id");
    let cancelled = staff.cancel_invite(&by, &id, held, &actor()).await;
    cancelled.expect("answers")
}

/// A session's staff member acts at a business with the role they hold there, and at another with that one's,
/// and at one where they hold none, not at all. Their first business is the one they joined first.
#[tokio::test]
async fn a_member_acts_with_the_role_their_business_gives() {
    let (staff, _database) = team().await;
    let olu = session_of(OLU);
    let at = |business| staff.member_at(&olu, BusinessId::new(business));
    assert_eq!(at(BUSINESS).await, Ok(Some(member(OLU, Role::Owner))));
    let elsewhere = at(OTHER).await.expect("reads").expect("a member");
    assert_eq!(
        (elsewhere.business(), elsewhere.role()),
        (BusinessId::new(OTHER), Role::Instructor)
    );
    assert_eq!(at(9).await, Ok(None));
    assert_eq!(
        staff
            .member_at(&session_of(ZED), BusinessId::new(BUSINESS))
            .await,
        Ok(None)
    );
    assert_eq!(
        staff.first_business(&olu).await,
        Ok(Some(BusinessId::new(OTHER)))
    );
    assert_eq!(
        staff.first_business(&session_of(MAE)).await,
        Ok(Some(BusinessId::new(BUSINESS)))
    );
    assert_eq!(staff.first_business(&session_of(6)).await, Ok(None));
}

/// The Team page lists the business's own staff, the longest here first, and its open invites, never another
/// business's or an ended one; only those who may see the team see it.
#[tokio::test]
async fn the_team_shows_its_own_staff_and_open_invites_to_those_who_may_see_it() {
    let (staff, _database) = team().await;
    let shown = staff
        .team(&member(MAE, Role::Manager))
        .await
        .expect("reads")
        .expect("a manager sees the team");
    let members: Vec<_> = shown
        .members
        .iter()
        .map(|m| (m.staff.get(), m.name.as_str(), m.email.as_str(), m.role))
        .collect();
    assert_eq!(
        members,
        [
            (OLU, "Olu Bello", "olu@example.com", Role::Owner),
            (MAE, "Mae Lin", "mae@example.com", Role::Manager),
            (FEN, "Fen Ito", "fen@example.com", Role::FrontDesk),
            (IKE, "Ike Obi", "ike@example.com", Role::Instructor),
        ]
    );
    assert!(shown.members[0].joined_at < shown.members[1].joined_at);
    let invites: Vec<_> = shown
        .invites
        .iter()
        .map(|i| (i.id.as_str(), i.staff.get(), i.email.as_str(), i.role))
        .collect();
    assert_eq!(
        invites,
        [(PIP_INVITE, 6, "pip@example.com", Role::FrontDesk)]
    );
    assert!(shown.invites[0].ends_at > SystemTime::now());
    for role in [Role::FrontDesk, Role::Instructor] {
        assert_eq!(staff.team(&member(FEN, role)).await, Ok(None), "{role:?}");
    }
}

/// A manager changes no owner's or manager's role, and no one promotes past their reach, changes their own, or
/// changes any without the permission: nothing changes, and nothing is recorded.
#[tokio::test]
async fn roles_change_only_within_reach() {
    let (staff, database) = team().await;
    let mae = member(MAE, Role::Manager);
    for (who, held, wanted) in [
        (OLU, Role::Owner, Role::Instructor),
        (MAE, Role::Manager, Role::Instructor),
        (FEN, Role::FrontDesk, Role::Manager),
        (IKE, Role::Instructor, Role::Owner),
    ] {
        let refused = change(&staff, mae, who, held, wanted).await;
        assert_eq!(refused, TeamChange::Refused, "{who}");
    }
    let olu = member(OLU, Role::Owner);
    let own = change(&staff, olu, OLU, Role::Owner, Role::Manager).await;
    assert_eq!(own, TeamChange::Refused);
    for role in [Role::FrontDesk, Role::Instructor] {
        let them = member(FEN, role);
        let refused = change(&staff, them, IKE, Role::Instructor, Role::FrontDesk).await;
        assert_eq!(refused, TeamChange::Refused, "{role:?}");
    }
    assert_eq!(roles(&database).await, the_team());
    assert_eq!(recorded(&database, "identity.staff_role_changed").await, []);
}

/// A manager moves front desk and instructors between those roles, an owner makes another owner, and a role held
/// already is unchanged; each is recorded in the business's own record, naming the staff member and both roles.
#[tokio::test]
async fn roles_change_and_are_recorded() {
    let (staff, database) = team().await;
    let (mae, olu) = (member(MAE, Role::Manager), member(OLU, Role::Owner));
    let moved = change(&staff, mae, FEN, Role::FrontDesk, Role::Instructor).await;
    assert_eq!(moved, TeamChange::Done);
    let promoted = change(&staff, olu, MAE, Role::Manager, Role::Owner).await;
    assert_eq!(promoted, TeamChange::Done);
    let kept = change(&staff, olu, IKE, Role::Instructor, Role::Instructor).await;
    assert_eq!(kept, TeamChange::Unchanged);
    let now = [
        (OLU, "owner"),
        (MAE, "owner"),
        (FEN, "instructor"),
        (IKE, "instructor"),
    ];
    assert_eq!(
        roles(&database).await,
        now.map(|(id, role)| (id, role.to_string()))
    );
    let changed = |id, from, to| json!({"staff_id": id, "from": from, "to": to, "reason": null});
    assert_eq!(
        recorded(&database, "identity.staff_role_changed").await,
        [
            said(changed(FEN, "front_desk", "instructor"), "changed"),
            said(changed(MAE, "manager", "owner"), "changed"),
            said(changed(IKE, "instructor", "instructor"), "unchanged"),
        ]
    );
}

/// A change decided on a role that has since changed, the member's own or the other person's, changes nothing,
/// and says so: once one owner demotes another, the other no longer acts as an owner.
#[tokio::test]
async fn a_change_on_a_role_that_changed_meanwhile_is_stale() {
    let (staff, database) = team().await;
    let olu = member(OLU, Role::Owner);
    let wrong = change(&staff, olu, IKE, Role::FrontDesk, Role::Manager).await;
    assert_eq!(wrong, TeamChange::Stale);
    let promote =
        format!("UPDATE identity.staff_roles SET role_name = 'owner' WHERE staff_id = {MAE}");
    as_db_admin(&database, &promote).await;
    let demoted = change(&staff, olu, MAE, Role::Owner, Role::Manager).await;
    assert_eq!(demoted, TeamChange::Done);
    let mae_as_owner = member(MAE, Role::Owner);
    let late = change(&staff, mae_as_owner, OLU, Role::Owner, Role::Manager).await;
    assert_eq!(late, TeamChange::Stale);
    let late = remove(&staff, mae_as_owner, OLU, Role::Owner).await;
    assert_eq!(late, TeamChange::Stale);
    let stale = (json!("refused"), json!("stale"));
    assert_eq!(
        reasons(&database, "identity.staff_role_changed").await,
        [stale.clone(), (json!("changed"), json!(null)), stale]
    );
    assert_eq!(roles(&database).await, the_team());
}

/// The business's last owner is neither demoted nor removed, even where the use cases' checks are passed by, and
/// once another owner joins, may be.
#[tokio::test]
async fn the_last_owner_stays() {
    let (staff, database) = team().await;
    let olu = member(OLU, Role::Owner);
    let store = &crate::adapters::postgres::PostgresStaff::new(app_pool(&database));
    let me = AccountId::new(OLU);
    let demoted = store
        .change_role(&olu, moving(OLU, Role::Owner, Role::Manager), &actor())
        .await;
    assert_eq!(demoted, Ok(TeamChange::LastOwner));
    let removed = store.remove_staff(&olu, me, Role::Owner, &actor()).await;
    assert_eq!(removed, Ok(TeamChange::LastOwner));
    assert_eq!(roles(&database).await, the_team());
    let last = (json!("refused"), json!("last_owner"));
    assert_eq!(reasons(&database, "identity.staff_removed").await, [last]);
    change(&staff, olu, MAE, Role::Manager, Role::Owner).await;
    let demoted = store
        .change_role(&olu, moving(OLU, Role::Owner, Role::Manager), &actor())
        .await;
    assert_eq!(demoted, Ok(TeamChange::Done));
}

/// A manager removes no owner or manager, no one removes themselves, and front desk removes no one.
#[tokio::test]
async fn staff_are_removed_only_within_reach() {
    let (staff, database) = team().await;
    let mae = member(MAE, Role::Manager);
    assert_eq!(
        remove(&staff, mae, OLU, Role::Owner).await,
        TeamChange::Refused
    );
    assert_eq!(
        remove(&staff, mae, MAE, Role::Manager).await,
        TeamChange::Refused
    );
    let olu = member(OLU, Role::Owner);
    assert_eq!(
        remove(&staff, olu, OLU, Role::Owner).await,
        TeamChange::Refused
    );
    let fen = member(FEN, Role::FrontDesk);
    assert_eq!(
        remove(&staff, fen, IKE, Role::Instructor).await,
        TeamChange::Refused
    );
    assert_eq!(roles(&database).await, the_team());
    assert_eq!(recorded(&database, "identity.staff_removed").await, []);
}

/// Removing someone takes their role and any invite of theirs at the business, so they no longer act there, and
/// is recorded in its own record; one decided on a role they no longer hold is stale.
#[tokio::test]
async fn removing_someone_ends_their_role_and_invite() {
    let (staff, database) = team().await;
    let mae = member(MAE, Role::Manager);
    assert_eq!(
        remove(&staff, mae, IKE, Role::FrontDesk).await,
        TeamChange::Stale
    );
    as_db_admin(
        &database,
        &format!(
            "INSERT INTO identity.staff_invites (tenant_id, token_hash, staff_id, role_name, issued_at, ends_at)
             VALUES ({BUSINESS}, decode(repeat('04', 32), 'hex'), {IKE}, 'manager', now(), now() + interval '1 day')"
        ),
    )
    .await;
    assert_eq!(
        remove(&staff, mae, IKE, Role::Instructor).await,
        TeamChange::Done
    );
    let left: Vec<_> = roles(&database)
        .await
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(left, [OLU, MAE, FEN]);
    let invites = format!("SELECT count(*) FROM identity.staff_invites WHERE staff_id = {IKE}");
    assert_eq!(number(&database, &invites).await, 0);
    let ike = staff
        .member_at(&session_of(IKE), BusinessId::new(BUSINESS))
        .await;
    assert_eq!(ike, Ok(None));
    let removed = |role, reason| json!({"staff_id": IKE, "role": role, "reason": reason});
    assert_eq!(
        recorded(&database, "identity.staff_removed").await,
        [
            said(removed("front_desk", json!("stale")), "refused"),
            said(removed("instructor", json!(null)), "changed"),
        ]
    );
}

/// An open invite is cancelled by its id from the role it gives, only by those who reach that role; one that ended,
/// or gives another role now, or is another business's, is stale.
#[tokio::test]
async fn invites_are_cancelled_only_within_reach() {
    let (staff, database) = team().await;
    let (mae, olu) = (member(MAE, Role::Manager), member(OLU, Role::Owner));
    let ended = "01900000-0000-7000-8000-000000000002";
    let elsewhere = "01900000-0000-7000-8000-000000000003";
    assert_eq!(
        cancel(&staff, mae, ended, Role::Manager).await,
        TeamChange::Refused
    );
    let fen = member(FEN, Role::FrontDesk);
    assert_eq!(
        cancel(&staff, fen, PIP_INVITE, Role::FrontDesk).await,
        TeamChange::Refused
    );
    for (id, held) in [
        (PIP_INVITE, Role::Instructor),
        (elsewhere, Role::Instructor),
        (ended, Role::Manager),
    ] {
        assert_eq!(
            cancel(&staff, olu, id, held).await,
            TeamChange::Stale,
            "{id}"
        );
    }
    assert_eq!(
        cancel(&staff, mae, PIP_INVITE, Role::FrontDesk).await,
        TeamChange::Done
    );
    let left = "SELECT count(*) FROM identity.staff_invites";
    assert_eq!(number(&database, left).await, 2);
    let stale = (json!("refused"), json!("stale"));
    assert_eq!(
        reasons(&database, "identity.staff_invite_cancelled").await,
        [
            stale.clone(),
            stale.clone(),
            stale,
            (json!("changed"), json!(null))
        ]
    );
}

/// Inviting from the Team page.
mod inviting {
    use super::*;

    /// What inviting `email` as `role`, under `name`, came to, with whether a link came back.
    async fn invite(
        staff: &Accounts<impl TeamStore>,
        by: Member,
        who: (&str, &str),
        role: Role,
    ) -> (TeamChange, Option<crate::InviteToken>) {
        let invite = TeamInvite {
            name: Some(PersonName::parse(who.0).expect("a name")),
            email: Address::parse(who.1).expect("an address"),
            role,
        };
        let invited = staff.invite_from_team(&by, invite, &actor()).await;
        let invited = invited.expect("answers");
        (invited.change, invited.token)
    }

    const KAI: (&str, &str) = ("Kai Dunn", "kai@example.com");
    const KAIS: &str = "SELECT count(*) FROM identity.staff WHERE email = 'kai@example.com'";

    /// A manager invites no owner or manager, and front desk invites no one: nothing is made or recorded.
    #[tokio::test]
    async fn an_invite_is_sent_only_within_reach() {
        let (staff, database) = team().await;
        let mae = member(MAE, Role::Manager);
        for (by, role) in [
            (mae, Role::Manager),
            (mae, Role::Owner),
            (member(FEN, Role::FrontDesk), Role::Instructor),
        ] {
            let (change, token) = invite(&staff, by, KAI, role).await;
            assert_eq!(
                (change, token.is_none()),
                (TeamChange::Refused, true),
                "{role:?}"
            );
        }
        assert_eq!(number(&database, KAIS).await, 0);
        assert_eq!(recorded(&database, "identity.staff_invited").await, []);
    }

    /// An invite within reach brings back the link to email, the invite is the business's, and its own record names
    /// the staff member and role.
    #[tokio::test]
    async fn an_invite_within_reach_is_the_businesss_own() {
        let (staff, database) = team().await;
        let (change, token) =
            invite(&staff, member(MAE, Role::Manager), KAI, Role::Instructor).await;
        assert_eq!(change, TeamChange::Done);
        let link = token.expect("a link to email").to_text();
        let invitation = staff.invitation(&link).await.expect("reads").expect("open");
        assert_eq!(
            (invitation.business, invitation.role),
            (BusinessId::new(BUSINESS), Role::Instructor)
        );
        let kai = number(
            &database,
            "SELECT id FROM identity.staff WHERE email = 'kai@example.com'",
        )
        .await;
        assert_eq!(
            recorded(&database, "identity.staff_invited").await,
            [said(
                json!({"staff_id": kai, "role": "instructor", "reason": null}),
                "changed"
            )]
        );
    }

    /// A member is told they are one; a disabled staff member is answered as sent but issued no link, and the
    /// business's record names no one, so it learns nothing another business gave them.
    #[tokio::test]
    async fn an_invite_says_nothing_another_business_knows() {
        let (staff, database) = team().await;
        let olu = member(OLU, Role::Owner);
        let fen = ("Fen Ito", "FEN@example.com");
        let (change, token) = invite(&staff, olu, fen, Role::Instructor).await;
        assert_eq!((change, token.is_none()), (TeamChange::Member, true));
        let disable = format!("UPDATE identity.staff SET disabled_at = now() WHERE id = {ZED}");
        as_db_admin(&database, &disable).await;
        let (change, token) =
            invite(&staff, olu, ("Zed Ray", "zed@example.com"), Role::Manager).await;
        assert_eq!((change, token.is_none()), (TeamChange::Done, true));
        // The Team page lists Zed's invite as it would anyone's, though no link went out.
        let listed = staff
            .team(&olu)
            .await
            .expect("reads")
            .expect("shown")
            .invites;
        let emails: Vec<_> = listed.iter().map(|i| (i.email.as_str(), i.role)).collect();
        assert_eq!(
            emails,
            [
                ("pip@example.com", Role::FrontDesk),
                ("zed@example.com", Role::Manager)
            ]
        );
        let invited =
            |id: Value, role, reason| json!({"staff_id": id, "role": role, "reason": reason});
        assert_eq!(
            recorded(&database, "identity.staff_invited").await,
            [
                said(invited(json!(FEN), "instructor", "member"), "refused"),
                said(invited(json!(null), "manager", "not_issued"), "refused"),
            ]
        );
    }

    /// A resend names no one, and invites whoever the open invite names, under the name they hold; with no open
    /// invite to resend, it is stale, and makes no one.
    #[tokio::test]
    async fn a_resend_names_no_one() {
        let (staff, database) = team().await;
        let mae = member(MAE, Role::Manager);
        let resend = |email: &str| TeamInvite {
            name: None,
            email: Address::parse(email).expect("an address"),
            role: Role::FrontDesk,
        };
        let sent = staff
            .invite_from_team(&mae, resend("pip@example.com"), &actor())
            .await;
        let sent = sent.expect("answers");
        assert_eq!(
            (sent.change, sent.token.is_some()),
            (TeamChange::Done, true)
        );
        let pips = "SELECT count(*) FROM identity.staff WHERE name = 'Pip Moss'";
        assert_eq!(number(&database, pips).await, 1);
        let stale = staff
            .invite_from_team(&mae, resend("kai@example.com"), &actor())
            .await;
        let stale = stale.expect("answers");
        assert_eq!(
            (stale.change, stale.token.is_none()),
            (TeamChange::Stale, true)
        );
        assert_eq!(number(&database, KAIS).await, 0);
    }

    /// A disabled staff member who holds a role at the business already is a member, as anyone would be.
    #[tokio::test]
    async fn a_disabled_member_is_a_member() {
        let (staff, database) = team().await;
        let disable = format!("UPDATE identity.staff SET disabled_at = now() WHERE id = {FEN}");
        as_db_admin(&database, &disable).await;
        let fen = ("Fen Ito", "fen@example.com");
        let (change, token) = invite(&staff, member(OLU, Role::Owner), fen, Role::Instructor).await;
        assert_eq!((change, token.is_none()), (TeamChange::Member, true));
        let fens = format!("SELECT count(*) FROM identity.staff_invites WHERE staff_id = {FEN}");
        assert_eq!(number(&database, &fens).await, 0);
    }

    /// An open invite to a role the inviter does not reach is never replaced, and one decided on a role the inviter
    /// no longer holds sends nothing.
    #[tokio::test]
    async fn an_invite_never_replaces_one_above_the_inviter() {
        let (staff, database) = team().await;
        let raise = format!(
            "UPDATE identity.staff_invites SET role_name = 'owner' WHERE tenant_id = {BUSINESS} AND staff_id = 6"
        );
        as_db_admin(&database, &raise).await;
        let mae = member(MAE, Role::Manager);
        let pip = ("Pip Moss", "pip@example.com");
        let (change, token) = invite(&staff, mae, pip, Role::FrontDesk).await;
        assert_eq!((change, token.is_none()), (TeamChange::Refused, true));
        let owners = "SELECT count(*) FROM identity.staff_invites WHERE staff_id = 6 AND role_name = 'owner'";
        assert_eq!(number(&database, owners).await, 1);
        let demote = format!(
            "UPDATE identity.staff_roles SET role_name = 'instructor' WHERE staff_id = {MAE}"
        );
        as_db_admin(&database, &demote).await;
        let (change, token) = invite(&staff, mae, KAI, Role::Instructor).await;
        assert_eq!((change, token.is_none()), (TeamChange::Stale, true));
        assert_eq!(number(&database, KAIS).await, 0);
        assert_eq!(
            reasons(&database, "identity.staff_invited").await,
            [
                (json!("refused"), json!("beyond")),
                (json!("refused"), json!("stale"))
            ]
        );
    }

    /// Each staff member sends twenty invites an hour, and the twenty-first waits, while another's room is untouched.
    #[tokio::test]
    async fn each_staff_member_sends_twenty_invites_an_hour() {
        let (staff, _database) = team().await;
        let send = |by: Member, n: usize| {
            let staff = &staff;
            async move {
                let invite = kai_at(&format!("kai{n}@example.com"));
                staff.invite_from_team(&by, invite, &actor()).await
            }
        };
        let mae = member(MAE, Role::Manager);
        for n in 0..20 {
            assert_eq!(
                send(mae, n).await.expect("sent").change,
                TeamChange::Done,
                "{n}"
            );
        }
        let Err(Error::Limited(wait)) = send(mae, 20).await else {
            panic!("the twenty-first waits");
        };
        assert!(wait.as_secs() > 0 && wait.as_secs() <= 180, "{wait:?}");
        let olu = member(OLU, Role::Owner);
        assert_eq!(send(olu, 21).await.expect("sent").change, TeamChange::Done);
    }
}

/// Each read and change of the team fails as storage does, and an invite fails without a secure random source.
#[tokio::test]
async fn the_team_fails_as_its_storage_does() {
    let olu = member(OLU, Role::Owner);
    for broken in [
        "REVOKE SELECT ON identity.staff_roles FROM tetsu_identity",
        "REVOKE EXECUTE ON FUNCTION identity.staff_first_business(bigint) FROM tetsu_identity",
        "REVOKE EXECUTE ON FUNCTION identity.staff_team() FROM tetsu_identity",
        "REVOKE EXECUTE ON FUNCTION identity.staff_open_invites() FROM tetsu_identity",
        "REVOKE EXECUTE ON FUNCTION identity.admit(text, bytea) FROM tetsu_identity",
    ] {
        let (staff, database) = team().await;
        as_db_admin(&database, broken).await;
        let at = staff
            .member_at(&session_of(OLU), BusinessId::new(BUSINESS))
            .await;
        let first = staff.first_business(&session_of(OLU)).await;
        let shown = staff.team(&olu).await;
        let invite = kai_at("kai@example.com");
        let invited = staff.invite_from_team(&olu, invite, &actor()).await;
        let failed = [
            at.is_err(),
            first.is_err(),
            shown.is_err(),
            invited.is_err(),
        ];
        let expected = match broken {
            b if b.contains("staff_roles") => [true, false, true, true],
            b if b.contains("first_business") => [false, true, false, false],
            b if b.contains("admit") => [false, false, false, true],
            _ => [false, false, true, false],
        };
        assert_eq!(failed, expected, "{broken}");
    }
    let (_, database) = team().await;
    let failing = Accounts::with_fill(
        crate::adapters::postgres::PostgresStaff::new(app_pool(&database)),
        |_| Err(getrandom::Error::UNSUPPORTED),
    );
    let invite = kai_at("kai@example.com");
    let invited = failing.invite_from_team(&olu, invite, &actor()).await;
    assert!(matches!(invited, Err(Error::Unavailable(_))));
}
