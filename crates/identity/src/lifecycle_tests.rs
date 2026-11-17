//! The edges of an operator's login over HTTP: a login page that knows who is already logged in, where it returns
//! to, why it was shown, and what logging out clears (docs/research/identity/operator-lifecycles.md).

use serde_json::json;

use crate::http_tests::{get, join, logged_in, login_sending, logout, serving};
use crate::join_tests::ORIGIN;

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
