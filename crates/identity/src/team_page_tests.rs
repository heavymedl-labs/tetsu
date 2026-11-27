//! A business's Team page over HTTP (ADR-0096), against a real database: who sees it, what it shows, and what each
//! of its forms does, with a recorder in place of Mail.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use tetsu_postgres::test_support::TestDatabase;

use crate::Realm;
use crate::http_tests::{Posted, Reply, form_on, get, send, serve_staff_posting};
use crate::join_tests::{ORIGIN, as_db_admin, number};
use crate::team_tests::{BUSINESS, FEN, IKE, MAE, OLU, PIP_INVITE, team};

const TEAM: &str = "/fern-studio/settings/team";
const FIREFOX: &str = "Mozilla/5.0 (X11; Linux x86_64) Firefox/140.0";

/// The team's pages served, with what they email recorded, over the team's database.
async fn serving() -> (SocketAddr, TestDatabase, Arc<Posted>) {
    let (staff, database) = team().await;
    let posted = Arc::new(Posted::default());
    (
        serve_staff_posting(staff, posted.clone()).await,
        database,
        posted,
    )
}

/// A logged-in staff session for `staff`, started as a login would, and the cookie its browser sends back.
async fn cookie_for(database: &TestDatabase, staff: i64) -> String {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).expect("random bytes");
    let token = crate::adapters::postgres::hex(&bytes);
    as_db_admin(
        database,
        &format!("SELECT identity.staff_start_session(sha256(decode('{token}', 'hex')), 'staff', {staff}, true)"),
    )
    .await;
    format!("{}={token}", Realm::Staff.cookie_name())
}

/// A page's form, posted as a browser would from the site itself, with `fields` encoded as a form encodes them.
async fn submit(address: SocketAddr, path: &str, cookie: &str, fields: &[(&str, &str)]) -> Reply {
    let body = fields
        .iter()
        .map(|(name, value)| format!("{name}={}", value.replace('@', "%40").replace(' ', "+")))
        .collect::<Vec<_>>()
        .join("&");
    let headers = [
        ("Origin", ORIGIN),
        ("Sec-Fetch-Site", "same-origin"),
        ("Content-Type", "application/x-www-form-urlencoded"),
        ("Cookie", cookie),
        ("User-Agent", FIREFOX),
    ];
    send(address, "POST", path, &headers, &body).await
}

/// A logged-in staff member's cookie and the form token their Team page carries.
async fn on_team(address: SocketAddr, database: &TestDatabase, staff: i64) -> (String, String) {
    let cookie = cookie_for(database, staff).await;
    let shown = get(address, TEAM, &cookie).await;
    assert_eq!(shown.status, 200, "{}", shown.body);
    let form = form_on(&shown.body);
    (cookie, form)
}

/// Where a form's answer sends the browser.
fn location(reply: &Reply) -> (u16, Option<&str>) {
    (reply.status, reply.header("location"))
}

fn said(word: &str) -> String {
    format!("{TEAM}?said={word}")
}

/// Where the Team page's `act` form, posting the session's form token and `fields`, sends the browser.
async fn acting(
    address: SocketAddr,
    by: (&str, &str),
    act: &str,
    fields: &[(&str, &str)],
) -> (u16, Option<String>) {
    let mut all = vec![("form", by.1)];
    all.extend_from_slice(fields);
    let answer = submit(address, &format!("{TEAM}/{act}"), by.0, &all).await;
    (answer.status, answer.header("location").map(str::to_string))
}

/// The answer that returns to the Team page saying `word`.
fn back(word: &str) -> (u16, Option<String>) {
    (303, Some(said(word)))
}

/// An owner sees the business's staff and open invites, can invite to every role, and the sidebar names the
/// business with its Team page current; the page names their role there.
#[tokio::test]
async fn an_owner_sees_the_whole_team() {
    let (address, database, _) = serving().await;
    let cookie = cookie_for(&database, OLU).await;
    let shown = get(address, TEAM, &cookie).await;
    assert_eq!(shown.status, 200);
    let body = &shown.body;
    for text in [
        "<title>Team – Fern Studio – Tetsu</title>",
        "Olu Bello<span class=\"font-normal text-muted\"> (you)</span>",
        "mae@example.com · Joined 3 days ago",
        "pip@example.com</p>",
        "Front desk · Ends in 6 days",
        r#"<option value="owner">Owner</option>"#,
        r#"<option value="instructor" selected>Instructor</option>"#,
        r#"href="/fern-studio/settings/team/remove?staff=2""#,
        r#"<a href="/fern-studio/settings/team" aria-current="page""#,
        ">Fern Studio</p>",
        r#"<span class="block text-xs text-muted">Owner</span>"#,
    ] {
        assert!(body.contains(text), "{text} in {body}");
    }
    assert!(
        !body.contains("remove?staff=1\""),
        "no one removes themselves"
    );
    assert!(
        !body.contains(r#"id="tetsu-role-1""#),
        "no one changes their own role"
    );
    for other in ["2", "3", "4"] {
        assert!(
            body.contains(&format!(r#"id="tetsu-role-{other}""#)),
            "{other}"
        );
    }
    assert!(!body.contains(r#"role="status""#), "no notice");
}

/// A manager sees the team but may change, remove, and invite only front desk and instructors, and resend or cancel
/// only their invites.
#[tokio::test]
async fn a_manager_sees_only_what_they_reach() {
    let (address, database, _) = serving().await;
    let cookie = cookie_for(&database, MAE).await;
    let body = get(address, TEAM, &cookie).await.body;
    assert!(!body.contains(r#"<option value="manager""#), "{body}");
    assert!(body.contains(r#"<option value="front_desk">Front desk</option>"#));
    assert!(!body.contains("remove?staff=1\"") && !body.contains("remove?staff=2\""));
    assert!(body.contains("remove?staff=3\"") && body.contains("remove?staff=4\""));
    assert!(body.contains(&format!(r#"name="invite" value="{PIP_INVITE}""#)));
    assert!(
        !body.contains("Pip Moss"),
        "an invite shows no name another business may have given"
    );
    let raise = format!(
        "UPDATE identity.staff_invites SET role_name = 'manager' WHERE tenant_id = {BUSINESS}"
    );
    as_db_admin(&database, &raise).await;
    let body = get(address, TEAM, &cookie).await.body;
    assert!(!body.contains(r#"name="invite" value=""#), "{body}");
    assert!(body.contains("Manager · Ends in 6 days"));
}

/// Front desk, instructors, someone the business never had, and any slug no business has all get the page any
/// unknown path gets; someone logged out is sent to log in, returning here after.
#[tokio::test]
async fn the_team_is_missing_for_those_who_may_not_see_it() {
    let (address, database, _) = serving().await;
    for staff in [FEN, IKE, 6] {
        let cookie = cookie_for(&database, staff).await;
        let missing = get(address, TEAM, &cookie).await;
        assert_eq!(missing.status, 404, "{staff}");
        assert!(missing.body.contains("Page not found"));
    }
    let olu = cookie_for(&database, OLU).await;
    for path in [
        "/no-such-studio/settings/team",
        "/Fern-Studio/settings/team",
    ] {
        assert_eq!(get(address, path, &olu).await.status, 404, "{path}");
    }
    let away = get(address, TEAM, "").await;
    let next = "/login?next=/fern-studio/settings/team";
    assert_eq!(location(&away), (303, Some(next)));
    let down = get(address, "/down/settings/team", &olu).await;
    assert_eq!(down.status, 503);
    assert!(down.body.contains("Logging in is unavailable"));
    assert!(crate::http_tests::reported(
        "could not show a business's page"
    ));
}

/// A staff member's own pages name the business they joined first, with its Team page, while they may see its
/// team; not otherwise.
#[tokio::test]
async fn a_staff_members_own_pages_name_their_first_business() {
    let (address, database, _) = serving().await;
    let team_link = r#"href="/fern-studio/settings/team""#;
    let mae = cookie_for(&database, MAE).await;
    for page in [
        "/settings/profile",
        "/settings/security",
        "/settings/preferences",
    ] {
        let body = get(address, page, &mae).await.body;
        assert!(
            body.contains(team_link) && body.contains(">Fern Studio</p>"),
            "{page}"
        );
    }
    // Olu joined the other business first, where he teaches.
    for staff in [OLU, FEN, 6] {
        let cookie = cookie_for(&database, staff).await;
        let body = get(address, "/settings/profile", &cookie).await.body;
        assert!(!body.contains(team_link), "{staff}");
    }
    let unreadable = "INSERT INTO identity.staff_roles (tenant_id, staff_id, role_name, created_at)
                      VALUES (3, 6, 'owner', now() - interval '1 year')";
    as_db_admin(&database, unreadable).await;
    let pip = cookie_for(&database, 6).await;
    assert_eq!(get(address, "/settings/profile", &pip).await.status, 503);
}

/// An invite emails its link, to the realm's own join page, and the page says it was sent; one whose email fails
/// says so, and one that names no one, or names them badly, is invalid.
#[tokio::test]
async fn an_invite_emails_its_link() {
    let (address, database, posted) = serving().await;
    let (cookie, form) = on_team(address, &database, MAE).await;
    let mae = (cookie.as_str(), form.as_str());
    let kai = [
        ("name", "Kai Dunn"),
        ("email", "kai@example.com"),
        ("role", "instructor"),
    ];
    assert_eq!(acting(address, mae, "invite", &kai).await, back("sent"));
    let messages = posted.sent.lock().expect("records").clone();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].to().as_str(), "kai@example.com");
    let text = messages[0].text();
    assert!(
        text.contains("work at Fern Studio on Tetsu, as an instructor."),
        "{text}"
    );
    assert!(text.contains(&format!("{ORIGIN}/join#")), "{text}");
    let kais =
        "SELECT count(*) FROM identity.staff WHERE name = 'Kai Dunn' AND email = 'kai@example.com'";
    assert_eq!(number(&database, kais).await, 1);
    let shown = get(address, &said("sent"), &cookie).await;
    assert!(
        shown.body.contains("<p role=\"status\" class=\"mt-6"),
        "{}",
        shown.body
    );
    assert!(shown.body.contains(">Invite sent.</p>"));
    posted.failing.store(true, Ordering::SeqCst);
    let lea = [
        ("name", "Lea Moe"),
        ("email", "lea@example.com"),
        ("role", "front_desk"),
    ];
    assert_eq!(acting(address, mae, "invite", &lea).await, back("unsent"));
}

/// An invite that names no one, or names them badly, is invalid, and sends nothing.
#[tokio::test]
async fn an_invite_that_cannot_be_read_is_invalid() {
    let (address, database, posted) = serving().await;
    let (cookie, form) = on_team(address, &database, MAE).await;
    for (name, email, role) in [
        ("", "lea@example.com", "front_desk"),
        ("Lea Moe", "lea", "front_desk"),
        ("Lea Moe", "lea@example.com", "chef"),
    ] {
        let fields = [("name", name), ("email", email), ("role", role)];
        let answer = acting(address, (&cookie, &form), "invite", &fields).await;
        assert_eq!(answer, back("invalid"), "{name} {email} {role}");
    }
    assert_eq!(posted.sent.lock().expect("records").len(), 0);
}

/// An invite past the inviter's reach, to a member, or past their hour's room, says so, and sends nothing. An
/// invite to a member spends room, as any the inviter may send does.
#[tokio::test]
async fn an_invite_that_is_not_sent_says_why() {
    let (address, database, posted) = serving().await;
    let (cookie, form) = on_team(address, &database, MAE).await;
    let invite = |email: String, role: &'static str| {
        let (cookie, form) = (cookie.clone(), form.clone());
        async move {
            let fields = [
                ("name", "Kai Dunn"),
                ("email", email.as_str()),
                ("role", role),
            ];
            acting(address, (&cookie, &form), "invite", &fields).await
        }
    };
    assert_eq!(
        invite("kai@example.com".into(), "manager").await,
        back("refused")
    );
    assert_eq!(
        invite("fen@example.com".into(), "instructor").await,
        back("member")
    );
    for n in 0..20 {
        invite(format!("kai{n}@example.com"), "instructor").await;
    }
    assert_eq!(
        invite("kai20@example.com".into(), "instructor").await,
        back("limited")
    );
    assert_eq!(posted.sent.lock().expect("records").len(), 19);
}

/// Changing a role, removing someone after confirming, and cancelling an invite each return to the page, saying
/// what came of it.
#[tokio::test]
async fn the_teams_forms_change_it() {
    let (address, database, _) = serving().await;
    let (cookie, form) = on_team(address, &database, OLU).await;
    let olu = (cookie.as_str(), form.as_str());
    let promote = [("staff", "3"), ("held", "front_desk"), ("role", "manager")];
    assert_eq!(
        acting(address, olu, "role", &promote).await,
        back("changed")
    );
    let confirm = get(address, &format!("{TEAM}/remove?staff={IKE}"), &cookie).await;
    assert_eq!(confirm.status, 200);
    for text in [
        "<h1 class=\"text-2xl font-medium text-balance text-ink\">Remove Ike Obi?</h1>",
        "They lose access to Fern Studio at once.",
        r#"name="held" value="instructor""#,
    ] {
        assert!(confirm.body.contains(text), "{text} in {}", confirm.body);
    }
    let remove = [("staff", "4"), ("held", "instructor")];
    let confirmed = (cookie.as_str(), &*form_on(&confirm.body));
    assert_eq!(
        acting(address, confirmed, "remove", &remove).await,
        back("removed")
    );
    let cancel = [("invite", PIP_INVITE), ("held", "front_desk")];
    assert_eq!(
        acting(address, olu, "cancel", &cancel).await,
        back("cancelled")
    );
    let roles = format!(
        "SELECT string_agg(role_name, ',' ORDER BY staff_id) FROM identity.staff_roles WHERE tenant_id = {BUSINESS}"
    );
    let client = tetsu_postgres::connect(&database).await.expect("connects");
    let row = client.query_one(&roles, &[]).await.expect("reads");
    let held: String = row.try_get(0).expect("roles");
    assert_eq!(held, "owner,manager,manager");
    let pips = "SELECT count(*) FROM identity.staff_invites WHERE staff_id = 6";
    assert_eq!(number(&database, pips).await, 0);
}

/// Confirming the removal of oneself, of someone out of reach, gone, or never named, goes back to the page.
#[tokio::test]
async fn a_removal_out_of_reach_is_never_offered() {
    let (address, database, _) = serving().await;
    let cookie = cookie_for(&database, MAE).await;
    for staff in ["1", "2", "9", "x"] {
        let back = get(address, &format!("{TEAM}/remove?staff={staff}"), &cookie).await;
        assert_eq!(
            location(&back),
            (303, Some(said("stale").as_str())),
            "{staff}"
        );
    }
    let bare = get(address, &format!("{TEAM}/remove"), &cookie).await;
    assert_eq!(location(&bare), (303, Some(said("stale").as_str())));
    let away = get(address, &format!("{TEAM}/remove?staff=4"), "").await;
    assert_eq!(
        location(&away),
        (303, Some("/login?next=/fern-studio/settings/team"))
    );
    let fen = cookie_for(&database, FEN).await;
    assert_eq!(
        get(address, &format!("{TEAM}/remove?staff=4"), &fen)
            .await
            .status,
        404
    );
    let down = get(address, "/down/settings/team/remove?staff=4", &cookie).await;
    assert_eq!(down.status, 503);
}

/// A form missing what it needs, or holding what no form writes, changes nothing and says it was invalid.
#[tokio::test]
async fn a_form_that_cannot_be_read_changes_nothing() {
    let (address, database, _) = serving().await;
    let (cookie, form) = on_team(address, &database, MAE).await;
    let mae = (cookie.as_str(), form.as_str());
    for (act, fields) in [
        (
            "role",
            &[
                ("staff", "x"),
                ("held", "instructor"),
                ("role", "front_desk"),
            ][..],
        ),
        ("role", &[("staff", "4"), ("held", "instructor")]),
        ("remove", &[("staff", "4")]),
        ("role", &[("held", "instructor"), ("role", "front_desk")]),
        ("cancel", &[("invite", "not-an-id"), ("held", "front_desk")]),
        ("cancel", &[("invite", PIP_INVITE)]),
    ] {
        assert_eq!(
            acting(address, mae, act, fields).await,
            back("invalid"),
            "{act} {fields:?}"
        );
    }
    assert_eq!(
        number(&database, "SELECT count(*) FROM identity.staff_invites").await,
        3
    );
}

/// A form past the person's reach is refused, one on a role that changed meanwhile is stale, and one asking for the
/// role held already changes nothing; an answer's word no change writes says nothing on the page.
#[tokio::test]
async fn a_form_says_why_it_changed_nothing() {
    let (address, database, _) = serving().await;
    let (cookie, form) = on_team(address, &database, MAE).await;
    let mae = (cookie.as_str(), form.as_str());
    let owner = [("staff", "1"), ("held", "owner"), ("role", "manager")];
    assert_eq!(acting(address, mae, "role", &owner).await, back("refused"));
    let wrong = [
        ("staff", "4"),
        ("held", "front_desk"),
        ("role", "instructor"),
    ];
    assert_eq!(acting(address, mae, "role", &wrong).await, back("stale"));
    let same = [
        ("staff", "4"),
        ("held", "instructor"),
        ("role", "instructor"),
    ];
    assert_eq!(acting(address, mae, "role", &same).await, back("unchanged"));
    let shown = get(address, &said("nonsense"), &cookie).await;
    assert!(!shown.body.contains(r#"role="status""#));
}

/// A form from elsewhere, or without its session's form token, is refused, and changes nothing.
#[tokio::test]
async fn a_form_is_checked_as_every_change_is() {
    let (address, database, _) = serving().await;
    let (cookie, form) = on_team(address, &database, MAE).await;
    let path = format!("{TEAM}/cancel");
    let fields = format!("form={form}&invite={PIP_INVITE}&held=front_desk");
    let elsewhere = [
        ("Origin", "https://evil.example"),
        ("Cookie", cookie.as_str()),
        ("User-Agent", FIREFOX),
    ];
    assert_eq!(
        send(address, "POST", &path, &elsewhere, &fields)
            .await
            .status,
        403
    );
    let tokenless = [("invite", PIP_INVITE), ("held", "front_desk")];
    assert_eq!(
        submit(address, &path, &cookie, &tokenless).await.status,
        403
    );
    let pips = "SELECT count(*) FROM identity.staff_invites WHERE staff_id = 6";
    assert_eq!(number(&database, pips).await, 1);
}

/// A form for a business the person may not see the team of is missing, one whose business cannot be read is
/// unavailable, and one whose session ended goes to log in, returning to the page after.
#[tokio::test]
async fn a_form_reaches_only_a_team_the_person_sees() {
    let (address, database, _) = serving().await;
    let (cookie, form) = on_team(address, &database, MAE).await;
    let fen = cookie_for(&database, FEN).await;
    let fen_form = form_on(&get(address, "/settings/profile", &fen).await.body);
    let cancel = [("invite", PIP_INVITE), ("held", "front_desk")];
    let missing = acting(address, (&fen, &fen_form), "cancel", &cancel).await;
    assert_eq!(missing.0, 404);
    let mae = (cookie.as_str(), form.as_str());
    let down = submit(
        address,
        "/down/settings/team/cancel",
        &cookie,
        &[("form", &form)],
    )
    .await;
    assert_eq!(down.status, 503);
    as_db_admin(&database, "DELETE FROM identity.staff_sessions").await;
    let ended = acting(address, mae, "cancel", &cancel).await;
    let next = "/login?next=/fern-studio/settings/team".to_string();
    assert_eq!(ended, (303, Some(next)));
    let pips = "SELECT count(*) FROM identity.staff_invites WHERE staff_id = 6";
    assert_eq!(number(&database, pips).await, 1);
}

/// Taking a function from Identity's role, so a statement that calls it fails as storage does.
fn revoking(function: &str) -> String {
    format!("REVOKE EXECUTE ON FUNCTION identity.{function} FROM tetsu_identity")
}

const ROLES: &str = "REVOKE SELECT ON identity.staff_roles FROM tetsu_identity";
const CANCEL: [(&str, &str); 2] = [("invite", PIP_INVITE), ("held", "front_desk")];

/// An invite whose email cannot be written, as for a business whose name no email can hold, says it was not sent.
#[tokio::test]
async fn an_invite_whose_email_cannot_be_written_says_so() {
    let (address, database, posted) = serving().await;
    let (cookie, form) = on_team(address, &database, OLU).await;
    let owner =
        "INSERT INTO identity.staff_roles (tenant_id, staff_id, role_name) VALUES (6, 1, 'owner')";
    as_db_admin(&database, owner).await;
    let lea = format!("form={form}&name=Lea+Moe&email=lea%40example.com&role=instructor");
    let headers = [
        ("Origin", ORIGIN),
        ("Content-Type", "application/x-www-form-urlencoded"),
        ("Cookie", cookie.as_str()),
    ];
    let path = "/nul-studio/settings/team/invite";
    let unsent = send(address, "POST", path, &headers, &lea).await;
    let back_there = "/nul-studio/settings/team?said=unsent";
    assert_eq!(location(&unsent), (303, Some(back_there)));
    assert_eq!(posted.sent.lock().expect("records").len(), 0);
}

/// A form whose storage fails, at the session's check, the person's role, or the change itself, is unavailable.
#[tokio::test]
async fn a_form_whose_storage_fails_is_unavailable() {
    let invite = [
        ("name", "Kai Dunn"),
        ("email", "kai@example.com"),
        ("role", "instructor"),
    ];
    let change = [
        ("staff", "4"),
        ("held", "instructor"),
        ("role", "front_desk"),
    ];
    for (broken, act, fields) in [
        (
            revoking("staff_change_role(bigint, text, bigint, text, text)"),
            "role",
            &change[..],
        ),
        (
            revoking("staff_invite_from(bigint, text, text[], bytea, text, text, text)"),
            "invite",
            &invite,
        ),
        (
            revoking("staff_use_session(bytea, text)"),
            "cancel",
            &CANCEL,
        ),
        (ROLES.to_string(), "cancel", &CANCEL),
        (
            revoking("staff_remove(bigint, text, bigint, text)"),
            "remove",
            &[("staff", "4"), ("held", "instructor")],
        ),
        (
            revoking("staff_cancel_invite(bigint, text, text, text)"),
            "cancel",
            &CANCEL,
        ),
    ] {
        let (address, database, _) = serving().await;
        let (cookie, form) = on_team(address, &database, MAE).await;
        as_db_admin(&database, &broken).await;
        let (status, _) = acting(address, (&cookie, &form), act, fields).await;
        assert_eq!(status, 503, "{broken}");
    }
    assert!(crate::http_tests::reported(
        "could not change a business's team"
    ));
}

/// Each page fails as its storage does: reading the session, the person's role, their first business, and the team.
#[tokio::test]
async fn each_page_fails_as_its_storage_does() {
    let team = revoking("staff_team()");
    let remove_page = format!("{TEAM}/remove?staff={IKE}");
    for (broken, path) in [
        (revoking("staff_use_session(bytea, text)"), TEAM),
        (
            revoking("staff_first_business(bigint)"),
            "/settings/profile",
        ),
        (ROLES.to_string(), "/settings/profile"),
        (team.clone(), TEAM),
        (team, remove_page.as_str()),
    ] {
        let (address, database, _) = serving().await;
        let cookie = cookie_for(&database, MAE).await;
        as_db_admin(&database, &broken).await;
        let status = get(address, path, &cookie).await.status;
        assert_eq!(status, 503, "{broken} {path}");
    }
}

/// A staff member's own page whose account cannot be read is unavailable, and one disabled since logging in is
/// sent to log in.
#[tokio::test]
async fn a_staff_members_own_page_needs_their_account() {
    let (address, database, _) = serving().await;
    let cookie = cookie_for(&database, MAE).await;
    as_db_admin(&database, &revoking("staff_account(bytea)")).await;
    assert_eq!(get(address, "/settings/profile", &cookie).await.status, 503);
    let (address, database, _) = serving().await;
    let cookie = cookie_for(&database, MAE).await;
    let disable = format!("UPDATE identity.staff SET disabled_at = now() WHERE id = {MAE}");
    as_db_admin(&database, &disable).await;
    let away = get(address, "/settings/profile", &cookie).await;
    assert_eq!(away.status, 303);
}

/// Resending an invite posts no name, only the email and role its row holds, and sends a new link.
#[tokio::test]
async fn resending_an_invite_posts_no_name() {
    let (address, database, posted) = serving().await;
    let (cookie, form) = on_team(address, &database, MAE).await;
    let resend = [("email", "pip@example.com"), ("role", "front_desk")];
    assert_eq!(
        acting(address, (&cookie, &form), "invite", &resend).await,
        back("sent")
    );
    let sent = posted.sent.lock().expect("records");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to().as_str(), "pip@example.com");
}
