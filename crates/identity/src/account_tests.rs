//! The operator's account area (#156): Profile and Security pages in a settings shell any realm fills, after
//! Linear's settings mode. Each page carries a skip link, a labelled settings `nav` headed by Tetsu's wordmark and
//! "Back to app", then `main` with the page's `h1`; logout from the sidebar's foot and the narrow bar's initials
//! menu (`v5.0.0-7.4.4`); and a narrow-screen menu that is a native popover, which a browser without popovers shows
//! in place, above the page.

use crate::http_tests::{MENU_SCRIPT, assert_in_order, get, logged_in, logs_out_from};
use crate::join_tests::as_admin;

const PROFILE: &str = "/ops/account/profile";
const SECURITY: &str = "/ops/account/security";

/// What each account page holds, in the order a reader meets it.
const SHELL_PARTS: [&str; 10] = [
    "<a href=\"#main\"",
    "<nav aria-label=\"Settings\"",
    "aria-label=\"Tetsu home\"",
    "Back to app</a>",
    ">Personal</p>",
    "href=\"/ops/account/profile\"",
    "href=\"/ops/account/security\"",
    "<form method=\"post\" action=\"/ops/logout\"",
    "<main id=\"main\"",
    "<h1",
];

/// Each account page has the whole shell, in order, and marks only its own page as current in the sidebar.
#[tokio::test]
async fn each_account_page_has_the_settings_shell() {
    let (address, _database, cookie, _) = logged_in().await;
    for (path, title) in [(PROFILE, "Profile"), (SECURITY, "Security")] {
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
        for other in [PROFILE, SECURITY] {
            let current = format!("href=\"{other}\" aria-current=\"page\"");
            assert_eq!(
                page.body.contains(&current),
                other == path,
                "{other} on {path}"
            );
        }
    }
}

/// Below a laptop's width the sidebar is a native popover that "Menu" opens, with its own close button, inside the
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

/// The sidebar's Log out carries the page's own form token, so it logs the operator out from either page.
#[tokio::test]
async fn each_account_page_logs_out() {
    for path in [PROFILE, SECURITY] {
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
        "Passkeys need JavaScript, which this browser has turned off.</p></noscript>",
    ] {
        assert!(page.body.contains(part), "{part}: {}", page.body);
    }
}

/// Anyone without a live session is sent to log in, and returns to the page after.
#[tokio::test]
async fn an_account_page_without_a_session_goes_to_log_in() {
    let (address, _database, _, _) = logged_in().await;
    for path in [PROFILE, SECURITY] {
        let page = get(address, path, "").await;
        let back = format!("/ops/login?next={path}");
        assert_eq!(
            (page.status, page.header("location")),
            (303, Some(back.as_str())),
            "{path}"
        );
    }
}

/// Until the operator's home is built (#157), their page links to each account page and the mailbox.
#[tokio::test]
async fn the_operators_page_links_to_their_account() {
    let (address, _database, cookie, _) = logged_in().await;
    let page = get(address, "/ops", &cookie).await;
    for link in [PROFILE, SECURITY, "/ops/mailbox"] {
        assert!(
            page.body.contains(&format!("href=\"{link}\"")),
            "{link}: {}",
            page.body
        );
    }
    // Its passkeys moved to Security, and their notice with them.
    assert!(
        !page.body.contains("Passkeys need JavaScript"),
        "{}",
        page.body
    );
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
