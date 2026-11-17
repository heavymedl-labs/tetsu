//! Identity's inbound HTTP adapter: the operator pages at `/ops`, and the JSON endpoints their passkey script calls
//! (ADR-0048). Every state-changing request is checked through [`Origins::admit`], and once logged in through
//! [`Operators::session_for_change`], so no other site can act through a logged-in browser.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use askama::Template;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{MethodRouter, get, post};
use serde_json::{Value, json};

use crate::browser::{Change, ForChange, Logout, Origins, return_path};
use crate::domain::{
    Actor, Answer, Ceremony, Device, Error, HeldPasskey, Realm, Session, SessionToken, TimeLeft,
};
use crate::passkeys::{RelyingParty, base64url};
use crate::ports::OperatorStore;
use crate::use_cases::Operators;

const ACCOUNT: &str = "/ops";
const LOGIN: &str = "/ops/login";
const LOGIN_BEGIN: &str = "/ops/login/begin";
const LOGIN_FINISH: &str = "/ops/login/finish";
const JOIN: &str = "/ops/join";
const JOIN_BEGIN: &str = "/ops/join/begin";
const JOIN_FINISH: &str = "/ops/join/finish";
const ADD_BEGIN: &str = "/ops/passkeys/begin";
const ADD_FINISH: &str = "/ops/passkeys/finish";
const LOGOUT: &str = "/ops/logout";
/// How long the browser's session has left (GET), and staying logged in (POST), for the session island.
const SESSION: &str = "/ops/session";
/// Any other path under `/ops`: a login for anyone logged out, and not found for an operator.
const ELSEWHERE: &str = "/ops/{*rest}";

/// Every path Identity serves, which the composition root checks each start with a name no tenant may hold.
pub const PATHS: [&str; 12] = [
    ACCOUNT,
    LOGIN,
    LOGIN_BEGIN,
    LOGIN_FINISH,
    JOIN,
    JOIN_BEGIN,
    JOIN_FINISH,
    ADD_BEGIN,
    ADD_FINISH,
    LOGOUT,
    SESSION,
    ELSEWHERE,
];

/// The operator pages' policy: the site's, with Tetsu's own script, its calls back to this origin, and forms
/// posted here allowed, and still no inline script or style (ADR-0029).
const POLICY: &str = "default-src 'none'; object-src 'none'; script-src 'self'; connect-src 'self'; \
                      style-src 'self'; font-src 'self'; img-src 'self'; base-uri 'none'; form-action 'self'; \
                      frame-ancestors 'none'";

/// The operator pages' referrer policy. Under the site's `no-referrer`, a browser sends `Origin: null` with a
/// posted form, such as logging out, which the origin check must refuse; `same-origin` sends the real origin, and a
/// referrer, to Tetsu alone and nothing to any other site (Fetch, "append a request `Origin` header").
const REFERRER: &str = "same-origin";

/// The largest body any endpoint reads. A passkey's answer is a few kilobytes; joining stores none over 8 KiB.
const LARGEST_BODY: usize = 64 * 1024;

/// What a refused request is told when nothing more may be said.
const REFUSED: &str = "this request was refused";
const LOGGED_OUT: &str = "you are logged out; log in again";
const UNAVAILABLE: &str = "logging in is unavailable just now; try again in a minute";

/// What the pages need from the composition root: the relying party and the origins state-changing requests
/// must come from, whether this is the demo, the shared stylesheet's URL, the passkey script's, which every operator
/// page loads, and the session script's, which a logged-in operator's pages load too, and where a failure the
/// operator should see is reported (#27).
pub struct Web {
    pub party: RelyingParty,
    pub origins: Origins,
    pub demo: bool,
    pub stylesheet: &'static str,
    pub script: &'static str,
    pub session_script: &'static str,
    pub report: fn(&str),
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginPage<'a> {
    demo: bool,
    stylesheet: &'static str,
    script: &'static str,
    /// The checked page a login returns to, or empty for the operator's own.
    next: &'a str,
    /// Whether the session ended on its own, which the page says.
    ended: bool,
}

#[derive(Template)]
#[template(path = "join.html")]
struct JoinPage {
    demo: bool,
    stylesheet: &'static str,
    script: &'static str,
}

#[derive(Template)]
#[template(path = "account.html")]
struct AccountPage {
    demo: bool,
    stylesheet: &'static str,
    script: &'static str,
    session_script: &'static str,
    name: String,
    user: String,
    credentials: String,
    passkeys: Vec<&'static str>,
    can_add: bool,
    fresh: bool,
    form: String,
}

#[derive(Template)]
#[template(path = "not_found.html")]
struct NotFoundPage {
    demo: bool,
    stylesheet: &'static str,
    script: &'static str,
    session_script: &'static str,
    form: String,
}

#[derive(Template)]
#[template(path = "refused.html")]
struct RefusedPage {
    demo: bool,
    stylesheet: &'static str,
}

#[derive(Template)]
#[template(path = "unavailable.html")]
struct UnavailablePage {
    demo: bool,
    stylesheet: &'static str,
}

struct Served<S> {
    operators: Operators<S>,
    web: Web,
}

type Shared<S> = State<Arc<Served<S>>>;

/// Identity's routes. They set no fallback, which is the site's alone (#90).
pub fn router<S>(operators: Operators<S>, web: Web) -> Router
where
    S: OperatorStore + Send + Sync + 'static,
{
    Router::new()
        .route(ACCOUNT, get(account::<S>))
        .route(LOGIN, get(login_page::<S>))
        .route(LOGIN_BEGIN, before_route::<S>(Before::BeginLogin))
        .route(LOGIN_FINISH, before_route::<S>(Before::FinishLogin))
        .route(JOIN, get(join_page::<S>))
        .route(JOIN_BEGIN, before_route::<S>(Before::BeginJoin))
        .route(JOIN_FINISH, before_route::<S>(Before::FinishJoin))
        .route(ADD_BEGIN, logged_in_route::<S>(LoggedIn::BeginAdding))
        .route(ADD_FINISH, logged_in_route::<S>(LoggedIn::FinishAdding))
        .route(LOGOUT, post(logout::<S>))
        .route(
            SESSION,
            get(time_left::<S>).merge(logged_in_route::<S>(LoggedIn::Stay)),
        )
        .route(ELSEWHERE, get(elsewhere::<S>))
        .layer(DefaultBodyLimit::max(LARGEST_BODY))
        .with_state(Arc::new(Served { operators, web }))
}

fn before_route<S: OperatorStore + Send + Sync + 'static>(
    step: Before,
) -> MethodRouter<Arc<Served<S>>> {
    post(
        move |State(served): Shared<S>, headers: HeaderMap, body: Bytes| {
            before(served, headers, body, step)
        },
    )
}

fn logged_in_route<S: OperatorStore + Send + Sync + 'static>(
    step: LoggedIn,
) -> MethodRouter<Arc<Served<S>>> {
    post(
        move |State(served): Shared<S>, headers: HeaderMap, body: Bytes| {
            logged_in(served, headers, body, step)
        },
    )
}

/// The login page. An operator already logged in is sent on, to the checked page it would return to or their own,
/// since logging in again would only replace their session.
async fn login_page<S: OperatorStore>(
    State(served): Shared<S>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let web = &served.web;
    let next = query(&uri, "next").and_then(return_path);
    match served.operators.session(cookies(&headers)).await {
        Ok(Some(_)) => see_other(next.unwrap_or(ACCOUNT)),
        Ok(None) => {
            let ended = query(&uri, "ended") == Some("1");
            let mut shown = page(&LoginPage {
                demo: web.demo,
                stylesheet: web.stylesheet,
                script: web.script,
                next: next.unwrap_or_default(),
                ended,
            });
            // A session that ended on its own leaves nothing of the pages behind either, as logging out does
            // (ASVS 14.3.1).
            if ended {
                shown
                    .headers_mut()
                    .insert(CLEAR_SITE_DATA, HeaderValue::from_static("\"cache\""));
            }
            shown
        }
        Err(error) => unavailable_page(web, "could not show the login page", &error),
    }
}

/// The join page. Its link's token travels in the address's fragment, which no browser sends, so it never
/// reaches a log; the script reads it, then clears it from the address bar.
async fn join_page<S>(State(served): Shared<S>) -> Response {
    let web = &served.web;
    page(&JoinPage {
        demo: web.demo,
        stylesheet: web.stylesheet,
        script: web.script,
    })
}

/// The logged-in operator's page: their name, their passkeys, and a form token for what they may do next. A
/// browser with no live session is sent to log in.
async fn account<S: OperatorStore>(State(served): Shared<S>, headers: HeaderMap) -> Response {
    match shown(&served, &headers).await {
        Ok(Some(page)) => page,
        Ok(None) => see_other(LOGIN),
        Err(error) => unavailable_page(&served.web, "could not show an operator's page", &error),
    }
}

/// A path under `/ops` that Identity does not serve. Anyone without a live session is sent to log in, returning here
/// after, as at `/ops`, so the operator pages cannot be mapped from outside; an operator is told it is not found.
async fn elsewhere<S: OperatorStore>(
    State(served): Shared<S>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let web = &served.web;
    match viewing(&served, &headers).await {
        Ok(Some((_, form))) => (
            StatusCode::NOT_FOUND,
            page(&NotFoundPage {
                demo: web.demo,
                stylesheet: web.stylesheet,
                script: web.script,
                session_script: web.session_script,
                form,
            }),
        )
            .into_response(),
        Ok(None) => see_other(&return_to(uri.path())),
        Err(error) => unavailable_page(web, "could not answer an unknown operator path", &error),
    }
}

/// The live session a page's request comes with, which the request renews, and a form token for the page to carry;
/// none for a request with no live session.
async fn viewing<S: OperatorStore>(
    served: &Served<S>,
    headers: &HeaderMap,
) -> Result<Option<(Session, String)>, Error> {
    let cookies = cookies(headers);
    let Some(token) = Realm::Operator
        .token_in(cookies.iter().copied())
        .and_then(SessionToken::parse)
    else {
        return Ok(None);
    };
    let Some(session) = served.operators.session(cookies.iter().copied()).await? else {
        return Ok(None);
    };
    token.form_token().map(|form| Some((session, form)))
}

async fn shown<S: OperatorStore>(
    served: &Served<S>,
    headers: &HeaderMap,
) -> Result<Option<Response>, Error> {
    let Some((session, form)) = viewing(served, headers).await? else {
        return Ok(None);
    };
    // A session found live names an enabled operator, so an account is missing only if they were disabled since.
    let account = served.operators.account(&session).await?;
    let web = &served.web;
    Ok(account.map(|account| {
        page(&AccountPage {
            demo: web.demo,
            stylesheet: web.stylesheet,
            script: web.script,
            session_script: web.session_script,
            name: account.name,
            user: base64url(&account.user_handle),
            credentials: account
                .passkeys
                .iter()
                .map(|passkey| base64url(&passkey.credential))
                .collect::<Vec<_>>()
                .join(" "),
            can_add: account.passkeys.len() < 2,
            fresh: fresh(session.proven_at(), SystemTime::now()),
            passkeys: account.passkeys.iter().map(described).collect(),
            form,
        })
    }))
}

/// How long the browser's session has left, read without renewing it, for the session island's clock: only the
/// person's own requests renew a session (NIST SP 800-63B-4).
async fn time_left<S: OperatorStore>(State(served): Shared<S>, headers: HeaderMap) -> Response {
    let left = served.operators.time_left(cookies(&headers)).await;
    answered(&served.web, left.map(left_json))
}

/// A session's time left as the island reads it, in whole seconds to each limit, or the answer to a session that
/// has ended.
fn left_json(left: Option<TimeLeft>) -> Response {
    left.map_or_else(
        || refusal(StatusCode::UNAUTHORIZED, LOGGED_OUT),
        |left| {
            json_out(
                StatusCode::OK,
                &json!({ "idle": left.idle, "absolute": left.absolute }),
            )
        },
    )
}

/// How long after logging in an operator may add a passkey, which the database decides (ADR-0048, `v5.0.0-7.5.1`).
/// The page uses it only to offer the button, or say why it does not.
const FRESH: Duration = Duration::from_mins(10);

/// Whether a session a passkey last proved at `proven` is recent enough at `now` to add a passkey: under ten minutes
/// since. A session never proven, as one joining began, never is (ADR-0069). A proof this server's clock puts in the
/// future counts as fresh, and the database refuses it if the clocks disagree.
pub(crate) fn fresh(proven: Option<SystemTime>, now: SystemTime) -> bool {
    proven.is_some_and(|proven| now.duration_since(proven).map_or(true, |age| age < FRESH))
}

/// How the operator's page lists a passkey: whether it is synced to their other devices, as the authenticator
/// last said.
fn described(passkey: &HeldPasskey) -> &'static str {
    if passkey.backup_state {
        "Synced passkey"
    } else {
        "Passkey on one device"
    }
}

/// A step of a ceremony before login, which has no session to check.
#[derive(Clone, Copy)]
enum Before {
    BeginLogin,
    FinishLogin,
    BeginJoin,
    FinishJoin,
}

/// A step a logged-in operator takes, with the page's form token.
#[derive(Clone, Copy)]
enum LoggedIn {
    BeginAdding,
    FinishAdding,
    /// Staying logged in: the checks for a change have renewed the session, so this says how long it has now.
    Stay,
}

/// Runs a step before login, once the request is admitted: begins or finishes logging in or joining.
async fn before<S: OperatorStore>(
    served: Arc<Served<S>>,
    headers: HeaderMap,
    body: Bytes,
    step: Before,
) -> Result<Response, Refused> {
    let sent = admitted(&served.web, &headers, &body)?;
    let (web, operators) = (&served.web, &served.operators);
    let (ceremony, response) = answer_in(&sent);
    let answer = Answer {
        ceremony,
        response: &response,
    };
    let actor = browser(&headers, None);
    let answered_with = match step {
        Before::BeginLogin => operators
            .begin_login(&web.party)
            .await
            .map(|begun| begun_json(&begun)),
        Before::FinishLogin => operators
            .finish_login(&web.party, answer, &cookies(&headers), &actor)
            .await
            .map(|token| {
                let to = sent["next"].as_str().and_then(return_path);
                logged_in_at(&token, to.unwrap_or(ACCOUNT))
            }),
        Before::BeginJoin => operators
            .begin_join(&web.party, sent["link"].as_str().unwrap_or_default())
            .await
            .map(|begun| begun_json(&begun)),
        // Joining logs the operator in, with a session that rests on the link and is never fresh (ADR-0069).
        Before::FinishJoin => operators
            .finish_join(&web.party, ceremony, &response, &cookies(&headers), &actor)
            .await
            .map(|(_, token)| logged_in_at(&token, ACCOUNT)),
    };
    Ok(answered(web, answered_with))
}

/// Runs a logged-in operator's step, once the request has passed the checks for a change, which renew the session:
/// begins or finishes adding a passkey, or stays logged in.
async fn logged_in<S: OperatorStore>(
    served: Arc<Served<S>>,
    headers: HeaderMap,
    body: Bytes,
    step: LoggedIn,
) -> Result<Response, Refused> {
    let sent = json_in(&headers, &body)?;
    let checked = acting(&served, &headers, &sent).await?;
    let (web, operators) = (&served.web, &served.operators);
    let answered_with = match step {
        LoggedIn::BeginAdding => operators
            .begin_adding_passkey(&web.party, &checked)
            .await
            .map(|begun| begun_json(&begun)),
        LoggedIn::FinishAdding => {
            let (ceremony, response) = answer_in(&sent);
            let answer = Answer {
                ceremony,
                response: &response,
            };
            let actor = browser(&headers, Some(checked.session().operator()));
            operators
                .finish_adding_passkey(&web.party, &checked, answer, &actor)
                .await
                .map(|()| next(ACCOUNT))
        }
        LoggedIn::Stay => operators.time_left(cookies(&headers)).await.map(left_json),
    };
    Ok(answered(web, answered_with))
}

/// Logs out from the operator's page's form, with its form token, and sends the browser to log in with its
/// cookie cleared, whether or not the session was still live, returning after to the checked page the form's address
/// names, as the session dialog's "Log in again" does. A form posts and shows pages, so a logout refused or failed
/// gets a page saying so, never a bare status or JSON.
async fn logout<S: OperatorStore>(
    State(served): Shared<S>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    let form = std::str::from_utf8(&body)
        .ok()
        .and_then(|body| body.split('&').find_map(|pair| pair.strip_prefix("form=")))
        .unwrap_or_default();
    let change = change(&headers, form);
    match served
        .operators
        .end_session(&served.web.origins, &change)
        .await
    {
        Ok(Logout::Refused) => {
            let web = &served.web;
            let refused = RefusedPage {
                demo: web.demo,
                stylesheet: web.stylesheet,
            };
            (StatusCode::FORBIDDEN, page(&refused)).into_response()
        }
        // The browser also drops what it cached of the pages, so Back shows none of them. Only the cache: clearing
        // cookies would end its sessions in every realm on the host.
        Ok(Logout::Ended | Logout::NothingToEnd) => (
            [
                (header::SET_COOKIE, Realm::Operator.cleared_cookie()),
                (CLEAR_SITE_DATA, "\"cache\"".to_string()),
            ],
            see_other(&return_to(query(&uri, "next").unwrap_or_default())),
        )
            .into_response(),
        Err(error) => unavailable_page(&served.web, "could not log an operator out", &error),
    }
}

/// A page that says logging in is unavailable just now, for a request a person made by following a link or posting
/// a form, with the detail reported to the operator, never shown.
fn unavailable_page(web: &Web, what: &str, error: &Error) -> Response {
    (web.report)(&format!("identity: {what}: {error}"));
    let unavailable = UnavailablePage {
        demo: web.demo,
        stylesheet: web.stylesheet,
    };
    (StatusCode::SERVICE_UNAVAILABLE, page(&unavailable)).into_response()
}

/// The live session a logged-in request acts for, or the answer it gets instead: forbidden when it comes from
/// elsewhere or without its session's form token, and unauthorized when its session has ended.
async fn acting<S: OperatorStore>(
    served: &Served<S>,
    headers: &HeaderMap,
    sent: &Value,
) -> Result<crate::CheckedSession, Refused> {
    let change = change(headers, sent["form"].as_str().unwrap_or_default());
    match served
        .operators
        .session_for_change(&served.web.origins, &change)
        .await
    {
        Ok(ForChange::Acting(checked)) => Ok(checked),
        Ok(ForChange::NoSession) => {
            Err(Refused::with(refusal(StatusCode::UNAUTHORIZED, LOGGED_OUT)))
        }
        Ok(ForChange::Refused) => Err(Refused::with(refusal(StatusCode::FORBIDDEN, REFUSED))),
        Err(error) => Err(Refused::with(unavailable(&served.web, &error))),
    }
}

/// A request before login, which has no session to check: its JSON, if it comes from one of the realm's
/// origins and says so in both ways a browser can (ADR-0059), or the answer it gets instead.
fn admitted(web: &Web, headers: &HeaderMap, body: &Bytes) -> Result<Value, Refused> {
    let origin = text(headers, &header::ORIGIN);
    let site = fetch_site(headers);
    if web.origins.admit(origin, site) {
        json_in(headers, body)
    } else {
        Err(Refused::with(refusal(StatusCode::FORBIDDEN, REFUSED)))
    }
}

/// The answer a request gets when a check refuses it before its use case runs. Boxed, since a response is large and
/// most requests pass.
struct Refused(Box<Response>);

impl Refused {
    fn with(response: Response) -> Self {
        Self(Box::new(response))
    }
}

impl IntoResponse for Refused {
    fn into_response(self) -> Response {
        *self.0
    }
}

const SEC_FETCH_SITE: HeaderName = HeaderName::from_static("sec-fetch-site");
const CLEAR_SITE_DATA: HeaderName = HeaderName::from_static("clear-site-data");

/// The request's `Sec-Fetch-Site`, if sent. One sent but not readable as text counts as an empty value, which no
/// check admits, never as one not sent, so a mangled header cannot stand in for an older browser's silence.
fn fetch_site(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(SEC_FETCH_SITE)
        .map(|value| value.to_str().unwrap_or_default())
}

/// The JSON object a request sends, if it says it sends JSON. Anything else is refused before it is read: a form
/// or plain text, which a page on another site can send without asking, is unsupported.
fn json_in(headers: &HeaderMap, body: &Bytes) -> Result<Value, Refused> {
    let is_json = text(headers, &header::CONTENT_TYPE).is_some_and(|kind| {
        kind.split(';')
            .next()
            .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("application/json"))
    });
    if !is_json {
        return Err(Refused::with(refusal(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "send JSON",
        )));
    }
    serde_json::from_slice::<Value>(body)
        .ok()
        .filter(Value::is_object)
        .ok_or_else(|| Refused::with(refusal(StatusCode::BAD_REQUEST, REFUSED)))
}

/// A ceremony's token and the browser's answer, as text, from what a request sent. Missing or of the wrong kind,
/// each reads as text the use case refuses, so every such answer gets that ceremony's one refusal.
fn answer_in(sent: &Value) -> (&str, String) {
    (
        sent["ceremony"].as_str().unwrap_or_default(),
        sent["response"].to_string(),
    )
}

/// A begun ceremony as the script reads it: the token to send back, and the options to hand the browser.
fn begun_json(ceremony: &Ceremony) -> Response {
    let options = serde_json::from_str::<Value>(&ceremony.options).unwrap_or_default();
    json_out(
        StatusCode::OK,
        &json!({"ceremony": ceremony.token.to_text(), "options": options}),
    )
}

/// A session's cookie for the browser, and where the script sends it next.
fn logged_in_at(token: &SessionToken, path: &str) -> Response {
    let cookie = Realm::Operator.session_cookie(token);
    ([(header::SET_COOKIE, cookie)], next(path)).into_response()
}

/// Where the script sends the browser once a ceremony is done.
fn next(path: &str) -> Response {
    json_out(StatusCode::OK, &json!({ "next": path }))
}

/// A use case's answer: its response, its one refusal, or unavailable when the server failed, which the operator
/// is told of through `report` and the browser only in general.
fn answered(web: &Web, result: Result<Response, Error>) -> Response {
    match result {
        Ok(response) => response,
        Err(Error::Invalid(why)) => refusal(StatusCode::BAD_REQUEST, &why),
        Err(error) => unavailable(web, &error),
    }
}

fn unavailable(web: &Web, error: &Error) -> Response {
    (web.report)(&format!("identity: a passkey request failed: {error}"));
    refusal(StatusCode::SERVICE_UNAVAILABLE, UNAVAILABLE)
}

fn refusal(status: StatusCode, why: &str) -> Response {
    json_out(status, &json!({ "error": why }))
}

/// JSON no cache keeps, since it may carry a ceremony's token or set a session.
fn json_out(status: StatusCode, body: &Value) -> Response {
    let headers = [
        (header::CONTENT_TYPE, "application/json"),
        (header::CACHE_CONTROL, "no-store"),
    ];
    (status, headers, body.to_string()).into_response()
}

/// Renders an operator page, under the operator pages' policy, kept by no cache, since it may carry a form token.
/// A compiled template fails only if writing fails, so a failure gets a bare unavailable status rather than half a
/// page.
fn page(page: &impl Template) -> Response {
    let body = page.render().or(Err(StatusCode::SERVICE_UNAVAILABLE));
    let headers = [
        (header::CONTENT_TYPE, "text/html; charset=utf-8"),
        (header::CACHE_CONTROL, "no-store"),
        (header::CONTENT_SECURITY_POLICY, POLICY),
        (header::REFERRER_POLICY, REFERRER),
    ];
    (headers, body).into_response()
}

fn see_other(path: &str) -> Response {
    (
        StatusCode::SEE_OTHER,
        [(header::LOCATION, path.to_string())],
    )
        .into_response()
}

/// The login page, asked to return to `path` when that is a page a login may return to.
fn return_to(path: &str) -> String {
    return_path(path).map_or_else(|| LOGIN.to_string(), |path| format!("{LOGIN}?next={path}"))
}

/// A value the address's query gives `name`, as written. Only values the pages check before use are read, and a
/// checked one needs no decoding.
fn query<'a>(uri: &'a Uri, name: &str) -> Option<&'a str> {
    uri.query()?.split('&').find_map(|pair| {
        pair.strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('='))
    })
}

/// A state-changing request, as Identity's checks read it.
fn change<'a>(headers: &'a HeaderMap, form: &'a str) -> Change<'a> {
    Change::new(
        cookies(headers),
        text(headers, &header::ORIGIN),
        fetch_site(headers),
        form,
    )
}

/// Every `Cookie` header, since HTTP/2 lets a browser split its cookies across several.
fn cookies(headers: &HeaderMap) -> Vec<&str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .collect()
}

/// A header's text, if it has one that is visible ASCII.
fn text<'a>(headers: &'a HeaderMap, name: &HeaderName) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

/// Who acts from this browser, as the audit record names them.
fn browser(headers: &HeaderMap, operator: Option<crate::OperatorId>) -> Actor {
    Actor::browser(
        operator,
        Device::from_user_agent(text(headers, &header::USER_AGENT)),
    )
}
