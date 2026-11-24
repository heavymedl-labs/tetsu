//! Requests a check refuses before they reach storage are recorded, each with its route and the check it failed,
//! while the network they came from has room under the limit on recording refusals (#152, `v5.0.0-16.3.3`).

use std::net::SocketAddr;

use serde_json::{Value, json};
use tetsu_postgres::test_support::TestDatabase;

use crate::http_tests::{PROFILE, Reply, get, logged_in, send, serving, text};
use crate::join_tests::{ORIGIN, as_admin};

const FIREFOX: &str = "Mozilla/5.0 (X11; Linux x86_64) Firefox/140.0";
const ELSEWHERE: &str = "https://evil.example";

/// Each refusal's record, oldest first: its route, its reason, its outcome, and its actor, and every other key it
/// holds, so a record that kept anything more would show it.
async fn refusals(database: &TestDatabase) -> Vec<String> {
    let all = text(
        database,
        "SELECT coalesce(string_agg(concat_ws(' ', payload->>'route', payload->>'reason', payload->>'outcome',
             payload->>'actor', (SELECT string_agg(k, ',' ORDER BY k) FROM jsonb_object_keys(payload) AS k)),
             '|' ORDER BY occurred_at, id), '')
         FROM identity.outbox_global WHERE event_type = 'identity.request_refused'",
    )
    .await;
    all.split('|')
        .filter(|one| !one.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// What a record of a refusal reads as, from Firefox on Linux.
fn refusal(route: &str, reason: &str) -> String {
    format!("{route} {reason} refused browser@Firefox on Linux actor,outcome,reason,route")
}

/// Posts a page's form, `[path, origin, cookie, body]`, as a browser on Firefox, from network `network`.
async fn form(address: SocketAddr, sent: [&str; 4], network: &str) -> Reply {
    let [path, origin, cookie, body] = sent;
    let headers = [
        ("Origin", origin),
        ("Cookie", cookie),
        ("User-Agent", FIREFOX),
        ("X-Forwarded-For", network),
    ];
    send(address, "POST", path, &headers, body).await
}

/// Posts JSON, as the passkey script would, from `origin`.
async fn script(
    address: SocketAddr,
    path: &str,
    origin: &str,
    cookie: &str,
    body: &Value,
) -> Reply {
    let headers = [
        ("Origin", origin),
        ("Content-Type", "application/json"),
        ("Cookie", cookie),
        ("User-Agent", FIREFOX),
    ];
    send(address, "POST", path, &headers, &body.to_string()).await
}

/// Every form a check refuses is recorded once, naming its route and the check it failed, and nothing of its token
/// or address, and the session it carried stays live.
#[tokio::test]
async fn each_refused_form_is_recorded_with_its_check() {
    let (address, database, cookie, form_token) = logged_in().await;
    let wrong_form = format!("form={}", "ab".repeat(32));
    let good_form = format!("form={form_token}");
    let bad_id = format!("{good_form}&session=NOT-AN-ID");
    let (logout, end, live) = ("/admin/logout", "/admin/sessions/end", cookie.as_str());
    let (good, wrong) = (good_form.as_str(), wrong_form.as_str());
    for sent in [
        [logout, ELSEWHERE, live, good],
        [logout, ORIGIN, "", good],
        [logout, ORIGIN, live, wrong],
        [end, ORIGIN, live, bad_id.as_str()],
        // A bad id from a request that fails another check is recorded as failing that one.
        [end, ORIGIN, live, "session=NOT-AN-ID"],
        [end, ELSEWHERE, live, good],
    ] {
        let reply = form(address, sent, "203.0.113.7").await;
        assert_eq!(reply.status, 403, "{sent:?}");
    }
    assert_eq!(
        refusals(&database).await,
        [
            refusal("/admin/logout", "origin"),
            refusal("/admin/logout", "cookie"),
            refusal("/admin/logout", "form_token"),
            refusal("/admin/sessions/end", "field"),
            refusal("/admin/sessions/end", "form_token"),
            refusal("/admin/sessions/end", "origin"),
        ]
    );
    assert_eq!(get(address, PROFILE, &cookie).await.status, 200);
}

/// Every script request a check refuses is recorded once too: logged in, at its form token or origin, and before
/// login, at its origin.
#[tokio::test]
async fn each_refused_script_request_is_recorded_with_its_check() {
    let (address, database, cookie, form_token) = logged_in().await;
    for (path, origin, sent) in [
        ("/admin/passkeys/begin", ORIGIN, json!({ "form": "wrong" })),
        ("/admin/session", ELSEWHERE, json!({ "form": form_token })),
        ("/admin/login/begin", ELSEWHERE, json!({})),
    ] {
        let reply = script(address, path, origin, &cookie, &sent).await;
        assert_eq!(reply.status, 403, "{path} {origin}");
    }
    assert_eq!(
        refusals(&database).await,
        [
            refusal("/admin/passkeys/begin", "form_token"),
            refusal("/admin/session", "origin"),
            refusal("/admin/login/begin", "origin"),
        ]
    );
}

/// A network records ten refusals in fifteen minutes; past that its requests are still refused, but not recorded,
/// while another network's are.
#[tokio::test]
async fn a_flood_of_refusals_is_recorded_only_up_to_its_limit() {
    let (address, database, _) = serving().await;
    for _ in 0..15 {
        let reply = form(address, ["/admin/logout", ELSEWHERE, "", ""], "203.0.113.7").await;
        assert_eq!(reply.status, 403);
    }
    let other = form(
        address,
        ["/admin/logout", ELSEWHERE, "", ""],
        "198.51.100.9",
    )
    .await;
    assert_eq!(other.status, 403);
    assert_eq!(refusals(&database).await.len(), 11);
    // The first refusal past the limit says that the rest go unrecorded, once in the window.
    let suppressed = text(
        &database,
        "SELECT string_agg(payload->>'route' || ' ' || (payload->>'reason') || ' ' || (payload->>'actor'), '|')
         FROM identity.outbox_global WHERE event_type = 'identity.refusals_suppressed'",
    )
    .await;
    assert_eq!(suppressed, "/admin/logout origin browser@Firefox on Linux");
}

/// A refusal that cannot be recorded leaves the request unavailable rather than refused unseen.
#[tokio::test]
async fn a_refusal_that_cannot_be_recorded_is_unavailable() {
    let (address, database, cookie, form_token) = logged_in().await;
    as_admin(
        &database,
        "REVOKE EXECUTE ON FUNCTION identity.admit(text, bytea) FROM tetsu_identity",
    )
    .await;
    let page = form(
        address,
        ["/admin/logout", ELSEWHERE, &cookie, ""],
        "203.0.113.7",
    )
    .await;
    assert_eq!(page.status, 503);
    let sent = json!({ "form": form_token });
    let json = script(address, "/admin/session", ELSEWHERE, &cookie, &sent).await;
    assert_eq!(json.status, 503);
    // Nothing was refused unseen, and the session the requests carried is still live.
    assert_eq!(get(address, PROFILE, &cookie).await.status, 200);
}

/// A limit's answer storage gives in the wrong shape leaves the refusal unavailable, never recorded as if admitted.
#[tokio::test]
async fn an_unreadable_limit_leaves_a_refusal_unavailable() {
    let (address, database, _) = serving().await;
    as_admin(
        &database,
        "DROP FUNCTION identity.admit(text, bytea);
         CREATE FUNCTION identity.admit(text, bytea) RETURNS text LANGUAGE sql AS $$ SELECT 'soon' $$;
         GRANT EXECUTE ON FUNCTION identity.admit(text, bytea) TO tetsu_identity",
    )
    .await;
    let reply = form(address, ["/admin/logout", ELSEWHERE, "", ""], "203.0.113.7").await;
    assert_eq!(reply.status, 503);
    assert_eq!(refusals(&database).await, Vec::<String>::new());
}

/// Another site's form posted to a logged-in script's route is recorded at its origin, before its body is read, as a
/// browser sends such a form without asking.
#[tokio::test]
async fn another_sites_form_to_a_script_route_is_recorded() {
    let (address, database, cookie, _) = logged_in().await;
    let reply = form(
        address,
        ["/admin/session", ELSEWHERE, &cookie, "form=x"],
        "203.0.113.7",
    )
    .await;
    assert_eq!(reply.status, 403);
    assert_eq!(
        refusals(&database).await,
        [refusal("/admin/session", "origin")]
    );
}

/// What ADR-0081 leaves unrecorded stays so: malformed input from the realm's own origin, and a request that passes
/// the checks but whose session has ended.
#[tokio::test]
async fn malformed_input_and_an_ended_session_are_not_recorded() {
    let (address, database, cookie, form_token) = logged_in().await;
    let text_body = form(
        address,
        ["/admin/session", ORIGIN, &cookie, "form=x"],
        "203.0.113.7",
    )
    .await;
    assert_eq!(text_body.status, 415);
    as_admin(&database, "DELETE FROM identity.sessions").await;
    let ended = script(
        address,
        "/admin/session",
        ORIGIN,
        &cookie,
        &json!({ "form": form_token }),
    )
    .await;
    assert_eq!(ended.status, 401);
    let body = format!("form={form_token}");
    let logout = form(
        address,
        ["/admin/logout", ORIGIN, &cookie, &body],
        "203.0.113.7",
    )
    .await;
    assert_eq!(logout.status, 303);
    assert_eq!(refusals(&database).await, Vec::<String>::new());
}
