//! A logged-in operator's page outside the settings shell, which is only the not-found page while every other is in
//! it (ADR-0084), carries one header: Tetsu's wordmark home at left, and at right the operator's initials opening a
//! menu with their name, "Operator", Account, and Log out (#148, `v5.0.0-7.4.4`). Pages before login, and the
//! refused and unavailable pages, carry none.

use crate::adapters::http::initials;
use crate::http_tests::{PROFILE, assert_in_order, get, logged_in, logs_out_from, send};
use crate::join_tests::{as_admin, replace};

const MENU_BUTTON: &str = "popovertarget=\"tetsu-account-menu\"";

/// What the header shows on a logged-in page, in the order a reader meets it. The button's name starts with the
/// initials it shows, so speech input can say them (WCAG 2.5.3), and its target and the menu's rows grow to 44px
/// under a coarse pointer.
const HEADER_PARTS: [&str; 14] = [
    "<a href=\"#main\"",
    "<header",
    "<a href=\"/\" aria-label=\"Tetsu home\"",
    MENU_BUTTON,
    "pointer-coarse:after:content-['']\">KM<span class=\"sr-only\">, account menu, Kurt Medley</span></button>",
    "<div id=\"tetsu-account-menu\" popover",
    ">Kurt Medley</p>",
    ">Operator</p>",
    "href=\"/admin/settings/profile\"",
    "pointer-coarse:py-3\">Account</a>",
    "<form method=\"post\" action=\"/admin/logout\"",
    "name=\"form\" value=\"",
    "pointer-coarse:py-3\">Log out</button>",
    "<main id=\"main\"",
];

/// A logged-in page outside the settings shell has the whole header, in order, and marks nothing current.
#[tokio::test]
async fn every_logged_in_page_has_the_header_and_its_menu() {
    let (address, _database, cookie, _) = logged_in().await;
    let path = "/admin/nowhere";
    let page = get(address, path, &cookie).await;
    assert_eq!(page.status, 404);
    assert_in_order(&page, &HEADER_PARTS, path);
    assert_eq!(page.body.matches(MENU_BUTTON).count(), 1);
    assert!(
        !page.body.contains("aria-current=\"page\""),
        "{}",
        page.body
    );
}

/// Pages before login, and the refused and unavailable pages, show no menu.
#[tokio::test]
async fn pages_without_a_session_show_no_menu() {
    let (address, database, cookie, form) = logged_in().await;
    let mut pages = vec![
        get(address, "/admin/login", "").await,
        get(address, "/admin/join", "").await,
    ];
    let elsewhere = [
        ("Origin", "https://evil.example"),
        ("Cookie", cookie.as_str()),
    ];
    let refused = send(
        address,
        "POST",
        "/admin/logout",
        &elsewhere,
        &format!("form={form}"),
    )
    .await;
    assert_eq!(refused.status, 403);
    pages.push(refused);
    as_admin(
        &database,
        "REVOKE EXECUTE ON FUNCTION identity.account(bytea) FROM tetsu_identity",
    )
    .await;
    let unavailable = get(address, "/admin/nowhere", &cookie).await;
    assert_eq!(unavailable.status, 503);
    pages.push(unavailable);
    for page in pages {
        assert!(!page.body.contains(MENU_BUTTON), "{}", page.body);
    }
}

/// An operator disabled since their session was checked has no account to name, so the mailbox and the
/// not-found page send them to log in, as their Profile does.
#[tokio::test]
async fn a_page_without_an_account_goes_to_log_in() {
    let (address, database, cookie, _) = logged_in().await;
    let returns = "TABLE (operator_id bigint, user_handle bytea, name text, credential_id bytea, transports text, \
                   backup_eligible boolean, backup_state boolean)";
    let none = "SELECT 1::bigint, ''::bytea, ''::text, ''::bytea, ''::text, true, true WHERE false";
    replace(&database, "account(bytea)", returns, none).await;
    for path in ["/admin/mailbox", "/admin/nowhere"] {
        let page = get(address, path, &cookie).await;
        let back = format!("/admin/login?next={path}");
        assert_eq!(
            (page.status, page.header("location")),
            (303, Some(back.as_str())),
            "{path}"
        );
    }
    let profile = get(address, PROFILE, &cookie).await;
    assert_eq!(profile.header("location"), Some("/admin/login"));
}

/// The menu's initials are the first letters of the first and last words of the name, in capitals.
#[test]
fn initials_are_the_first_and_last_names_first_letters() {
    for (name, expected) in [
        ("Kurt Medley", "KM"),
        ("Ada", "A"),
        ("Grace Brewster Hopper", "GH"),
        ("  éva   lópez ", "ÉL"),
        ("", ""),
    ] {
        assert_eq!(initials(name), expected, "{name:?}");
    }
}

/// A page that cannot read the operator's account to name them is unavailable, rather than a page without a name.
#[tokio::test]
async fn a_page_whose_account_cannot_be_read_is_unavailable() {
    let (address, database, cookie, _) = logged_in().await;
    as_admin(
        &database,
        "REVOKE EXECUTE ON FUNCTION identity.account(bytea) FROM tetsu_identity",
    )
    .await;
    for path in ["/admin/mailbox", "/admin/nowhere"] {
        assert_eq!(get(address, path, &cookie).await.status, 503, "{path}");
    }
}

/// The menu's Log out carries the page's own form token, so it logs the operator out.
#[tokio::test]
async fn the_menu_logs_out() {
    logs_out_from("/admin/nowhere").await;
}

/// An operator's name is shown as text everywhere the header puts it, however it reads (`v5.0.0-3.2.2`).
#[tokio::test]
async fn a_name_with_markup_is_shown_as_text() {
    let (address, database, cookie, _) = logged_in().await;
    as_admin(
        &database,
        "UPDATE identity.operators SET name = '<b>Ada</b> & \"Co\"'",
    )
    .await;
    let path = "/admin/nowhere";
    let page = get(address, path, &cookie).await;
    assert!(!page.body.contains("<b>Ada"), "{path}: {}", page.body);
    for escaped in [
        "&#60;b&#62;Ada&#60;/b&#62; &#38; &#34;Co&#34;</p>",
        ">&#60;&#34;<span class=\"sr-only\">, account menu, &#60;b&#62;Ada",
    ] {
        assert!(
            page.body.contains(escaped),
            "{escaped} on {path}: {}",
            page.body
        );
    }
}
