//! The operator pages and their JSON endpoints over HTTP, against a real database, with passkey-rs as the device.

use std::fmt::Write as _;
use std::net::SocketAddr;
use std::sync::Mutex;

use passkey::types::Passkey;
use serde_json::{Value, json};
use tetsu_postgres::test_support::TestDatabase;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::adapters::http::{PATHS, Web, router};
use crate::adapters::postgres::PostgresOperators;
use crate::enrollment_tests::{ORIGIN, as_admin, authenticate, party, register_keeping, with_link};
use crate::{Operators, Origins};

const SCRIPT: &str = "/assets/passkey-test.js";
const POLICY: &str = "default-src 'none'; object-src 'none'; script-src 'self'; connect-src 'self'; \
                      style-src 'self'; font-src 'self'; img-src 'self'; base-uri 'none'; form-action 'self'; \
                      frame-ancestors 'none'";
const REFUSED: &str = "this request was refused";
const UNAVAILABLE: &str = "signing in is unavailable just now; try again in a minute";

/// What the operator was told, from every test, since `report` is a plain function.
static REPORTED: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn report(line: &str) {
    REPORTED.lock().expect("reports").push(line.to_string());
}

fn reported(about: &str) -> bool {
    REPORTED
        .lock()
        .expect("reports")
        .iter()
        .any(|line| line.contains(about))
}

/// A response, read whole.
struct Reply {
    status: u16,
    head: String,
    body: String,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.head.lines().find_map(|line| {
            let (found, value) = line.split_once(": ")?;
            found.eq_ignore_ascii_case(name).then_some(value)
        })
    }

    fn json(&self) -> Value {
        serde_json::from_str(&self.body).expect("JSON")
    }
}

/// Serves Identity's routes on a port of their own, over a fresh database with Kurt Medley's link issued.
async fn serving() -> (SocketAddr, TestDatabase, String) {
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
        report,
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("binds");
    let address = listener.local_addr().expect("an address");
    tokio::spawn(async move { axum::serve(listener, router(operators, web)).await });
    address
}

async fn send(
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
async fn post(address: SocketAddr, path: &str, cookie: &str, body: &Value) -> Reply {
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

async fn get(address: SocketAddr, path: &str, cookie: &str) -> Reply {
    send(address, "GET", path, &[("Cookie", cookie)], "").await
}

/// Enrolls with the link through the pages' endpoints, as the script would, and returns the passkey the device
/// keeps.
async fn enroll(address: SocketAddr, link: &str) -> Passkey {
    let begun = post(address, "/ops/enroll/begin", "", &json!({ "link": link }))
        .await
        .json();
    let (response, kept) = register_keeping(&begun["options"].to_string(), ORIGIN, true).await;
    let response: Value = serde_json::from_str(&response).expect("JSON");
    let finished = post(
        address,
        "/ops/enroll/finish",
        "",
        &json!({"ceremony": begun["ceremony"], "response": response}),
    )
    .await;
    assert_eq!(
        (finished.status, finished.json()),
        (200, json!({"next": "/ops/sign-in"}))
    );
    kept
}

/// Signs in through the pages' endpoints, as the script would, and returns the session cookie set.
async fn sign_in(address: SocketAddr, passkey: Passkey) -> String {
    let begun = post(address, "/ops/sign-in/begin", "", &json!({}))
        .await
        .json();
    let response = authenticate(&begun["options"].to_string(), ORIGIN, passkey, true).await;
    let response: Value = serde_json::from_str(&response).expect("JSON");
    let finished = post(
        address,
        "/ops/sign-in/finish",
        "",
        &json!({"ceremony": begun["ceremony"], "response": response}),
    )
    .await;
    assert_eq!(
        (finished.status, finished.json()),
        (200, json!({"next": "/ops"}))
    );
    let set = finished.header("set-cookie").expect("a cookie");
    set.split(';').next().unwrap_or_default().to_string()
}

/// The form token the operator's page carries.
fn form_on(page: &str) -> String {
    let (_, after) = page
        .split_once("name=\"form\" value=\"")
        .expect("a form token");
    after.split('"').next().unwrap_or_default().to_string()
}

/// Kurt Medley, enrolled and signed in through the pages: the server, the database, his cookie, and the form token
/// his page carries.
async fn signed_in() -> (SocketAddr, TestDatabase, String, String) {
    let (address, database, link) = serving().await;
    let passkey = enroll(address, &link).await;
    let cookie = sign_in(address, passkey).await;
    let page = get(address, "/ops", &cookie).await;
    (address, database, cookie, form_on(&page.body))
}

/// One text value the database answers.
async fn text(database: &TestDatabase, sql: &str) -> String {
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

/// Posts the operator page's sign-out form from `origin`.
async fn sign_out(address: SocketAddr, origin: &str, cookie: &str, form: &str) -> Reply {
    let headers = [("Origin", origin), ("Cookie", cookie)];
    send(
        address,
        "POST",
        "/ops/sign-out",
        &headers,
        &format!("form={form}"),
    )
    .await
}

/// An operator enrolls and signs in as the passkey script would, sees their page, and adds a second passkey; with
/// two, the page offers no third.
#[tokio::test]
async fn an_operator_enrolls_signs_in_and_adds_a_passkey() {
    let (address, database, cookie, form) = signed_in().await;
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

/// Ten minutes after signing in, the page no longer offers to add a passkey, which the server would refuse, and
/// says how to get it back.
#[tokio::test]
async fn a_stale_sign_in_is_told_to_sign_in_again_to_add_a_passkey() {
    let (address, database, cookie, _) = signed_in().await;
    as_admin(
        &database,
        "UPDATE identity.sessions SET started_at = now() - interval '11 minutes'",
    )
    .await;
    let page = get(address, "/ops", &cookie).await;
    assert!(!page.body.contains("action=\"add\""), "{}", page.body);
    assert!(page.body.contains("sign out and sign in"), "{}", page.body);
}

/// A sign-in counts as fresh for under ten minutes, and one that starts after now counts too.
#[test]
fn a_sign_in_is_fresh_for_under_ten_minutes() {
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
            .map(|seconds| crate::adapters::http::fresh(started(seconds), now)),
        [true, true, false, false, true]
    );
}

/// Signing out clears the cookie and sends the browser to sign in, and the operator's page then does the same.
#[tokio::test]
async fn signing_out_clears_the_cookie() {
    let (address, _database, cookie, form) = signed_in().await;
    let out = sign_out(address, ORIGIN, &cookie, &form).await;
    let cleared = "__Host-tetsu-operator=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0";
    assert_eq!(
        (out.status, out.header("location"), out.header("set-cookie")),
        (303, Some("/ops/sign-in"), Some(cleared))
    );
    let page = get(address, "/ops", &cookie).await;
    assert_eq!(page.header("location"), Some("/ops/sign-in"));
}

/// The pages before sign-in load Tetsu's own script under the operator pages' policy, and are kept by no cache. A
/// browser with no live session asking for the operator's page is sent to sign in.
#[tokio::test]
async fn the_pages_run_only_tetsus_own_script() {
    let (address, _database, _) = serving().await;
    for path in ["/ops/sign-in", "/ops/enroll"] {
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
        // A posted form, such as signing out, then carries the page's real origin, not null.
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
            (303, Some("/ops/sign-in")),
            "{cookie}"
        );
    }
}

/// Each page before sign-in offers its one action as a passkey island, in the shared button's look, under Tetsu's
/// wordmark. It shows no demo business, so it has no demo banner and no "Runs on Tetsu" footer.
#[tokio::test]
async fn each_page_offers_one_passkey_island() {
    let (address, _database, _) = serving().await;
    for (path, action) in [("/ops/sign-in", "sign-in"), ("/ops/enroll", "enroll")] {
        let page = get(address, path, "").await;
        for absent in ["role=\"note\"", "Runs on"] {
            assert!(!page.body.contains(absent), "{absent} in {}", page.body);
        }
        for shown in [
            format!("<tetsu-passkey action=\"{action}\""),
            "viewBox=\"0 0 564 144\"".to_string(),
            "disabled:opacity-50 rounded-full font-medium focus-visible:outline-2".to_string(),
        ] {
            assert!(page.body.contains(&shown), "{shown} in {}", page.body);
        }
    }
}

/// A passkey on one device is listed as one, not as synced.
#[tokio::test]
async fn a_passkey_on_one_device_is_listed_as_one() {
    let (address, database, cookie, _) = signed_in().await;
    as_admin(
        &database,
        "UPDATE identity.passkeys SET passkey = jsonb_set(passkey, '{cred,backup_state}', 'false')",
    )
    .await;
    let page = get(address, "/ops", &cookie).await;
    assert!(page.body.contains("Passkey on one device"), "{}", page.body);
}

/// A request before sign-in is read only if it comes from the site, as both its origin and, when sent, its
/// `Sec-Fetch-Site` say, and only as a JSON object; each endpoint refuses alike.
#[tokio::test]
async fn requests_before_sign_in_come_from_the_site_as_json() {
    let (address, _database, _) = serving().await;
    let json = ("Content-Type", "application/json");
    for path in [
        "/ops/sign-in/begin",
        "/ops/sign-in/finish",
        "/ops/enroll/begin",
        "/ops/enroll/finish",
    ] {
        for headers in [
            vec![
                ("Origin", "https://evil.example"),
                ("Sec-Fetch-Site", "cross-site"),
                json,
            ],
            vec![("Origin", ORIGIN), ("Sec-Fetch-Site", "cross-site"), json],
            vec![("Sec-Fetch-Site", "same-origin"), json],
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
        async move { send(address, "POST", "/ops/sign-in/begin", &headers, body).await }
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
            started.json()["options"]["publicKey"].is_object()
        ),
        (200, Some("no-store"), true)
    );
}

/// A ceremony the use case refuses gets its one refusal, the same whatever was wrong.
#[tokio::test]
async fn refused_ceremonies_get_their_one_refusal() {
    let (address, _database, _) = serving().await;
    let said = [
        (
            "/ops/sign-in/finish",
            json!({"ceremony": 1}),
            "this sign-in could not be completed",
        ),
        (
            "/ops/enroll/begin",
            json!({"link": "not a link"}),
            "this enrollment link has ended or is not valid",
        ),
        (
            "/ops/enroll/finish",
            json!({}),
            "this enrollment link has ended or is not valid",
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

/// A signed-in change needs the session's form token from the site, and a live session: without the token it is
/// forbidden, once signed out it is unauthorized, and it is read only as JSON. Signing out from elsewhere is
/// forbidden and ends nothing.
#[tokio::test]
async fn signed_in_changes_need_the_session_and_its_form_token() {
    let (address, _database, cookie, form) = signed_in().await;
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
    let refused = sign_out(address, "https://evil.example", &cookie, &form).await;
    assert_eq!(refused.status, 403);
    assert_eq!(
        get(address, "/ops", &cookie).await.status,
        200,
        "still signed in"
    );
    sign_out(address, ORIGIN, &cookie, &form).await;
    let ended = post(
        address,
        "/ops/passkeys/begin",
        &cookie,
        &json!({ "form": form }),
    )
    .await;
    assert_eq!(
        (ended.status, ended.json()),
        (401, json!({"error": "you are signed out; sign in again"}))
    );
}

/// A failure of the server is unavailable to the browser, which learns nothing more, and reported to the operator.
#[tokio::test]
async fn failures_of_the_server_are_unavailable() {
    let revoke = |function: &str| {
        format!("REVOKE EXECUTE ON FUNCTION identity.{function} FROM tetsu_identity")
    };
    let (address, database, cookie, form) = signed_in().await;
    as_admin(&database, &revoke("account(bytea)")).await;
    let page = get(address, "/ops", &cookie).await;
    assert_eq!(
        (page.status, page.header("cache-control")),
        (503, Some("no-store"))
    );
    as_admin(&database, &revoke("end_session(bytea, text)")).await;
    let out = sign_out(address, ORIGIN, &cookie, &form).await;
    assert_eq!(out.status, 503);
    as_admin(&database, &revoke("use_session(bytea, text)")).await;
    assert_eq!(get(address, "/ops", &cookie).await.status, 503);
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
    as_admin(&database, &revoke("begin_sign_in(bytea, jsonb)")).await;
    let refused = post(address, "/ops/sign-in/begin", "", &json!({})).await;
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
/// their shape: registering with a passkey held, and signing in.
#[tokio::test]
async fn the_scripts_fixture_has_the_options_shapes() {
    let (address, _database, cookie, form) = signed_in().await;
    let add = post(
        address,
        "/ops/passkeys/begin",
        &cookie,
        &json!({ "form": form }),
    )
    .await;
    let sign = post(address, "/ops/sign-in/begin", "", &json!({})).await;
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../web/src/passkey/fixtures/options.json"
    ))
    .expect("JSON");
    assert_eq!(
        shape(&json!({"creation": add.json()["options"], "request": sign.json()["options"]})),
        shape(&fixture)
    );
}
