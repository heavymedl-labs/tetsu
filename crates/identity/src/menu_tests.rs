//! The admin's initials menu (#148, `v5.0.0-7.4.4`), at the end of the settings shell's narrow bar on every
//! admin page: their initials opening a menu with their name, "Admin", Account, and Log out. Pages before login,
//! and the refused, unavailable, and not-found pages, carry none.

use crate::adapters::http::initials;
use crate::http_tests::{PROFILE, assert_in_order, get, logged_in, send};
use crate::join_tests::{as_db_admin, replace};

const MENU_BUTTON: &str = "popovertarget=\"tetsu-account-menu\"";

/// What the menu shows, in the order a reader meets it. The button's name starts with the initials it shows, so speech
/// input can say them (WCAG 2.5.3), and its target and the menu's rows grow to 44px under a coarse pointer.
const MENU_PARTS: [&str; 10] = [
    MENU_BUTTON,
    "pointer-coarse:after:content-['']\">KM<span class=\"sr-only\">, account menu, Kurt Medley</span></button>",
    "<div id=\"tetsu-account-menu\" popover",
    ">Kurt Medley</p>",
    ">Admin</p>",
    "href=\"/admin/settings/profile\"",
    "pointer-coarse:py-3\">Account</a>",
    "<form method=\"post\" action=\"/admin/logout\"",
    "name=\"form\" value=\"",
    "pointer-coarse:py-3\">Log out</button>",
];

/// Each admin page carries the whole menu once, in order.
#[tokio::test]
async fn every_admin_page_has_the_menu() {
    let (address, _database, cookie, _) = logged_in().await;
    for path in [PROFILE, "/admin/settings/security", "/admin/mailbox"] {
        let page = get(address, path, &cookie).await;
        assert_eq!(page.status, 200, "{path}");
        assert_in_order(&page, &MENU_PARTS, path);
        assert_eq!(page.body.matches(MENU_BUTTON).count(), 1, "{path}");
    }
}

/// Pages before login, the refused and unavailable pages, and the not-found page, which is the site's, show no menu.
#[tokio::test]
async fn pages_without_a_session_show_no_menu() {
    let (address, database, cookie, form) = logged_in().await;
    let mut pages = vec![
        get(address, "/admin/login", "").await,
        get(address, "/admin/join", "").await,
        get(address, "/admin/nowhere", &cookie).await,
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
    as_db_admin(
        &database,
        "REVOKE EXECUTE ON FUNCTION identity.account(bytea) FROM tetsu_identity",
    )
    .await;
    let unavailable = get(address, PROFILE, &cookie).await;
    assert_eq!(unavailable.status, 503);
    pages.push(unavailable);
    for page in pages {
        assert!(!page.body.contains(MENU_BUTTON), "{}", page.body);
    }
}

/// An admin disabled since their session was checked has no account to name, so the mailbox sends them to log in,
/// returning there after, as their Profile does.
#[tokio::test]
async fn a_page_without_an_account_goes_to_log_in() {
    let (address, database, cookie, _) = logged_in().await;
    let returns = "TABLE (admin_id bigint, user_handle bytea, name text, credential_id bytea, transports text, \
                   backup_eligible boolean, backup_state boolean)";
    let none = "SELECT 1::bigint, ''::bytea, ''::text, ''::bytea, ''::text, true, true WHERE false";
    replace(&database, "account(bytea)", returns, none).await;
    for (path, to) in [
        ("/admin/mailbox", "/admin/login?next=/admin/mailbox"),
        (PROFILE, "/admin/login"),
    ] {
        let page = get(address, path, &cookie).await;
        assert_eq!(
            (page.status, page.header("location")),
            (303, Some(to)),
            "{path}"
        );
    }
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

/// A page that cannot read the admin's account to name them is unavailable, rather than a page without a name.
#[tokio::test]
async fn a_page_whose_account_cannot_be_read_is_unavailable() {
    let (address, database, cookie, _) = logged_in().await;
    as_db_admin(
        &database,
        "REVOKE EXECUTE ON FUNCTION identity.account(bytea) FROM tetsu_identity",
    )
    .await;
    assert_eq!(get(address, "/admin/mailbox", &cookie).await.status, 503);
}

/// An admin's name is shown as text everywhere the menu and the sidebar put it, however it reads (`v5.0.0-3.2.2`).
#[tokio::test]
async fn a_name_with_markup_is_shown_as_text() {
    let (address, database, cookie, _) = logged_in().await;
    as_db_admin(
        &database,
        "UPDATE identity.admins SET name = '<b>Ada</b> & \"Co\"'",
    )
    .await;
    let page = get(address, PROFILE, &cookie).await;
    assert!(!page.body.contains("<b>Ada"), "{}", page.body);
    for escaped in [
        "&#60;b&#62;Ada&#60;/b&#62; &#38; &#34;Co&#34;</p>",
        ">&#60;&#34;<span class=\"sr-only\">, account menu, &#60;b&#62;Ada",
    ] {
        assert!(page.body.contains(escaped), "{escaped}: {}", page.body);
    }
}
