//! The edges of an operator's login over HTTP: a login page that knows who is already logged in, where it returns
//! to, why it was shown, what logging out clears, and how a page learns its session's time left and stays
//! logged in (docs/research/identity/operator-lifecycles.md).

use serde_json::{Value, json};

use crate::http_tests::{
    SESSION_SCRIPT, UNAVAILABLE, form_on, get, join, logged_in, login_sending, logout, post,
    reported, serving, text,
};
use crate::join_tests::{ORIGIN, as_admin};

/// The notice the login page shows when a session ended under the operator, rather than by their logging out.
const ENDED: &str = "For your security, we logged you out.";

/// A logged-in operator who opens the login page is sent on, to the page it would return to or their own, as
/// Django's `redirect_authenticated_user` does; logging in again would only replace their session.
#[tokio::test]
async fn a_logged_in_operator_is_sent_on_from_the_login_page() {
    let (address, _database, cookie, _) = logged_in().await;
    for (path, to) in [
        ("/ops/login", "/ops"),
        ("/ops/login?next=/ops/passkeys", "/ops/passkeys"),
        ("/ops/login?ended=1&next=/ops/a", "/ops/a"),
        ("/ops/login?next=//evil.example", "/ops"),
        ("/ops/login?next=/login", "/ops"),
    ] {
        let page = get(address, path, &cookie).await;
        assert_eq!(
            (page.status, page.header("location")),
            (303, Some(to)),
            "{path}"
        );
    }
}

/// The login page carries the page to return to, once checked, to its passkey island, and says why it was shown
/// when the session ended on its own.
#[tokio::test]
async fn the_login_page_keeps_a_checked_return_and_says_why_it_shows() {
    let (address, _database, _) = serving().await;
    let page = get(address, "/ops/login?next=/ops/passkeys&ended=1", "").await;
    assert_eq!(page.status, 200);
    for shown in ["next=\"/ops/passkeys\"", ENDED] {
        assert!(page.body.contains(shown), "{shown} in {}", page.body);
    }
    for path in [
        "/ops/login",
        "/ops/login?next=//evil.example",
        "/ops/login?next=",
        "/ops/login?ended=0",
    ] {
        let page = get(address, path, "").await;
        assert_eq!(page.status, 200, "{path}");
        for absent in ["next=\"", ENDED] {
            assert!(!page.body.contains(absent), "{absent} in {path}");
        }
    }
}

/// Finishing a login sends the browser where the page asked, once checked again on the server, since the script's
/// request could say anything; anything else goes to the operator's page.
#[tokio::test]
async fn a_login_returns_where_the_page_asked_once_checked() {
    for (sent, next) in [
        (Some("/ops/passkeys"), "/ops/passkeys"),
        (Some("//evil.example"), "/ops"),
        (Some("https://evil.example/ops"), "/ops"),
        (None, "/ops"),
    ] {
        let (address, _database, link) = serving().await;
        let passkey = join(address, &link).await;
        let finished = login_sending(address, passkey, sent).await;
        assert_eq!(
            (finished.status, finished.json()),
            (200, json!({ "next": next })),
            "{sent:?}"
        );
    }
}

/// Logging out also tells the browser to drop what it cached of the operator's pages, so Back shows none of them.
/// Only the cache: clearing cookies would end the browser's sessions in every realm on the host.
#[tokio::test]
async fn logging_out_clears_the_browsers_cache_of_the_pages() {
    let (address, _database, cookie, form) = logged_in().await;
    let out = logout(address, ORIGIN, &cookie, &form).await;
    assert_eq!(
        (out.status, out.header("clear-site-data")),
        (303, Some("\"cache\""))
    );
}

/// Moves every session's last request `minutes` into the past, as if the operator had been away.
async fn away(database: &tetsu_postgres::test_support::TestDatabase, minutes: u32) {
    as_admin(
        database,
        &format!("UPDATE identity.sessions SET last_seen_at = last_seen_at - interval '{minutes} minutes'"),
    )
    .await;
}

/// Seconds a session-left answer gives each limit.
fn left(answer: &Value) -> (i64, i64) {
    (
        answer["idle"].as_i64().expect("idle"),
        answer["absolute"].as_i64().expect("absolute"),
    )
}

/// A page reads how long its session has left, by the database's clock, without the read counting as a request:
/// only the person's own requests renew a session (NIST SP 800-63B-4, OWASP).
#[tokio::test]
async fn a_page_reads_its_sessions_time_left_without_renewing_it() {
    let (address, database, cookie, _) = logged_in().await;
    away(&database, 5).await;
    let seen = "SELECT last_seen_at::text FROM identity.sessions";
    let before = text(&database, seen).await;
    let answer = get(address, "/ops/session", &cookie).await;
    assert_eq!(
        (answer.status, answer.header("cache-control")),
        (200, Some("no-store"))
    );
    let (idle, absolute) = left(&answer.json());
    assert!((595..=600).contains(&idle), "{idle}");
    assert!((28_790..=28_800).contains(&absolute), "{absolute}");
    assert_eq!(
        text(&database, seen).await,
        before,
        "the read renewed nothing"
    );
}

/// The read's idle limit is the one requests are held to: past it, the read finds no session, and neither does the
/// operator's page.
#[tokio::test]
async fn a_session_past_its_idle_limit_has_no_time_left() {
    let (address, database, cookie, _) = logged_in().await;
    away(&database, 15).await;
    let answer = get(address, "/ops/session", &cookie).await;
    assert_eq!(
        (answer.status, answer.json()),
        (401, json!({ "error": "you are logged out; log in again" }))
    );
    let page = get(address, "/ops", &cookie).await;
    assert_eq!(
        (page.status, page.header("location")),
        (303, Some("/ops/login"))
    );
}

/// Staying logged in is a request of the person's own, with the page's form token: it renews the session and says
/// how long it has now. Without the token, or once the session has ended, it is refused.
#[tokio::test]
async fn staying_logged_in_renews_the_session() {
    let (address, database, cookie, form) = logged_in().await;
    away(&database, 10).await;
    let stayed = post(address, "/ops/session", &cookie, &json!({ "form": form })).await;
    assert_eq!(stayed.status, 200);
    let (idle, _) = left(&stayed.json());
    assert!((895..=900).contains(&idle), "{idle}");
    let refused = post(address, "/ops/session", &cookie, &json!({})).await;
    assert_eq!(refused.status, 403);
    away(&database, 15).await;
    let ended = post(address, "/ops/session", &cookie, &json!({ "form": form })).await;
    assert_eq!(ended.status, 401);
}

/// Every page a logged-in operator sees carries the session island and its script, with the form token staying
/// logged in sends; the login page carries neither.
#[tokio::test]
async fn logged_in_pages_carry_the_session_island() {
    let (address, _database, cookie, _) = logged_in().await;
    let script = format!("<script type=\"module\" src=\"{SESSION_SCRIPT}\">");
    for path in ["/ops", "/ops/nowhere"] {
        let page = get(address, path, &cookie).await;
        // Each page masks its form token afresh, so the island carries the one this page's logout form does.
        let island = format!("<tetsu-session form-token=\"{}\"", form_on(&page.body));
        for shown in [&script, &island] {
            assert!(page.body.contains(shown.as_str()), "{shown} on {path}");
        }
    }
    let login = get(address, "/ops/login?next=/ops", "").await;
    assert!(!login.body.contains("tetsu-session"), "{}", login.body);
}

/// A read of the time left that storage refuses is unavailable to the page, which asks again later, and reported to
/// the operator.
#[tokio::test]
async fn a_failed_read_of_the_time_left_is_unavailable() {
    let (address, database, cookie, _) = logged_in().await;
    as_admin(
        &database,
        "REVOKE EXECUTE ON FUNCTION identity.session_left(bytea, text) FROM tetsu_identity",
    )
    .await;
    let left = get(address, "/ops/session", &cookie).await;
    assert_eq!(
        (left.status, left.json()),
        (503, json!({ "error": UNAVAILABLE }))
    );
    assert!(reported(
        "identity: a passkey request failed: storage failed"
    ));
}
