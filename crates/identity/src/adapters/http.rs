//! Identity's inbound HTTP adapter: the operator pages at `/ops`, and the JSON endpoints their passkey script calls
//! (ADR-0048). Every state-changing request is checked through [`Origins::admit`], and once signed in through
//! [`Operators::session_for_change`], so no other site can act through a signed-in browser.

use std::sync::Arc;

use askama::Template;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, HeaderName, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::{Value, json};

use crate::browser::{Change, ForChange, Origins, SignOut};
use crate::domain::{Actor, Answer, Ceremony, Device, Error, HeldPasskey, Realm, SessionToken};
use crate::passkeys::{RelyingParty, base64url};
use crate::ports::OperatorStore;
use crate::use_cases::Operators;

const ACCOUNT: &str = "/ops";
const SIGN_IN: &str = "/ops/sign-in";
const SIGN_IN_BEGIN: &str = "/ops/sign-in/begin";
const SIGN_IN_FINISH: &str = "/ops/sign-in/finish";
const ENROLL: &str = "/ops/enroll";
const ENROLL_BEGIN: &str = "/ops/enroll/begin";
const ENROLL_FINISH: &str = "/ops/enroll/finish";
const ADD_BEGIN: &str = "/ops/passkeys/begin";
const ADD_FINISH: &str = "/ops/passkeys/finish";
const SIGN_OUT: &str = "/ops/sign-out";

/// Every path Identity serves, which the composition root checks each start with a name no tenant may hold.
pub const PATHS: [&str; 10] = [
    ACCOUNT,
    SIGN_IN,
    SIGN_IN_BEGIN,
    SIGN_IN_FINISH,
    ENROLL,
    ENROLL_BEGIN,
    ENROLL_FINISH,
    ADD_BEGIN,
    ADD_FINISH,
    SIGN_OUT,
];

/// The operator pages' policy: the site's, with Tetsu's own script, its calls back to this origin, and forms
/// posted here allowed, and still no inline script or style (ADR-0029).
const POLICY: &str = "default-src 'none'; object-src 'none'; script-src 'self'; connect-src 'self'; \
                      style-src 'self'; font-src 'self'; img-src 'self'; base-uri 'none'; form-action 'self'; \
                      frame-ancestors 'none'";

/// The largest body any endpoint reads. A passkey's answer is a few kilobytes; enrolling stores none over 8 KiB.
const LARGEST_BODY: usize = 64 * 1024;

/// What a refused request is told when nothing more may be said.
const REFUSED: &str = "this request was refused";
const SIGNED_OUT: &str = "you are signed out; sign in again";
const UNAVAILABLE: &str = "signing in is unavailable just now; try again in a minute";

/// What the pages need from the composition root: the relying party and the origins state-changing requests
/// must come from, whether this is the demo, the shared stylesheet's and the passkey script's URLs, and where a
/// failure the operator should see is reported (#27).
pub struct Web {
    pub party: RelyingParty,
    pub origins: Origins,
    pub demo: bool,
    pub stylesheet: &'static str,
    pub script: &'static str,
    pub report: fn(&str),
}

#[derive(Template)]
#[template(path = "sign_in.html")]
struct SignInPage {
    demo: bool,
    stylesheet: &'static str,
    script: &'static str,
}

#[derive(Template)]
#[template(path = "enroll.html")]
struct EnrollPage {
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
    name: String,
    user: String,
    credentials: String,
    passkeys: Vec<&'static str>,
    can_add: bool,
    form: String,
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
        .route(SIGN_IN, get(sign_in_page::<S>))
        .route(SIGN_IN_BEGIN, post(begin_sign_in::<S>))
        .route(SIGN_IN_FINISH, post(finish_sign_in::<S>))
        .route(ENROLL, get(enroll_page::<S>))
        .route(ENROLL_BEGIN, post(begin_enrollment::<S>))
        .route(ENROLL_FINISH, post(finish_enrollment::<S>))
        .route(ADD_BEGIN, post(begin_adding::<S>))
        .route(ADD_FINISH, post(finish_adding::<S>))
        .route(SIGN_OUT, post(sign_out::<S>))
        .layer(DefaultBodyLimit::max(LARGEST_BODY))
        .with_state(Arc::new(Served { operators, web }))
}

async fn sign_in_page<S>(State(served): Shared<S>) -> Response {
    let web = &served.web;
    page(&SignInPage {
        demo: web.demo,
        stylesheet: web.stylesheet,
        script: web.script,
    })
}

/// The enrollment page. Its link's token travels in the address's fragment, which no browser sends, so it never
/// reaches a log; the script reads it, then clears it from the address bar.
async fn enroll_page<S>(State(served): Shared<S>) -> Response {
    let web = &served.web;
    page(&EnrollPage {
        demo: web.demo,
        stylesheet: web.stylesheet,
        script: web.script,
    })
}

/// The signed-in operator's page: their name, their passkeys, and a form token for what they may do next. A
/// browser with no live session is sent to sign in.
async fn account<S: OperatorStore>(State(served): Shared<S>, headers: HeaderMap) -> Response {
    let cookies = cookies(&headers);
    let Some(token) = Realm::Operator
        .token_in(cookies.iter().copied())
        .and_then(SessionToken::parse)
    else {
        return see_other(SIGN_IN);
    };
    match shown(&served, &cookies, &token).await {
        Ok(Some(page)) => page,
        Ok(None) => see_other(SIGN_IN),
        Err(error) => {
            (served.web.report)(&format!(
                "identity: could not show an operator's page: {error}"
            ));
            let web = &served.web;
            let page = UnavailablePage {
                demo: web.demo,
                stylesheet: web.stylesheet,
            };
            (StatusCode::SERVICE_UNAVAILABLE, self::page(&page)).into_response()
        }
    }
}

async fn shown<S: OperatorStore>(
    served: &Served<S>,
    cookies: &[&str],
    token: &SessionToken,
) -> Result<Option<Response>, Error> {
    let Some(session) = served.operators.session(cookies.iter().copied()).await? else {
        return Ok(None);
    };
    // A session found live names an enabled operator, so an account is missing only if they were disabled since.
    let account = served.operators.account(&session).await?;
    let web = &served.web;
    token.form_token().map(|form| {
        account.map(|account| {
            page(&AccountPage {
                demo: web.demo,
                stylesheet: web.stylesheet,
                script: web.script,
                name: account.name,
                user: base64url(&account.user_handle),
                credentials: account
                    .passkeys
                    .iter()
                    .map(|passkey| base64url(&passkey.credential))
                    .collect::<Vec<_>>()
                    .join(" "),
                can_add: account.passkeys.len() < 2,
                passkeys: account.passkeys.iter().map(described).collect(),
                form,
            })
        })
    })
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

async fn begin_sign_in<S: OperatorStore>(
    State(served): Shared<S>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(refused) = admitted(&served.web, &headers, &body) {
        return *refused;
    }
    let begun = served.operators.begin_sign_in(&served.web.party).await;
    answered(&served.web, begun.map(|ceremony| begun_json(&ceremony)))
}

async fn finish_sign_in<S: OperatorStore>(
    State(served): Shared<S>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let sent = match admitted(&served.web, &headers, &body) {
        Ok(sent) => sent,
        Err(refused) => return *refused,
    };
    let (ceremony, response) = answer_in(&sent);
    let answer = Answer {
        ceremony,
        response: &response,
    };
    let finished = served
        .operators
        .finish_sign_in(
            &served.web.party,
            answer,
            &cookies(&headers),
            &browser(&headers, None),
        )
        .await
        .map(|token| {
            let cookie = Realm::Operator.session_cookie(&token);
            ([(header::SET_COOKIE, cookie)], next(ACCOUNT)).into_response()
        });
    answered(&served.web, finished)
}

async fn begin_enrollment<S: OperatorStore>(
    State(served): Shared<S>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let sent = match admitted(&served.web, &headers, &body) {
        Ok(sent) => sent,
        Err(refused) => return *refused,
    };
    let link = sent["link"].as_str().unwrap_or_default();
    let begun = served
        .operators
        .begin_enrollment(&served.web.party, link)
        .await;
    answered(&served.web, begun.map(|ceremony| begun_json(&ceremony)))
}

async fn finish_enrollment<S: OperatorStore>(
    State(served): Shared<S>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let sent = match admitted(&served.web, &headers, &body) {
        Ok(sent) => sent,
        Err(refused) => return *refused,
    };
    let (ceremony, response) = answer_in(&sent);
    let finished = served
        .operators
        .finish_enrollment(
            &served.web.party,
            ceremony,
            &response,
            &browser(&headers, None),
        )
        .await
        .map(|_| next(SIGN_IN));
    answered(&served.web, finished)
}

async fn begin_adding<S: OperatorStore>(
    State(served): Shared<S>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let sent = match json_in(&headers, &body) {
        Ok(sent) => sent,
        Err(refused) => return *refused,
    };
    let checked = match acting(&served, &headers, &sent).await {
        Ok(checked) => checked,
        Err(refused) => return *refused,
    };
    let begun = served
        .operators
        .begin_adding_passkey(&served.web.party, &checked)
        .await;
    answered(&served.web, begun.map(|ceremony| begun_json(&ceremony)))
}

async fn finish_adding<S: OperatorStore>(
    State(served): Shared<S>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let sent = match json_in(&headers, &body) {
        Ok(sent) => sent,
        Err(refused) => return *refused,
    };
    let checked = match acting(&served, &headers, &sent).await {
        Ok(checked) => checked,
        Err(refused) => return *refused,
    };
    let (ceremony, response) = answer_in(&sent);
    let answer = Answer {
        ceremony,
        response: &response,
    };
    let actor = browser(&headers, Some(checked.session().operator()));
    let finished = served
        .operators
        .finish_adding_passkey(&served.web.party, &checked, answer, &actor)
        .await
        .map(|()| next(ACCOUNT));
    answered(&served.web, finished)
}

/// Signs out from the operator's page's form, with its form token, and sends the browser to sign in with its
/// cookie cleared, whether or not the session was still live.
async fn sign_out<S: OperatorStore>(
    State(served): Shared<S>,
    headers: HeaderMap,
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
        Ok(SignOut::Refused) => StatusCode::FORBIDDEN.into_response(),
        Ok(SignOut::Ended | SignOut::NothingToEnd) => (
            [(header::SET_COOKIE, Realm::Operator.cleared_cookie())],
            see_other(SIGN_IN),
        )
            .into_response(),
        Err(error) => unavailable(&served.web, &error),
    }
}

/// The live session a signed-in request acts for, or the answer it gets instead: forbidden when it comes from
/// elsewhere or without its session's form token, and unauthorized when its session has ended.
async fn acting<S: OperatorStore>(
    served: &Served<S>,
    headers: &HeaderMap,
    sent: &Value,
) -> Result<crate::CheckedSession, Box<Response>> {
    let change = change(headers, sent["form"].as_str().unwrap_or_default());
    match served
        .operators
        .session_for_change(&served.web.origins, &change)
        .await
    {
        Ok(ForChange::Acting(checked)) => Ok(checked),
        Ok(ForChange::NoSession) => Err(Box::new(refusal(StatusCode::UNAUTHORIZED, SIGNED_OUT))),
        Ok(ForChange::Refused) => Err(Box::new(refusal(StatusCode::FORBIDDEN, REFUSED))),
        Err(error) => Err(Box::new(unavailable(&served.web, &error))),
    }
}

/// A request before sign-in, which has no session to check: its JSON, if it comes from one of the realm's
/// origins and says so in both ways a browser can (ADR-0059), or the answer it gets instead.
fn admitted(web: &Web, headers: &HeaderMap, body: &Bytes) -> Result<Value, Box<Response>> {
    let origin = text(headers, &header::ORIGIN);
    let site = text(headers, &SEC_FETCH_SITE);
    if web.origins.admit(origin, site) {
        json_in(headers, body)
    } else {
        Err(Box::new(refusal(StatusCode::FORBIDDEN, REFUSED)))
    }
}

const SEC_FETCH_SITE: HeaderName = HeaderName::from_static("sec-fetch-site");

/// The JSON object a request sends, if it says it sends JSON. Anything else is refused before it is read: a form
/// or plain text, which a page on another site can send without asking, is unsupported.
fn json_in(headers: &HeaderMap, body: &Bytes) -> Result<Value, Box<Response>> {
    let is_json = text(headers, &header::CONTENT_TYPE).is_some_and(|kind| {
        kind.split(';')
            .next()
            .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("application/json"))
    });
    if !is_json {
        return Err(Box::new(refusal(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "send JSON",
        )));
    }
    serde_json::from_slice::<Value>(body)
        .ok()
        .filter(Value::is_object)
        .ok_or_else(|| Box::new(refusal(StatusCode::BAD_REQUEST, REFUSED)))
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
    ];
    (headers, body).into_response()
}

fn see_other(path: &'static str) -> Response {
    (StatusCode::SEE_OTHER, [(header::LOCATION, path)]).into_response()
}

/// A state-changing request, as Identity's checks read it.
fn change<'a>(headers: &'a HeaderMap, form: &'a str) -> Change<'a> {
    Change::new(
        cookies(headers),
        text(headers, &header::ORIGIN),
        text(headers, &SEC_FETCH_SITE),
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
