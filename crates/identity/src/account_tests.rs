//! The operator's pages in the settings shell any realm fills, after Linear's settings mode: Profile and Security
//! (#156), and the mailbox in the Platform group only operators have (ADR-0084). Each page carries a skip link, a
//! labelled settings `nav` headed by Tetsu's wordmark, then `main` with the page's `h1`; logout from the sidebar's
//! foot and the narrow bar's initials menu (`v5.0.0-7.4.4`); and a narrow-screen menu that is a native popover, which
//! a browser without popovers shows in place, above the page.

use crate::http_tests::{MENU_SCRIPT, PROFILE, assert_in_order, get, logged_in, logs_out_from};
use crate::join_tests::as_admin;

const SECURITY: &str = "/admin/settings/security";
const MAILBOX: &str = "/admin/mailbox";

/// What each page in the shell holds, in the order a reader meets it.
const SHELL_PARTS: [&str; 11] = [
    "<a href=\"#main\"",
    "<nav aria-label=\"Settings\"",
    "aria-label=\"Tetsu home\"",
    ">Personal</p>",
    "href=\"/admin/settings/profile\"",
    "href=\"/admin/settings/security\"",
    ">Platform</p>",
    "href=\"/admin/mailbox\"",
    "<form method=\"post\" action=\"/admin/logout\"",
    "<main id=\"main\"",
    "<h1",
];

/// Each page in the shell has all of it, in order, and marks only its own page as current in the sidebar. Operators
/// have no app outside it yet (#157), so nothing links back to one.
#[tokio::test]
async fn each_account_page_has_the_settings_shell() {
    let (address, _database, cookie, _) = logged_in().await;
    for (path, title) in [
        (PROFILE, "Profile"),
        (SECURITY, "Security"),
        (MAILBOX, "Mailbox"),
    ] {
        let page = get(address, path, &cookie).await;
        assert_eq!(page.status, 200, "{path}");
        assert_eq!(page.header("cache-control"), Some("no-store"), "{path}");
        assert_in_order(&page, &SHELL_PARTS, path);
        assert!(page.body.contains(&format!(">{title}</h1>")), "{path}");
        assert!(
            page.body
                .contains(&format!("<title>{title} – Tetsu</title>")),
            "{path}"
        );
        assert!(!page.body.contains("Back to app"), "{path}");
        for other in [PROFILE, SECURITY, MAILBOX] {
            let current = format!("href=\"{other}\" aria-current=\"page\"");
            assert_eq!(
                page.body.contains(&current),
                other == path,
                "{other} on {path}"
            );
        }
    }
}

/// Below a tablet's width the sidebar is a native popover that "Menu" opens, with its own close button, inside the
/// menu island, which keeps focus where it shows; the narrow bar also carries the initials menu, so logout is one tap
/// away.
#[tokio::test]
async fn the_narrow_menu_is_a_popover() {
    let (address, _database, cookie, _) = logged_in().await;
    let page = get(address, SECURITY, &cookie).await;
    let script = format!("<script type=\"module\" src=\"{MENU_SCRIPT}\">");
    for part in [
        "popovertarget=\"tetsu-settings-menu\" aria-label=\"Menu\"",
        "<tetsu-menu class=\"contents\"><div id=\"tetsu-settings-menu\" popover",
        script.as_str(),
        "popovertarget=\"tetsu-settings-menu\" popovertargetaction=\"hide\" aria-label=\"Close menu\"",
        "popovertarget=\"tetsu-account-menu\"",
    ] {
        assert!(page.body.contains(part), "{part}: {}", page.body);
    }
    // No page-load focus, which a browser without popovers would give the menu it shows in place; each copy of the
    // sidebar names its group with an id of its own.
    assert_eq!(page.body.matches("autofocus").count(), 1, "{}", page.body);
    assert_eq!(page.body.matches("<nav aria-label=\"Settings\"").count(), 2);
    assert_ne!(
        page.body.matches("id=\"tetsu-personal\"").count(),
        page.body.matches("id=\"tetsu-personal").count()
    );
}

/// The sidebar's Log out carries the page's own form token, so it logs the operator out from any page in it.
#[tokio::test]
async fn each_account_page_logs_out() {
    for path in [PROFILE, SECURITY, MAILBOX] {
        logs_out_from(path).await;
    }
}

/// Profile shows who the operator is: their name, their email address, or that they have none, and their role.
#[tokio::test]
async fn profile_shows_the_operators_name_email_and_role() {
    let (address, database, cookie, _) = logged_in().await;
    let page = get(address, PROFILE, &cookie).await;
    for shown in [">Kurt Medley</p>", ">No email address</p>", ">Operator</p>"] {
        assert!(page.body.contains(shown), "{shown}: {}", page.body);
    }
    as_admin(
        &database,
        "UPDATE identity.operators SET email = 'kurt@example.com'",
    )
    .await;
    let page = get(address, PROFILE, &cookie).await;
    assert!(page.body.contains(">kurt@example.com</p>"), "{}", page.body);
    assert!(!page.body.contains("No email address"));
}

/// Security lists sessions first, as Linear does, then the operator's passkeys, one row each, inside the passkeys
/// island, which adds one from the page.
#[tokio::test]
async fn security_lists_sessions_then_passkeys() {
    let (address, _database, cookie, _) = logged_in().await;
    let page = get(address, SECURITY, &cookie).await;
    let sessions = page
        .body
        .find(">Sessions</h2>")
        .expect("a sessions section");
    let passkeys = page
        .body
        .find(">Passkeys</h2>")
        .expect("a passkeys section");
    assert!(sessions < passkeys, "{}", page.body);
    for part in [
        "<ul role=\"list\" aria-label=\"Sessions\"",
        "This session",
        "<tetsu-passkeys",
        "<ul role=\"list\" aria-label=\"Passkeys\"",
        "<tetsu-passkey action=\"add\"",
        "Turn on JavaScript to use passkeys.</p></noscript>",
    ] {
        assert!(page.body.contains(part), "{part}: {}", page.body);
    }
}

/// Anyone without a live session is sent to log in, and returns to the page after: Profile, where a login lands
/// anyway, needs no saying.
#[tokio::test]
async fn an_account_page_without_a_session_goes_to_log_in() {
    let (address, _database, _, _) = logged_in().await;
    for (path, back) in [
        (PROFILE, "/admin/login"),
        (SECURITY, "/admin/login?next=/admin/settings/security"),
        (MAILBOX, "/admin/login?next=/admin/mailbox"),
    ] {
        let page = get(address, path, "").await;
        assert_eq!(
            (page.status, page.header("location")),
            (303, Some(back)),
            "{path}"
        );
    }
}

/// Until operators have a home (#157), their realm's own address sends them to their Profile, where logging in lands,
/// and anyone without a session to log in (ADR-0084).
#[tokio::test]
async fn the_realms_address_goes_to_profile() {
    let (address, _database, cookie, _) = logged_in().await;
    for (cookie, to) in [(cookie.as_str(), PROFILE), ("", "/admin/login")] {
        let page = get(address, "/admin", cookie).await;
        assert_eq!((page.status, page.header("location")), (303, Some(to)));
    }
}

/// A session on a phone shows a phone, and any other a desktop, whose icon the color scheme control's System also
/// draws once.
#[tokio::test]
async fn a_session_shows_the_kind_of_device_it_is_on() {
    const PHONE: &str = "d=\"M10.5 1.5H8.25";
    const DESKTOP: &str = "d=\"M9 17.25v1.007";
    let (address, database, cookie, _) = logged_in().await;
    for (device, phones, desktops) in [
        ("Safari on iOS", 1, 1),
        ("Chrome on Android", 1, 1),
        ("Safari on iPadOS", 0, 2),
        ("Firefox on Linux", 0, 2),
    ] {
        as_admin(
            &database,
            &format!("UPDATE identity.sessions SET device = '{device}'"),
        )
        .await;
        let page = get(address, SECURITY, &cookie).await;
        assert_eq!(
            (
                page.body.matches(PHONE).count(),
                page.body.matches(DESKTOP).count()
            ),
            (phones, desktops),
            "{device}"
        );
    }
}

/// An account page is unavailable, rather than missing a part, when it cannot check the session, read the operator's
/// account, or, for Profile, read their email.
#[tokio::test]
async fn an_account_page_that_cannot_be_read_is_unavailable() {
    for (path, broken) in [
        (
            PROFILE,
            "REVOKE EXECUTE ON FUNCTION identity.use_session(bytea, text) FROM tetsu_identity",
        ),
        (
            PROFILE,
            "REVOKE EXECUTE ON FUNCTION identity.account(bytea) FROM tetsu_identity",
        ),
        (
            PROFILE,
            "REVOKE SELECT (email) ON identity.operators FROM tetsu_identity",
        ),
        (
            SECURITY,
            "REVOKE EXECUTE ON FUNCTION identity.use_session(bytea, text) FROM tetsu_identity",
        ),
        (
            SECURITY,
            "REVOKE EXECUTE ON FUNCTION identity.account(bytea) FROM tetsu_identity",
        ),
    ] {
        let (address, database, cookie, _) = logged_in().await;
        as_admin(&database, broken).await;
        assert_eq!(
            get(address, path, &cookie).await.status,
            503,
            "{path}: {broken}"
        );
    }
}
