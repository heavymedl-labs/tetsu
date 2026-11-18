//! The operator pages and their JSON endpoints over HTTP, against a real database, with passkey-rs as the device.

use std::fmt::Write as _;
use std::net::SocketAddr;
use std::sync::Mutex;
use tetsu_kernel::Requester;

use passkey::types::Passkey;
use serde_json::{Value, json};
use tetsu_postgres::test_support::TestDatabase;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::adapters::http::{PATHS, Web, router};
use crate::adapters::postgres::PostgresOperators;
use crate::join_tests::{ORIGIN, as_admin, authenticate, party, register_keeping, with_link};
use crate::{Operators, Origins};

const SCRIPT: &str = "/assets/passkey-test.js";
pub(crate) const SESSION_SCRIPT: &str = "/assets/session-test.js";
const POLICY: &str = "default-src 'none'; object-src 'none'; script-src 'self'; connect-src 'self'; \
                      style-src 'self'; font-src 'self'; img-src 'self'; base-uri 'none'; form-action 'self'; \
                      frame-ancestors 'none'";
const REFUSED: &str = "this request was refused";
pub(crate) const UNAVAILABLE: &str = "logging in is unavailable just now; try again in a minute";

/// What the operator was told, from every test, since `report` is a plain function.
static REPORTED: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn report(line: &str) {
    REPORTED.lock().expect("reports").push(line.to_string());
}

pub(crate) fn reported(about: &str) -> bool {
    REPORTED
        .lock()
        .expect("reports")
        .iter()
        .any(|line| line.contains(about))
}

/// A response, read whole.
pub(crate) struct Reply {
    pub(crate) status: u16,
    head: String,
    pub(crate) body: String,
}

impl Reply {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.head.lines().find_map(|line| {
            let (found, value) = line.split_once(": ")?;
            found.eq_ignore_ascii_case(name).then_some(value)
        })
    }

    pub(crate) fn json(&self) -> Value {
        serde_json::from_str(&self.body).expect("JSON")
    }
}

/// Serves Identity's routes on a port of their own, over a fresh database with Kurt Medley's link issued.
pub(crate) async fn serving() -> (SocketAddr, TestDatabase, String) {
    let (operators, database, _, link) = with_link().await;
    (serve(operators).await, database, link)
}

async fn serve(operators: Operators<PostgresOperators>) -> SocketAddr {
    let web = Web {
        party: party(),
        origins: Origins::parse(&[ORIGIN]).expect("origins"),
        demo: true,
        stylesheet: "/assets/tetsu-test.css",
        script: SCRIPT,
        session_script: SESSION_SCRIPT,
        scheme_script: "/assets/scheme-test.js",
        report,
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("binds");
    let address = listener.local_addr().expect("an address");
    let app = router(operators, web).layer(axum::middleware::from_fn(requester));
    tokio::spawn(async move { axum::serve(listener, app).await });
    address
}

/// Notes each request's network as the site does behind one proxy: the last `X-Forwarded-For` entry, so a test
/// names its network, or this machine.
async fn requester(
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let local = std::net::IpAddr::from([127, 0, 0, 1]);
    let forwarded = request.headers().get_all("x-forwarded-for");
    let from = Requester::from_request(
        local,
        forwarded.iter().filter_map(|value| value.to_str().ok()),
        1,
    );
    request.extensions_mut().insert(from);
    next.run(request).await
}

pub(crate) async fn send(
    address: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Reply {
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: ops.example.com\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        let _ = write!(request, "{name}: {value}\r\n");
    }
    request.push_str("\r\n");
    request.push_str(body);
    let mut stream = TcpStream::connect(address).await.expect("connects");
    stream.write_all(request.as_bytes()).await.expect("sends");
    let mut raw = String::new();
    stream.read_to_string(&mut raw).await.expect("reads");
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_default();
    Reply {
        status,
        head: head.to_string(),
        body: body.to_string(),
    }
}

/// What the passkey script sends: JSON, from the site itself, with the browser's cookie if it has one.
pub(crate) async fn post(address: SocketAddr, path: &str, cookie: &str, body: &Value) -> Reply {
    let headers = [
        ("Origin", ORIGIN),
        ("Sec-Fetch-Site", "same-origin"),
        ("Content-Type", "application/json"),
        ("Cookie", cookie),
        (
            "User-Agent",
            "Mozilla/5.0 (X11; Linux x86_64) Firefox/140.0",
        ),
    ];
    send(address, "POST", path, &headers, &body.to_string()).await
}

pub(crate) async fn get(address: SocketAddr, path: &str, cookie: &str) -> Reply {
    send(address, "GET", path, &[("Cookie", cookie)], "").await
}

/// Joins with the link through the pages' endpoints, as the script would, and returns the passkey the device keeps
/// and the session cookie joining set.
pub(crate) async fn join(address: SocketAddr, link: &str) -> (Passkey, String) {
    let begun = post(address, "/ops/join/begin", "", &json!({ "link": link }))
        .await
        .json();
    let (response, kept) = register_keeping(&begun["options"].to_string(), ORIGIN, true).await;
    let response: Value = serde_json::from_str(&response).expect("JSON");
    let finished = post(
        address,
        "/ops/join/finish",
        "",
        &json!({"ceremony": begun["ceremony"], "response": response}),
    )
    .await;
    // Joining logs the operator in, with a session that rests on the link (ADR-0069).
    assert_eq!(
        (finished.status, finished.json()),
        (200, json!({"next": "/ops"}))
    );
    assert!(
        finished
            .header("set-cookie")
            .is_some_and(|set| set.starts_with("__Host-tetsu-operator=")),
        "{}",
        finished.head
    );
    (kept, cookie_in(&finished))
}

/// The session cookie a response set, as the browser would send it back.
fn cookie_in(reply: &Reply) -> String {
    let set = reply.header("set-cookie").expect("a cookie");
    set.split(';').next().unwrap_or_default().to_string()
}

/// Logs in through the pages' endpoints, as the script would, sending `next` if given, and returns how finishing
/// answered.
pub(crate) async fn login_sending(
    address: SocketAddr,
    passkey: Passkey,
    presented: &str,
    next: Option<&str>,
) -> Reply {
    let begun = post(address, "/ops/login/begin", "", &json!({}))
        .await
        .json();
    let response = authenticate(&begun["options"].to_string(), ORIGIN, passkey, true).await;
    let response: Value = serde_json::from_str(&response).expect("JSON");
    let mut finish = json!({"ceremony": begun["ceremony"], "response": response});
    if let Some(next) = next {
        finish["next"] = json!(next);
    }
    post(address, "/ops/login/finish", presented, &finish).await
}

/// The form token the operator's page carries.
pub(crate) fn form_on(page: &str) -> String {
    let (_, after) = page
        .split_once("name=\"form\" value=\"")
        .expect("a form token");
    after.split('"').next().unwrap_or_default().to_string()
}

/// Kurt Medley, joined and logged in through the pages: the server, the database, his cookie, and the form token
/// his page carries.
pub(crate) async fn logged_in() -> (SocketAddr, TestDatabase, String, String) {
    let (address, database, link) = serving().await;
    // A browser that joined presents that session when it logs in, which ends it.
    let (passkey, joined) = join(address, &link).await;
    let finished = login_sending(address, passkey, &joined, None).await;
    assert_eq!(
        (finished.status, finished.json()),
        (200, json!({"next": "/ops"}))
    );
    let cookie = cookie_in(&finished);
    let page = get(address, "/ops", &cookie).await;
    (address, database, cookie, form_on(&page.body))
}

/// One text value the database answers.
pub(crate) async fn text(database: &TestDatabase, sql: &str) -> String {
    tetsu_postgres::connect(database)
        .await
        .expect("connects")
        .query_one(sql, &[])
        .await
        .expect("answers")
        .try_get(0)
        .expect("text")
}

/// Every path Identity serves begins at `/ops`, which the composition root reserves.
#[test]
fn every_path_is_under_ops() {
    assert!(
        PATHS
            .iter()
            .all(|path| *path == "/ops" || path.starts_with("/ops/"))
    );
}

/// Adds a passkey through the pages' endpoints, as the script would, and returns how finishing answered.
async fn add_passkey(address: SocketAddr, cookie: &str, form: &str) -> Reply {
    let request = json!({ "form": form });
    let begun = post(address, "/ops/passkeys/begin", cookie, &request)
        .await
        .json();
    let (response, _) = register_keeping(&begun["options"].to_string(), ORIGIN, true).await;
    let response: Value = serde_json::from_str(&response).expect("JSON");
    let finish = json!({"form": form, "ceremony": begun["ceremony"], "response": response});
    post(address, "/ops/passkeys/finish", cookie, &finish).await
}

/// Posts the operator page's logout form from `origin`.
pub(crate) async fn logout(address: SocketAddr, origin: &str, cookie: &str, form: &str) -> Reply {
    let headers = [("Origin", origin), ("Cookie", cookie)];
    send(
        address,
        "POST",
        "/ops/logout",
        &headers,
        &format!("form={form}"),
    )
    .await
}

/// An operator joins and logs in as the passkey script would, sees their page, and adds a second passkey; with
/// two, the page offers no third.
#[tokio::test]
async fn an_operator_joins_logs_in_and_adds_a_passkey() {
    let (address, database, cookie, form) = logged_in().await;
    let page = get(address, "/ops", &cookie).await;
    assert_eq!(
        (page.status, page.header("content-security-policy")),
        (200, Some(POLICY))
    );
    let account = "SELECT encode(uuid_send(user_handle), 'base64') FROM identity.operators";
    let handle = text(&database, account).await;
    let user = handle
        .trim_end_matches('=')
        .replace('+', "-")
        .replace('/', "_");
    for shown in [
        "<tetsu-passkeys user=",
        "Kurt Medley",
        "Synced passkey",
        "action=\"add\"",
        &format!("user=\"{user}\""),
    ] {
        assert!(page.body.contains(shown), "{shown} in {}", page.body);
    }
    let added = add_passkey(address, &cookie, &form).await;
    assert_eq!((added.status, added.json()), (200, json!({"next": "/ops"})));
    let page = get(address, "/ops", &cookie).await;
    assert_eq!(
        page.body.matches("Synced passkey").count(),
        2,
        "{}",
        page.body
    );
    assert!(!page.body.contains("action=\"add\""), "two is the most");
}

/// Ten minutes after logging in, the page no longer offers to add a passkey, which the server would refuse, and
/// says how to get it back.
#[tokio::test]
async fn a_stale_login_is_told_to_login_again_to_add_a_passkey() {
    let (address, database, cookie, _) = logged_in().await;
    as_admin(
        &database,
        "UPDATE identity.sessions SET proven_at = now() - interval '11 minutes'",
    )
    .await;
    let page = get(address, "/ops", &cookie).await;
    assert!(!page.body.contains("action=\"add\""), "{}", page.body);
    assert!(page.body.contains("log out and log in"), "{}", page.body);
}

/// A login counts as fresh for under ten minutes, and one that starts after now counts too.
#[test]
fn a_login_is_fresh_for_under_ten_minutes_and_a_session_never_proven_never_is() {
    use std::time::{Duration, SystemTime};
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    let started = |seconds: i64| {
        let offset = Duration::from_secs(seconds.unsigned_abs());
        if seconds < 0 {
            now - offset
        } else {
            now + offset
        }
    };
    assert_eq!(
        [0, -599, -600, -601, 60]
            .map(|seconds| crate::adapters::http::fresh(Some(started(seconds)), now)),
        [true, true, false, false, true]
    );
    assert!(!crate::adapters::http::fresh(None, now));
}

/// Logging out clears the cookie and sends the browser to log in, and the operator's page then does the same.
#[tokio::test]
async fn logging_out_clears_the_cookie() {
    let (address, _database, cookie, form) = logged_in().await;
    let out = logout(address, ORIGIN, &cookie, &form).await;
    let cleared = "__Host-tetsu-operator=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0";
    assert_eq!(
        (out.status, out.header("location"), out.header("set-cookie")),
        (303, Some("/ops/login"), Some(cleared))
    );
    let page = get(address, "/ops", &cookie).await;
    assert_eq!(page.header("location"), Some("/ops/login"));
}

/// The pages before login load Tetsu's own script under the operator pages' policy, and are kept by no cache. A
/// browser with no live session asking for the operator's page is sent to log in.
#[tokio::test]
async fn the_pages_run_only_tetsus_own_script() {
    let (address, _database, _) = serving().await;
    for path in ["/ops/login", "/ops/join"] {
        let page = get(address, path, "").await;
        assert_eq!(
            (
                page.status,
                page.header("content-security-policy"),
                page.header("cache-control")
            ),
            (200, Some(POLICY), Some("no-store")),
            "{path}"
        );
        // A posted form, such as logging out, then carries the page's real origin, not null.
        assert_eq!(
            page.header("referrer-policy"),
            Some("same-origin"),
            "{path}"
        );
        assert!(
            page.body
                .contains(&format!("<script type=\"module\" src=\"{SCRIPT}\">")),
            "{path}"
        );
    }
    let unknown = format!("__Host-tetsu-operator={}", "ab".repeat(32));
    for cookie in ["", unknown.as_str()] {
        let page = get(address, "/ops", cookie).await;
        assert_eq!(
            (page.status, page.header("location")),
            (303, Some("/ops/login")),
            "{cookie}"
        );
    }
}

/// Each page before login offers its one action as a passkey island, in the shared button's look, under Tetsu's
/// wordmark, which links home, with no note above it and no "Runs on Tetsu" footer.
#[tokio::test]
async fn each_page_offers_one_passkey_island() {
    let (address, _database, _) = serving().await;
    for (path, action) in [("/ops/login", "login"), ("/ops/join", "join")] {
        let page = get(address, path, "").await;
        for absent in ["role=\"note\"", "Runs on"] {
            assert!(!page.body.contains(absent), "{absent} in {}", page.body);
        }
        for shown in [
            format!("<tetsu-passkey action=\"{action}\""),
            "viewBox=\"0 0 564 144\"".to_string(),
            "<a href=\"/\" aria-label=\"Tetsu home\"".to_string(),
            "disabled:opacity-50 rounded-full font-medium focus-visible:outline-2".to_string(),
        ] {
            assert!(page.body.contains(&shown), "{shown} in {}", page.body);
        }
    }
}

/// A passkey on one device is listed as one, not as synced.
/// A path under `/ops` that Identity does not serve sends anyone logged out to log in, so no one can map the
/// operator pages without a session, and tells a logged-in operator it is not found, on an operator page.
#[tokio::test]
async fn an_unknown_operator_path_is_a_login_or_not_found() {
    let (address, _database, cookie, _) = logged_in().await;
    let unknown = format!("__Host-tetsu-operator={}", "ab".repeat(32));
    for path in ["/ops/nowhere", "/ops/a/b"] {
        for logged_out in ["", unknown.as_str()] {
            let page = get(address, path, logged_out).await;
            let back = format!("/ops/login?next={path}");
            assert_eq!(
                (page.status, page.header("location")),
                (303, Some(back.as_str())),
                "{path} {logged_out}"
            );
        }
        let page = get(address, path, &cookie).await;
        assert_eq!(
            (page.status, page.header("cache-control")),
            (404, Some("no-store")),
            "{path}"
        );
        for shown in [
            "<title>Not found – Tetsu</title>",
            "viewBox=\"0 0 564 144\"",
            "href=\"/ops\"",
        ] {
            assert!(page.body.contains(shown), "{shown} in {}", page.body);
        }
    }
}

#[tokio::test]
async fn a_passkey_on_one_device_is_listed_as_one() {
    let (address, database, cookie, _) = logged_in().await;
    as_admin(
        &database,
        "UPDATE identity.passkeys SET passkey = jsonb_set(passkey, '{cred,backup_state}', 'false')",
    )
    .await;
    let page = get(address, "/ops", &cookie).await;
    assert!(page.body.contains("Passkey on one device"), "{}", page.body);
}

/// A request before login is read only if it comes from the site, as both its origin and, when sent, its
/// `Sec-Fetch-Site` say, and only as a JSON object; each endpoint refuses alike.
#[tokio::test]
async fn requests_before_login_come_from_the_site_as_json() {
    let (address, _database, _) = serving().await;
    let json = ("Content-Type", "application/json");
    for path in [
        "/ops/login/begin",
        "/ops/login/finish",
        "/ops/join/begin",
        "/ops/join/finish",
    ] {
        for headers in [
            vec![
                ("Origin", "https://evil.example"),
                ("Sec-Fetch-Site", "cross-site"),
                json,
            ],
            vec![("Origin", ORIGIN), ("Sec-Fetch-Site", "cross-site"), json],
            vec![("Sec-Fetch-Site", "same-origin"), json],
            // Sent but not readable as text is never taken for not sent.
            vec![
                ("Origin", ORIGIN),
                ("Sec-Fetch-Site", "s\u{e9}me-origin"),
                json,
            ],
        ] {
            let refused = send(address, "POST", path, &headers, "{}").await;
            assert_eq!(
                (refused.status, refused.json()),
                (403, json!({ "error": REFUSED })),
                "{path}"
            );
        }
    }
    let begin = |kind: &'static str, body: &'static str| {
        let headers = [("Origin", ORIGIN), ("Content-Type", kind)];
        async move { send(address, "POST", "/ops/login/begin", &headers, body).await }
    };
    assert_eq!(begin("text/plain", "{}").await.status, 415);
    for body in ["[]", "not JSON"] {
        assert_eq!(begin("application/json", body).await.status, 400, "{body}");
    }
    let started = begin("application/json; charset=utf-8", "{}").await;
    assert_eq!(
        (
            started.status,
            started.header("cache-control"),
            started.json()["options"]["publicKey"]["timeout"].as_u64()
        ),
        (200, Some("no-store"), Some(60_000))
    );
}

/// A ceremony the use case refuses gets its one refusal, the same whatever was wrong.
#[tokio::test]
async fn refused_ceremonies_get_their_one_refusal() {
    let (address, _database, _) = serving().await;
    let said = [
        (
            "/ops/login/finish",
            json!({"ceremony": 1}),
            "this login could not be completed",
        ),
        (
            "/ops/join/begin",
            json!({"link": "not a link"}),
            "this invite has ended; if you have already joined, log in, or ask an operator for a new one",
        ),
        (
            "/ops/join/finish",
            json!({}),
            "this invite has ended; if you have already joined, log in, or ask an operator for a new one",
        ),
    ];
    for (path, body, refusal) in said {
        let refused = post(address, path, "", &body).await;
        assert_eq!(
            (refused.status, refused.json()),
            (400, json!({ "error": refusal })),
            "{path}"
        );
    }
}

/// A logged-in change needs the session's form token from the site, and a live session: without the token it is
/// forbidden, once logged out it is unauthorized, and it is read only as JSON. Logging out from elsewhere is
/// forbidden and ends nothing.
#[tokio::test]
async fn logged_in_changes_need_the_session_and_its_form_token() {
    let (address, _database, cookie, form) = logged_in().await;
    for path in ["/ops/passkeys/begin", "/ops/passkeys/finish"] {
        let refused = post(address, path, &cookie, &json!({})).await;
        assert_eq!(
            (refused.status, refused.json()),
            (403, json!({ "error": REFUSED })),
            "{path}"
        );
        let headers = [
            ("Origin", ORIGIN),
            ("Cookie", cookie.as_str()),
            ("Content-Type", "text/plain"),
        ];
        assert_eq!(
            send(address, "POST", path, &headers, "{}").await.status,
            415,
            "{path}"
        );
    }
    let refused = logout(address, "https://evil.example", &cookie, &form).await;
    assert_eq!(refused.status, 403);
    // A form shows pages, so the refusal is one that says nothing changed.
    assert_eq!(
        refused.header("content-type"),
        Some("text/html; charset=utf-8")
    );
    assert!(refused.body.contains("Not logged out"), "{}", refused.body);
    assert_eq!(
        get(address, "/ops", &cookie).await.status,
        200,
        "still logged in"
    );
    logout(address, ORIGIN, &cookie, &form).await;
    let ended = post(
        address,
        "/ops/passkeys/begin",
        &cookie,
        &json!({ "form": form }),
    )
    .await;
    assert_eq!(
        (ended.status, ended.json()),
        (401, json!({"error": "you are logged out; log in again"}))
    );
}

/// A failure of the server is unavailable to the browser, which learns nothing more, and reported to the operator.
#[tokio::test]
async fn failures_of_the_server_are_unavailable() {
    let revoke = |function: &str| {
        format!("REVOKE EXECUTE ON FUNCTION identity.{function} FROM tetsu_identity")
    };
    let (address, database, cookie, form) = logged_in().await;
    as_admin(&database, &revoke("account(bytea)")).await;
    let page = get(address, "/ops", &cookie).await;
    assert_eq!(
        (page.status, page.header("cache-control")),
        (503, Some("no-store"))
    );
    as_admin(&database, &revoke("end_session(bytea, text)")).await;
    let out = logout(address, ORIGIN, &cookie, &form).await;
    assert_eq!(out.status, 503);
    assert!(
        out.body.contains("Logging in is unavailable just now"),
        "{}",
        out.body
    );
    as_admin(&database, &revoke("use_session(bytea, text)")).await;
    assert_eq!(get(address, "/ops", &cookie).await.status, 503);
    assert_eq!(get(address, "/ops/nowhere", &cookie).await.status, 503);
    let refused = post(
        address,
        "/ops/passkeys/begin",
        &cookie,
        &json!({ "form": form }),
    )
    .await;
    assert_eq!(
        (refused.status, refused.json()),
        (503, json!({ "error": UNAVAILABLE }))
    );
    as_admin(&database, &revoke("begin_login(bytea, jsonb)")).await;
    let refused = post(address, "/ops/login/begin", "", &json!({})).await;
    assert_eq!(
        (refused.status, refused.json()),
        (503, json!({ "error": UNAVAILABLE }))
    );
    assert!(reported(
        "identity: could not show an operator's page: storage failed"
    ));
    assert!(reported(
        "identity: a passkey request failed: storage failed"
    ));
    assert!(reported(
        "identity: could not log an operator out: storage failed"
    ));
}

/// The JSON's shape alone: each object's keys, each array's first item, and each value's kind, so two sets of
/// options compare equal when only their challenges and IDs differ.
fn shape(value: &Value) -> Value {
    match value {
        Value::Object(fields) => fields
            .iter()
            .map(|(key, value)| (key.clone(), shape(value)))
            .collect(),
        Value::Array(items) => Value::Array(items.iter().take(1).map(shape).collect()),
        Value::String(_) => json!("text"),
        Value::Number(_) => json!("number"),
        Value::Bool(_) => json!("bool"),
        Value::Null => Value::Null,
    }
}

/// The passkey script is tested against a fixture of the options these endpoints send, so the fixture must keep
/// their shape: registering with a passkey held, and logging in.
#[tokio::test]
async fn the_scripts_fixture_has_the_options_shapes() {
    let (address, _database, cookie, form) = logged_in().await;
    let add = post(
        address,
        "/ops/passkeys/begin",
        &cookie,
        &json!({ "form": form }),
    )
    .await;
    let sign = post(address, "/ops/login/begin", "", &json!({})).await;
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../web/src/passkey/fixtures/options.json"
    ))
    .expect("JSON");
    assert_eq!(
        shape(&json!({"creation": add.json()["options"], "request": sign.json()["options"]})),
        shape(&fixture)
    );
}

/// Posts to a step before login from the network `from` names, as the site's proxy would add it.
async fn post_from(address: SocketAddr, path: &str, from: &str, body: &Value) -> Reply {
    let headers = [
        ("Origin", ORIGIN),
        ("Content-Type", "application/json"),
        ("X-Forwarded-For", from),
    ];
    send(address, "POST", path, &headers, &body.to_string()).await
}

/// Each step before login is held to its limit: thirty login begins, ten join steps, and ten failed logins from
/// one network, each then refused with when to try again; failed joins are not failed logins, and another network
/// is not held back (ADR-0072).
#[tokio::test]
async fn each_step_before_login_is_held_to_its_network_s_limit() {
    let (address, _database, _) = serving().await;
    let bad = json!({"ceremony": "x", "response": {}, "link": "x"});
    let statuses = async |path: &str, from: &str, times: usize| {
        let mut seen = Vec::new();
        for _ in 0..times {
            seen.push(post_from(address, path, from, &bad).await.status);
        }
        seen
    };
    assert_eq!(
        statuses("/ops/login/begin", "192.0.2.1", 31).await[29..],
        [200, 429]
    );
    assert_eq!(statuses("/ops/join/begin", "192.0.2.2", 5).await, [400; 5]);
    assert_eq!(
        statuses("/ops/join/finish", "192.0.2.2", 6).await,
        [400, 400, 400, 400, 400, 429]
    );
    assert_eq!(
        statuses("/ops/login/finish", "192.0.2.2", 10).await,
        [400; 10]
    );
    let refused = post_from(address, "/ops/login/finish", "192.0.2.2", &bad).await;
    assert_eq!(refused.status, 429, "{}", refused.body);
    assert_eq!(refused.header("retry-after"), Some("60"));
    assert_eq!(
        refused.body,
        json!({"error": "too many tries from your network. Try again in a minute"}).to_string()
    );
    assert_eq!(statuses("/ops/login/finish", "192.0.2.3", 1).await, [400]);
}

/// A refusal by a limit says when in whole minutes, and asks for at least a second's wait.
#[tokio::test]
async fn a_limit_says_when_to_try_again() {
    for (wait, retry, when) in [
        (0, "1", "a minute"),
        (60, "60", "a minute"),
        (61, "61", "2 minutes"),
        (3600, "3600", "60 minutes"),
    ] {
        let response = crate::adapters::http::limited(std::time::Duration::from_secs(wait));
        assert_eq!(response.status(), 429);
        assert_eq!(response.headers()["retry-after"], retry, "{wait}");
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("a body");
        let said = format!("too many tries from your network. Try again in {when}");
        assert_eq!(body, json!({ "error": said }).to_string(), "{wait}");
    }
}
