//! An admin's own sessions: their page lists them, this one first, and they end one other session, or every
//! other one, from a login within ten minutes (ADR-0059, `v5.0.0-7.5.2`).

use std::net::SocketAddr;

use tetsu_postgres::test_support::TestDatabase;

use crate::adapters::postgres::PostgresAdmins;
use crate::http_tests::{
    Reply, cookie_in, form_on, get, join, login_sending, sample_mailbox, send, send_bytes, serve,
    serving, text,
};
use crate::join_tests::app_pool;
use crate::join_tests::{ORIGIN, actor, as_db_admin, with_link};
use crate::ports::LoginStore;
use crate::{Actor, AdminName, Ending, Realm, SessionId};

const FIREFOX: &str = "Mozilla/5.0 (X11; Linux x86_64) Firefox/140.0";
/// The admin's Security page, which lists their sessions (#156).
const SECURITY: &str = "/admin/settings/security";
const LISTED: &str = "Started ";
const PRESENTED: &str = "This session";
const STALE: &str = "Log in again to log out other sessions.";

/// Kurt, joined, then logged in from `count` browsers: the server, the database, and each browser's cookie, oldest
/// first. Joining's own session ends with the first login, which presents it.
async fn logged_in_from(count: usize) -> (SocketAddr, TestDatabase, Vec<String>) {
    let (address, database, link) = serving().await;
    let (passkey, joined) = join(address, &link).await;
    let mut cookies = Vec::new();
    for browser in 0..count {
        let presented = if browser == 0 { joined.as_str() } else { "" };
        let reply = login_sending(address, passkey.clone(), presented, None).await;
        assert_eq!(reply.status, 200, "{}", reply.body);
        cookies.push(cookie_in(&reply));
    }
    (address, database, cookies)
}

/// Ends sessions from the page's form, as the browser posts it: one by its id, or every other one.
async fn end(address: SocketAddr, cookie: &str, form: &str, session: Option<&str>) -> Reply {
    let body = session.map_or_else(
        || format!("form={form}"),
        |id| format!("form={form}&session={id}"),
    );
    let headers = [
        ("Origin", ORIGIN),
        ("Cookie", cookie),
        ("User-Agent", FIREFOX),
    ];
    send(address, "POST", "/admin/sessions/end", &headers, &body).await
}

/// Each ending's record, oldest first: its outcome, how many ended, the session that asked, the one it named, its
/// reason, and its actor, with `-` for none.
async fn recorded(database: &TestDatabase) -> Vec<String> {
    let all = text(
        database,
        "SELECT coalesce(string_agg(concat_ws(' ', payload->>'outcome', coalesce(payload->>'ended', '-'),
             coalesce(payload->>'session', '-'), coalesce(payload->>'named', '-'),
             coalesce(payload->>'reason', '-'), payload->>'actor'), '|' ORDER BY occurred_at, id), '')
         FROM identity.outbox_global WHERE event_type = 'identity.other_sessions_ended'",
    )
    .await;
    all.split('|')
        .filter(|one| !one.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// The ids of the sessions a page offers to end, in its order.
fn offered(page: &str) -> Vec<String> {
    page.split("name=\"session\" value=\"")
        .skip(1)
        .map(|after| after.split('"').next().unwrap_or_default().to_string())
        .collect()
}

/// How many of the admin's sessions the database holds.
async fn held(database: &TestDatabase) -> String {
    text(
        database,
        "SELECT count(*)::text FROM identity.admin_sessions",
    )
    .await
}

/// The page lists every live session, this one first and marked, with the device each started on.
#[tokio::test]
async fn the_page_lists_every_session_this_one_first() {
    let (address, _database, cookies) = logged_in_from(2).await;
    let page = get(address, SECURITY, &cookies[1]).await;
    assert_eq!(page.status, 200);
    assert_eq!(page.body.matches(LISTED).count(), 2, "{}", page.body);
    assert_eq!(page.body.matches(PRESENTED).count(), 1);
    assert_eq!(page.body.matches("\">Firefox on Linux").count(), 2);
    // A screen reader tells the other session's log out from this one's by its device and start.
    assert_eq!(
        page.body
            .matches("<span class=\"sr-only\"> session 2, Firefox on Linux, started ")
            .count(),
        1
    );
    // This one comes first, and only the other is offered to end.
    let rows: Vec<&str> = page.body.split("<li class=\"flex").skip(1).collect();
    assert!(rows[0].contains(PRESENTED) && !rows[0].contains("name=\"session\""));
    assert!(!rows[1].contains(PRESENTED) && rows[1].contains("name=\"session\""));
    assert_eq!(offered(&page.body).len(), 1);
    assert!(page.body.contains("Log out of all other sessions"));
}

/// A session whose browser named no device is listed as an unknown one.
#[tokio::test]
async fn a_session_with_no_device_is_listed_as_unknown() {
    let (address, database, cookies) = logged_in_from(1).await;
    as_db_admin(
        &database,
        "UPDATE identity.admin_sessions SET device = NULL",
    )
    .await;
    let page = get(address, SECURITY, &cookies[0]).await;
    assert!(page.body.contains("Unknown device"), "{}", page.body);
    // Alone, there is nothing else to end.
    assert_eq!(offered(&page.body), Vec::<String>::new());
    assert!(!page.body.contains("Log out of all other sessions"));
}

/// Ending one other session logs it out, keeps this one, and records it with the admin and how many ended.
#[tokio::test]
async fn one_other_session_ends_and_this_one_stays() {
    let (address, database, cookies) = logged_in_from(3).await;
    let page = get(address, SECURITY, &cookies[2]).await;
    let ids = offered(&page.body);
    let ended = end(address, &cookies[2], &form_on(&page.body), Some(&ids[0])).await;
    assert_eq!(
        (ended.status, ended.header("location")),
        (303, Some(SECURITY))
    );
    assert_eq!(held(&database).await, "2");
    assert_eq!(get(address, SECURITY, &cookies[2]).await.status, 200);
    let after = get(address, SECURITY, &cookies[2]).await;
    assert_eq!(after.body.matches(LISTED).count(), 2);
    assert!(!offered(&after.body).contains(&ids[0]));
    // The record names the session that asked, the newest, and the one it ended, by their public ids.
    let asking = text(
        &database,
        "SELECT id::text FROM identity.admin_sessions ORDER BY started_at DESC LIMIT 1",
    )
    .await;
    assert_eq!(
        recorded(&database).await,
        [format!(
            "changed 1 {asking} {} - admin-1@Firefox on Linux",
            ids[0]
        )]
    );
}

/// Ending every other session logs them all out and keeps this one.
#[tokio::test]
async fn every_other_session_ends_and_this_one_stays() {
    let (address, database, cookies) = logged_in_from(3).await;
    let page = get(address, SECURITY, &cookies[1]).await;
    let ended = end(address, &cookies[1], &form_on(&page.body), None).await;
    assert_eq!(
        (ended.status, ended.header("location")),
        (303, Some(SECURITY))
    );
    assert_eq!(held(&database).await, "1");
    for gone in [&cookies[0], &cookies[2]] {
        assert_eq!(get(address, SECURITY, gone).await.status, 303);
    }
    assert_eq!(get(address, SECURITY, &cookies[1]).await.status, 200);
}

/// Only a login from the last ten minutes may end other sessions: a stale one is told so on the page, and ends
/// none, and its attempt is recorded as refused.
#[tokio::test]
async fn only_a_fresh_login_ends_other_sessions() {
    let (address, database, cookies) = logged_in_from(2).await;
    let page = get(address, SECURITY, &cookies[1]).await;
    let (form, ids) = (form_on(&page.body), offered(&page.body));
    as_db_admin(
        &database,
        "UPDATE identity.admin_sessions SET proven_at = proven_at - interval '11 minutes'",
    )
    .await;
    let stale = get(address, SECURITY, &cookies[1]).await;
    assert!(stale.body.contains(STALE), "{}", stale.body);
    assert_eq!(offered(&stale.body), Vec::<String>::new());
    for only in [Some(ids[0].as_str()), None] {
        let refused = end(address, &cookies[1], &form, only).await;
        assert_eq!(
            (refused.status, refused.header("location")),
            (303, Some(SECURITY))
        );
    }
    assert_eq!(held(&database).await, "2");
    let asking = text(
        &database,
        "SELECT id::text FROM identity.admin_sessions ORDER BY started_at DESC LIMIT 1",
    )
    .await;
    let stale = |named: &str| format!("refused - {asking} {named} stale admin-1@Firefox on Linux");
    assert_eq!(recorded(&database).await, [stale(&ids[0]), stale("-")]);
}

/// A session joining began rests on the invite link, not a passkey (ADR-0069), so it ends no other session.
#[tokio::test]
async fn a_session_joining_began_ends_no_other() {
    let (address, database, link) = serving().await;
    let (passkey, joined) = join(address, &link).await;
    let login = login_sending(address, passkey, "", None).await;
    let other = cookie_in(&login);
    let page = get(address, SECURITY, &joined).await;
    assert!(page.body.contains(STALE), "{}", page.body);
    let refused = end(address, &joined, &form_on(&page.body), None).await;
    assert_eq!(refused.status, 303);
    assert_eq!(get(address, SECURITY, &other).await.status, 200);
    assert_eq!(held(&database).await, "2");
}

/// Kurt and Ada, admins 1 and 2, each joined and then logged in once: the server, the database, and their
/// cookies.
async fn kurt_and_ada() -> (SocketAddr, TestDatabase, Vec<String>) {
    let (admins, database, _, kurt_link) = with_link().await;
    let name = AdminName::parse("Ada Lovelace").expect("a name");
    let ada = admins.create(&name, None, &actor()).await.expect("creates");
    let ada_link = admins.invite(ada, &actor()).await.expect("issues");
    let address = serve(admins, sample_mailbox()).await;
    let mut cookies = Vec::new();
    for link in [kurt_link, ada_link.to_text()] {
        let (passkey, joined) = join(address, &link).await;
        let reply = login_sending(address, passkey, &joined, None).await;
        cookies.push(cookie_in(&reply));
    }
    (address, database, cookies)
}

/// One admin's request can never list or end another admin's sessions, even naming one's id.
#[tokio::test]
async fn an_admin_never_lists_or_ends_anothers_sessions() {
    let (address, database, cookies) = kurt_and_ada().await;
    let (kurt, ada) = (&cookies[0], &cookies[1]);
    let kurts_id = text(
        &database,
        "SELECT id::text FROM identity.admin_sessions WHERE admin_id = 1",
    )
    .await;
    let page = get(address, SECURITY, ada).await;
    assert_eq!(page.body.matches(LISTED).count(), 1, "{}", page.body);
    assert!(!page.body.contains(&kurts_id));
    let tried = end(address, ada, &form_on(&page.body), Some(&kurts_id)).await;
    assert_eq!(tried.status, 303);
    let everything = end(address, ada, &form_on(&page.body), None).await;
    assert_eq!(everything.status, 303);
    assert_eq!(get(address, SECURITY, kurt).await.status, 200);
    assert_eq!(held(&database).await, "2");
    // Both are recorded as Ada's, ending nothing.
    let adas = text(
        &database,
        "SELECT id::text FROM identity.admin_sessions WHERE admin_id = 2",
    )
    .await;
    let unchanged = |named: &str| format!("unchanged 0 {adas} {named} - admin-2@Firefox on Linux");
    assert_eq!(
        recorded(&database).await,
        [unchanged(&kurts_id), unchanged("-")]
    );
}

/// A request to end sessions passes logging out's checks: one from elsewhere, without its form token, or naming no
/// id is refused and ends nothing, and one with no live session goes to log in. None records an ending; each is
/// recorded as a refused request instead (`refusal_tests`).
#[tokio::test]
async fn an_ending_from_elsewhere_or_unchecked_is_refused() {
    let (address, database, cookies) = logged_in_from(2).await;
    let page = get(address, SECURITY, &cookies[1]).await;
    let form = form_on(&page.body);
    let elsewhere = [
        ("Origin", "https://evil.example"),
        ("Cookie", cookies[1].as_str()),
    ];
    let from_elsewhere = send(
        address,
        "POST",
        "/admin/sessions/end",
        &elsewhere,
        &format!("form={form}"),
    )
    .await;
    assert_eq!(from_elsewhere.status, 403);
    for (cookie, form, only) in [
        (cookies[1].as_str(), "wrong", None),
        (cookies[1].as_str(), form.as_str(), Some("not-an-id")),
        (
            cookies[1].as_str(),
            form.as_str(),
            Some("ABCDEF01-2345-6789-ABCD-EF0123456789"),
        ),
    ] {
        assert_eq!(
            end(address, cookie, form, only).await.status,
            403,
            "{only:?}"
        );
    }
    assert_eq!(held(&database).await, "2");
    as_db_admin(&database, "DELETE FROM identity.admin_sessions").await;
    let ended = end(address, &cookies[1], &form, None).await;
    assert_eq!(
        (ended.status, ended.header("location")),
        (303, Some("/admin/login"))
    );
    // None of them reached the ending, so none records one.
    assert_eq!(recorded(&database).await, Vec::<String>::new());
}

/// A store that fails says the page is unavailable, rather than a list or an ending it could not make.
#[tokio::test]
async fn a_failing_store_is_unavailable() {
    let (address, database, cookies) = logged_in_from(2).await;
    let page = get(address, SECURITY, &cookies[1]).await;
    let form = form_on(&page.body);
    let revoke = |function: &str| {
        format!("REVOKE EXECUTE ON FUNCTION identity.{function} FROM tetsu_identity")
    };
    as_db_admin(&database, &revoke("end_other_sessions(bytea, uuid)")).await;
    assert_eq!(end(address, &cookies[1], &form, None).await.status, 503);
    as_db_admin(&database, &revoke("sessions_of(bytea)")).await;
    assert_eq!(get(address, SECURITY, &cookies[1]).await.status, 503);
}

/// A browser's actor names its device, and one that named none has none; a command's actor is never asked.
#[test]
fn an_actor_names_the_device_it_acted_from() {
    let device = crate::Device::from_user_agent(Some(FIREFOX));
    assert_eq!(Actor::browser(Realm::Admin, None, device).device(), device);
    assert_eq!(Actor::browser(Realm::Admin, None, None).device(), None);
    // An actor typed as text names no device: only a device read from a request is one.
    let typed = Actor::parse("admin-1@workstation").expect("an actor");
    assert_eq!(typed.device(), None);
    assert_eq!(
        Actor::from_login(Some("kurt"), Some("workstation")).device(),
        None
    );
}

/// A session id is a UUID in lowercase and nothing else.
#[test]
fn a_session_id_is_a_lowercase_uuid() {
    let valid = "0123abcd-4567-89ef-0123-456789abcdef";
    assert_eq!(
        SessionId::parse(valid).map(|id| id.as_str().to_string()),
        Some(valid.into())
    );
    for invalid in [
        "",
        "0123ABCD-4567-89ef-0123-456789abcdef",
        "0123abcd-4567-89ef-0123-456789abcde",
        "0123abcd-4567-89ef-0123456789abcdef0",
        "0123abcg-4567-89ef-0123-456789abcdef",
        "0123abcd 4567-89ef-0123-456789abcdef",
    ] {
        assert_eq!(SessionId::parse(invalid), None, "{invalid}");
    }
}

/// A list storage answers in the wrong shape, any column of it, is unavailable rather than misread.
#[tokio::test]
async fn an_unreadable_list_is_unavailable() {
    let columns = [
        ("id text", "'0123abcd-4567-89ef-0123-456789abcdef'::text"),
        ("started_at timestamptz", "now()"),
        ("last_seen_at timestamptz", "now()"),
        ("device text", "'x'::text"),
        ("presented boolean", "true"),
    ];
    for (shaped, row) in each_broken(&columns, ("integer", "1")) {
        let (address, database, cookies) = logged_in_from(1).await;
        as_db_admin(
            &database,
            &format!(
                "DROP FUNCTION identity.sessions_of(bytea);
                 CREATE FUNCTION identity.sessions_of(bytea) RETURNS TABLE ({shaped})
                 LANGUAGE sql AS $$ SELECT {row} $$;
                 GRANT EXECUTE ON FUNCTION identity.sessions_of(bytea) TO tetsu_identity"
            ),
        )
        .await;
        assert_eq!(
            get(address, SECURITY, &cookies[0]).await.status,
            503,
            "{shaped}"
        );
    }
}

/// A form that is not UTF-8 carries no token, so it is refused.
#[tokio::test]
async fn a_form_that_is_not_text_is_refused() {
    let (address, database, cookies) = logged_in_from(2).await;
    let headers = [("Origin", ORIGIN), ("Cookie", cookies[1].as_str())];
    let reply = send_bytes(
        address,
        "POST",
        "/admin/sessions/end",
        &headers,
        &[0xff, 0xfe],
    )
    .await;
    assert_eq!(reply.status, 403);
    assert_eq!(held(&database).await, "2");
}

/// Storage that fails while the session is checked leaves the ending unavailable.
#[tokio::test]
async fn a_session_unreadable_while_ending_is_unavailable() {
    let (address, database, cookies) = logged_in_from(2).await;
    let form = form_on(&get(address, SECURITY, &cookies[1]).await.body);
    as_db_admin(
        &database,
        "REVOKE EXECUTE ON FUNCTION identity.use_session(bytea, text) FROM tetsu_identity",
    )
    .await;
    assert_eq!(end(address, &cookies[1], &form, None).await.status, 503);
}

/// The newest session's token hash, as the store is given it.
async fn newest_hash(database: &TestDatabase) -> [u8; 32] {
    let hex = text(
        database,
        "SELECT encode(token_hash, 'hex') FROM identity.admin_sessions ORDER BY started_at DESC LIMIT 1",
    )
    .await;
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).expect("hex"))
        .collect();
    bytes.try_into().expect("32 bytes")
}

/// Storage refuses an ending from a session too stale, one that never was or was spent since the request was
/// checked, or one whose admin was disabled since, says which, and records each; the store answers a stale one
/// as not fresh and the others as no session.
#[tokio::test]
async fn storage_says_why_it_ends_nothing() {
    let (_address, database, _cookies) = logged_in_from(2).await;
    let store = PostgresAdmins {
        pool: app_pool(&database),
    };
    let (hash, actor) = (
        newest_hash(&database).await,
        Actor::browser(Realm::Admin, None, None),
    );
    let asking = text(
        &database,
        "SELECT id::text FROM identity.admin_sessions ORDER BY started_at DESC LIMIT 1",
    )
    .await;
    let mut expected = Vec::new();
    for (change, presented, answer, record) in [
        (
            "SELECT 1",
            [7; 32],
            Ending::NoSession,
            "refused - - - ended".to_string(),
        ),
        (
            "UPDATE identity.admin_sessions SET proven_at = proven_at - interval '11 minutes'",
            hash,
            Ending::NotFresh,
            format!("refused - {asking} - stale"),
        ),
        (
            "UPDATE identity.admin_sessions SET last_seen_at = last_seen_at - interval '16 minutes'",
            hash,
            Ending::NoSession,
            "refused - - - ended".to_string(),
        ),
        (
            "UPDATE identity.admins SET disabled_at = now()",
            hash,
            Ending::NoSession,
            "refused - - - disabled".to_string(),
        ),
    ] {
        as_db_admin(&database, change).await;
        let ended = store.end_other_sessions(&presented, None, &actor).await;
        assert_eq!(ended.expect("answers"), answer, "{change}");
        expected.push(format!("{record} browser@unknown"));
    }
    assert_eq!(recorded(&database).await, expected);
    assert_eq!(held(&database).await, "2");
}

/// Joining names the device its session started on, and a session already named keeps its name.
#[tokio::test]
async fn joining_names_the_device_once() {
    let (address, database, link) = serving().await;
    let _ = join(address, &link).await;
    let device = "SELECT coalesce(string_agg(device, ' '), '-') FROM identity.admin_sessions";
    assert_eq!(text(&database, device).await, "Firefox on Linux");
    as_db_admin(
        &database,
        "SELECT identity.describe_session(token_hash, 'Edge on Windows') FROM identity.admin_sessions",
    )
    .await;
    assert_eq!(text(&database, device).await, "Firefox on Linux");
}

/// Listing sessions renews none but the one asking, which its own request renews.
#[tokio::test]
async fn listing_renews_no_other_session() {
    let (address, database, cookies) = logged_in_from(2).await;
    as_db_admin(
        &database,
        "UPDATE identity.admin_sessions SET last_seen_at = last_seen_at - interval '5 minutes'",
    )
    .await;
    assert_eq!(get(address, SECURITY, &cookies[1]).await.status, 200);
    let idle = "SELECT count(*)::text FROM identity.admin_sessions
         WHERE last_seen_at < statement_timestamp() - interval '4 minutes'";
    assert_eq!(text(&database, idle).await, "1");
}

/// A function's columns and a row for them, once with each column of the type `wrong` gives, and its value.
fn each_broken(columns: &[(&str, &str)], wrong: (&str, &str)) -> Vec<(String, String)> {
    (0..columns.len())
        .map(|broken| {
            let (shaped, row): (Vec<String>, Vec<&str>) = columns
                .iter()
                .enumerate()
                .map(|(at, (column, value))| {
                    let name = column.split(' ').next().unwrap_or_default();
                    if at == broken {
                        (format!("{name} {}", wrong.0), wrong.1)
                    } else {
                        ((*column).to_string(), *value)
                    }
                })
                .unzip();
            (shaped.join(", "), row.join(", "))
        })
        .collect()
}

/// The shapes of an ending storage cannot read: each column the wrong type, no row, and a negative count.
fn unreadable_endings() -> Vec<(String, String)> {
    let columns = [
        ("who bigint", "1::bigint"),
        ("asking text", "'x'::text"),
        ("ended bigint", "1::bigint"),
        ("reason text", "null::text"),
    ];
    let whole: Vec<&str> = columns.iter().map(|(column, _)| *column).collect();
    let mut shapes = each_broken(&columns, ("boolean", "true"));
    for row in [
        "1::bigint, 'x', 1::bigint, null::text WHERE false",
        "1::bigint, 'x', -1::bigint, null::text",
        "1::bigint, 'x', null::bigint, 'elsewhere'::text",
        "1::bigint, 'x', null::bigint, null::text",
        "1::bigint, 'x', 5::bigint, 'stale'::text",
        "null::bigint, null::text, 1::bigint, null::text",
        "1::bigint, null::text, null::bigint, 'stale'::text",
        "1::bigint, 'x', null::bigint, 'ended'::text",
    ] {
        shapes.push((whole.join(", "), row.to_string()));
    }
    shapes
}

/// An ending storage answers in the wrong shape, any column of it, or with no row, is unavailable.
#[tokio::test]
async fn an_unreadable_ending_is_unavailable() {
    for (shaped, row) in unreadable_endings() {
        let (address, database, cookies) = logged_in_from(2).await;
        let form = form_on(&get(address, SECURITY, &cookies[1]).await.body);
        as_db_admin(
            &database,
            &format!(
                "DROP FUNCTION identity.end_other_sessions(bytea, uuid);
                 CREATE FUNCTION identity.end_other_sessions(bytea, uuid) RETURNS TABLE ({shaped})
                 LANGUAGE sql AS $$ SELECT {row} $$;
                 GRANT EXECUTE ON FUNCTION identity.end_other_sessions(bytea, uuid) TO tetsu_identity"
            ),
        )
        .await;
        assert_eq!(
            end(address, &cookies[1], &form, None).await.status,
            503,
            "{row}"
        );
    }
}
