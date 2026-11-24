//! Identity's inbound HTTP adapter: the admin pages at `/admin`, and the JSON endpoints their passkey script calls
//! (ADR-0048). Every state-changing request is checked through [`Origins::admit`], and once logged in through
//! [`Admins::session_for_change`], so no other site can act through a logged-in browser.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use askama::Template;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Extension, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{MethodRouter, get, post};
use serde_json::{Value, json};

use crate::browser::{Change, ForChange, Logout, Origins, return_path};
use crate::domain::{
    Account, Actor, Answer, Ceremony, Device, Ending, Error, FailedCheck, HeldPasskey,
    ListedSession, RateLimit, Realm, Session, SessionId, SessionToken, TimeLeft,
};
use crate::passkeys::{RelyingParty, base64url};
use crate::ports::AdminStore;
use crate::use_cases::Admins;
use tetsu_kernel::{Mailbox, Requester, Scheme};

/// The admins' realm (ADR-0084), which sends them to their Profile until it has a home of its own (#157).
const HOME: &str = "/admin";
/// The admin's account (#156): who they are, and how they log in and where. Logging in lands on Profile.
const PROFILE: &str = "/admin/settings/profile";
const SECURITY: &str = "/admin/settings/security";
const LOGIN: &str = "/admin/login";
const LOGIN_BEGIN: &str = "/admin/login/begin";
const LOGIN_FINISH: &str = "/admin/login/finish";
const JOIN: &str = "/admin/join";
const JOIN_BEGIN: &str = "/admin/join/begin";
const JOIN_FINISH: &str = "/admin/join/finish";
const ADD_BEGIN: &str = "/admin/passkeys/begin";
const ADD_FINISH: &str = "/admin/passkeys/finish";
const LOGOUT: &str = "/admin/logout";
/// Ending the admin's other sessions, one by its id or every one (ADR-0059).
const END_SESSIONS: &str = "/admin/sessions/end";
/// How long the browser's session has left (GET), and staying logged in (POST), for the session island.
const SESSION: &str = "/admin/session";
/// The admins' mailbox: email kept rather than sent (ADR-0074).
const MAILBOX: &str = "/admin/mailbox";
/// Any other path under `/admin`: a login for anyone logged out, and not found for an admin.
const ELSEWHERE: &str = "/admin/{*rest}";

/// Every path Identity serves, which the composition root checks each start with a name no tenant may hold.
pub const PATHS: [&str; 16] = [
    HOME,
    PROFILE,
    SECURITY,
    LOGIN,
    LOGIN_BEGIN,
    LOGIN_FINISH,
    JOIN,
    JOIN_BEGIN,
    JOIN_FINISH,
    ADD_BEGIN,
    ADD_FINISH,
    LOGOUT,
    END_SESSIONS,
    SESSION,
    MAILBOX,
    ELSEWHERE,
];

/// The admin pages' policy: the site's, with Tetsu's own script, its calls back to this origin, and forms
/// posted here allowed, and still no inline script or style (ADR-0029).
const POLICY: &str = "default-src 'none'; object-src 'none'; script-src 'self'; connect-src 'self'; \
                      style-src 'self'; font-src 'self'; img-src 'self'; base-uri 'none'; form-action 'self'; \
                      frame-ancestors 'none'";

/// The admin pages' referrer policy. Under the site's `no-referrer`, a browser sends `Origin: null` with a
/// posted form, such as logging out, which the origin check must refuse; `same-origin` sends the real origin, and a
/// referrer, to Tetsu alone and nothing to any other site (Fetch, "append a request `Origin` header").
const REFERRER: &str = "same-origin";

/// The largest body any endpoint reads. A passkey's answer is a few kilobytes; joining stores none over 8 KiB.
const LARGEST_BODY: usize = 64 * 1024;

/// What a refused request is told when nothing more may be said.
const REFUSED: &str = "this request was refused";
const LOGGED_OUT: &str = "you are logged out; log in again";
const UNAVAILABLE: &str = "logging in is unavailable. Try again in a minute";

/// What the pages need from the composition root: the relying party and the origins state-changing requests
/// must come from, whether this is the demo, the shared stylesheet's URL, the passkey script's, which every admin
/// page loads, the session script's, which a logged-in admin's pages load too, and the color scheme control's,
/// and where a failure the admin should see is reported (#27).
pub struct Web {
    pub party: RelyingParty,
    pub origins: Origins,
    pub demo: bool,
    pub stylesheet: &'static str,
    pub script: &'static str,
    pub session_script: &'static str,
    /// The color scheme control's script, which every admin page loads (ADR-0070).
    pub scheme_script: &'static str,
    /// The settings shell's menu script, which keeps focus where it shows (#156).
    pub menu_script: &'static str,
    /// The messages Mail kept rather than sent, which the mailbox page shows (ADR-0074).
    pub mailbox: Arc<dyn Mailbox>,
    pub report: fn(&str),
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginPage<'a> {
    demo: bool,
    stylesheet: &'static str,
    script: &'static str,
    scheme_script: &'static str,
    /// The radio the color scheme control checks, from the visitor's cookie (ADR-0070).
    scheme: &'static str,
    /// The checked page a login returns to, or empty for the admin's own.
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
    scheme_script: &'static str,
    /// The radio the color scheme control checks, from the visitor's cookie (ADR-0070).
    scheme: &'static str,
}

#[derive(Template)]
#[template(path = "account_profile.html")]
struct ProfilePage {
    shell: Shell,
    /// The admin's email address, if they have one.
    email: Option<String>,
}

#[derive(Template)]
#[template(path = "account_security.html")]
struct SecurityPage {
    shell: Shell,
    user: String,
    credentials: String,
    passkeys: Vec<&'static str>,
    can_add: bool,
    fresh: bool,
    /// The admin's live sessions, the one asking first (ADR-0059).
    sessions: Vec<Shown>,
}

/// What every logged-in admin page holds (#148): the shared page's, and the admin the header names with the
/// form token its Log out posts. Each page's struct holds one and reads through it, so templates name its fields
/// directly.
struct Shell {
    demo: bool,
    stylesheet: &'static str,
    script: &'static str,
    scheme_script: &'static str,
    /// The radio the color scheme control checks, from the visitor's cookie (ADR-0070).
    scheme: &'static str,
    session_script: &'static str,
    menu_script: &'static str,
    name: String,
    /// The admin's initials, which the header's menu button shows.
    initials: String,
    form: String,
    /// The page's own path, which the menu marks as current when it is the account's.
    here: &'static str,
}

impl Shell {
    fn of(web: &Web, headers: &HeaderMap, name: String, form: String, here: &'static str) -> Self {
        Self {
            demo: web.demo,
            stylesheet: web.stylesheet,
            script: web.script,
            scheme_script: web.scheme_script,
            scheme: scheme_of(headers).as_str(),
            session_script: web.session_script,
            menu_script: web.menu_script,
            initials: initials(&name),
            name,
            form,
            here,
        }
    }
}

/// Lets a page's template read its shell's fields as its own.
macro_rules! through_shell {
    ($($page:ty),+) => {$(
        impl std::ops::Deref for $page {
            type Target = Shell;
            fn deref(&self) -> &Shell {
                &self.shell
            }
        }
    )+};
}

through_shell!(ProfilePage, SecurityPage, MailboxPage);

/// A session as the admin's page lists it: its id, which ending it names, its device, whether that is a phone,
/// which its icon shows, and how long ago it started and was last used.
struct Shown {
    id: String,
    device: String,
    phone: bool,
    started: String,
    last_seen: String,
    presented: bool,
}

impl Shown {
    fn of(listed: ListedSession, now: SystemTime) -> Self {
        let device = listed
            .device
            .unwrap_or_else(|| "Unknown device".to_string());
        Self {
            id: listed.id.as_str().to_string(),
            phone: device.ends_with(" on iOS") || device.ends_with(" on Android"),
            device,
            started: ago(listed.started_at, now),
            last_seen: ago(listed.last_seen_at, now),
            presented: listed.presented,
        }
    }
}

#[derive(Template)]
#[template(path = "mailbox.html")]
struct MailboxPage {
    shell: Shell,
    messages: Vec<Kept>,
}

/// A kept message as the mailbox lists it.
struct Kept {
    recipient: String,
    subject: String,
    text: String,
    ago: String,
}

#[derive(Template)]
#[template(path = "not_found.html")]
struct NotFoundPage {
    demo: bool,
    stylesheet: &'static str,
    /// The radio the site footer's color scheme control checks, from the visitor's cookie (ADR-0070).
    scheme: &'static str,
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
    admins: Admins<S>,
    web: Web,
}

type Shared<S> = State<Arc<Served<S>>>;

/// Identity's routes. They set no fallback, which is the site's alone (#90).
pub fn router<S>(admins: Admins<S>, web: Web) -> Router
where
    S: AdminStore + Send + Sync + 'static,
{
    Router::new()
        .route(HOME, get(home::<S>))
        .route(PROFILE, get(profile::<S>))
        .route(SECURITY, get(security::<S>))
        .route(LOGIN, get(login_page::<S>))
        .route(LOGIN_BEGIN, before_route::<S>(Before::BeginLogin))
        .route(LOGIN_FINISH, before_route::<S>(Before::FinishLogin))
        .route(JOIN, get(join_page::<S>))
        .route(JOIN_BEGIN, before_route::<S>(Before::BeginJoin))
        .route(JOIN_FINISH, before_route::<S>(Before::FinishJoin))
        .route(ADD_BEGIN, logged_in_route::<S>(LoggedIn::BeginAdding))
        .route(ADD_FINISH, logged_in_route::<S>(LoggedIn::FinishAdding))
        .route(LOGOUT, post(logout::<S>))
        .route(END_SESSIONS, post(end_sessions::<S>))
        .route(
            SESSION,
            get(time_left::<S>).merge(logged_in_route::<S>(LoggedIn::Stay)),
        )
        .route(MAILBOX, get(mailbox::<S>))
        .route(ELSEWHERE, get(elsewhere::<S>))
        .layer(DefaultBodyLimit::max(LARGEST_BODY))
        .with_state(Arc::new(Served { admins, web }))
}

fn before_route<S: AdminStore + Send + Sync + 'static>(
    step: Before,
) -> MethodRouter<Arc<Served<S>>> {
    post(
        move |State(served): Shared<S>,
              Extension(from): Extension<Requester>,
              headers: HeaderMap,
              body: Bytes| { before(served, from, headers, body, step) },
    )
}

fn logged_in_route<S: AdminStore + Send + Sync + 'static>(
    step: LoggedIn,
) -> MethodRouter<Arc<Served<S>>> {
    post(
        move |State(served): Shared<S>,
              Extension(from): Extension<Requester>,
              headers: HeaderMap,
              body: Bytes| { logged_in(served, from, headers, body, step) },
    )
}

/// The login page. An admin already logged in is sent on, to the checked page it would return to or their own,
/// since logging in again would only replace their session.
async fn login_page<S: AdminStore>(
    State(served): Shared<S>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let web = &served.web;
    let next = query(&uri, "next").and_then(return_path);
    match served.admins.session(cookies(&headers)).await {
        Ok(Some(_)) => see_other(next.unwrap_or(PROFILE)),
        Ok(None) => {
            let ended = query(&uri, "ended") == Some("1");
            let mut shown = page(&LoginPage {
                demo: web.demo,
                stylesheet: web.stylesheet,
                script: web.script,
                scheme_script: web.scheme_script,
                scheme: scheme_of(&headers).as_str(),
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
async fn join_page<S>(State(served): Shared<S>, headers: HeaderMap) -> Response {
    let web = &served.web;
    page(&JoinPage {
        demo: web.demo,
        stylesheet: web.stylesheet,
        script: web.script,
        scheme_script: web.scheme_script,
        scheme: scheme_of(&headers).as_str(),
    })
}

/// The admins' realm itself (ADR-0084): until it has a home (#157), it sends a logged-in admin to their
/// Profile, and anyone else to log in.
async fn home<S: AdminStore>(State(served): Shared<S>, headers: HeaderMap) -> Response {
    match served.admins.session(cookies(&headers)).await {
        Ok(Some(_)) => see_other(PROFILE),
        Ok(None) => see_other(LOGIN),
        Err(error) => unavailable_page(&served.web, "could not show an admin's page", &error),
    }
}

/// The admin's Profile (#156): their name, email address, and role. Anyone without a live session is sent to
/// log in, returning here after.
async fn profile<S: AdminStore>(State(served): Shared<S>, headers: HeaderMap) -> Response {
    let web = &served.web;
    let (account, form) = match viewer(&served, &headers).await {
        Ok(Some(viewer)) => viewer,
        Ok(None) => return see_other(&return_to(PROFILE)),
        Err(error) => return unavailable_page(web, "could not show an admin's profile", &error),
    };
    match served.admins.email(account.admin).await {
        Ok(email) => page(&ProfilePage {
            email: email.map(|email| email.as_str().to_string()),
            shell: Shell::of(web, &headers, account.name, form, PROFILE),
        }),
        Err(error) => unavailable_page(web, "could not show an admin's profile", &error),
    }
}

/// The admin's Security (#156): their sessions and passkeys. Anyone without a live session is sent to log in,
/// returning here after.
async fn security<S: AdminStore>(State(served): Shared<S>, headers: HeaderMap) -> Response {
    match shown(&served, &headers).await {
        Ok(Some(page)) => page,
        Ok(None) => see_other(&return_to(SECURITY)),
        Err(error) => unavailable_page(&served.web, "could not show an admin's security", &error),
    }
}

/// The admins' mailbox. Anyone without a live session is sent to log in, returning here after.
async fn mailbox<S: AdminStore>(
    State(served): Shared<S>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let web = &served.web;
    // A session found live names an enabled admin, so an account is missing only if they were disabled since.
    let (account, viewer) = match viewer(&served, &headers).await {
        Ok(Some(viewer)) => viewer,
        Ok(None) => return see_other(&return_to(uri.path())),
        Err(error) => return unavailable_page(web, "could not show the mailbox", &error),
    };
    let Ok(captured) = web.mailbox.captured().await else {
        let error = Error::Unavailable("mail's store failed; its log says why".into());
        return unavailable_page(web, "could not show the mailbox", &error);
    };
    let now = SystemTime::now();
    page(&MailboxPage {
        shell: Shell::of(web, &headers, account.name, viewer, MAILBOX),
        messages: captured
            .into_iter()
            .map(|message| Kept {
                ago: ago(message.at, now),
                recipient: message.recipient,
                subject: message.subject,
                text: message.text,
            })
            .collect(),
    })
}

/// The account of the live session a page's request comes with, and a form token for the page to carry; none for a
/// request with no live session, or whose admin was disabled since it was checked.
async fn viewer<S: AdminStore>(
    served: &Served<S>,
    headers: &HeaderMap,
) -> Result<Option<(Account, String)>, Error> {
    let Some((session, form)) = viewing(served, headers).await? else {
        return Ok(None);
    };
    let account = served.admins.account(&session).await?;
    Ok(account.map(|account| (account, form)))
}

/// A name's initials as the header's menu button shows them: the first letters of its first and last words, in
/// capitals, or one letter for a one-word name (#148).
pub(crate) fn initials(name: &str) -> String {
    let mut words = name.split_whitespace();
    let first = words.next();
    let last = words.next_back();
    first
        .into_iter()
        .chain(last)
        .filter_map(|word| word.chars().next())
        .flat_map(char::to_uppercase)
        .collect()
}

/// How long ago `at` was, in the largest whole unit: just now, minutes, hours, or days.
fn ago(at: SystemTime, now: SystemTime) -> String {
    let minutes = now.duration_since(at).unwrap_or_default().as_secs() / 60;
    match minutes {
        0 => "just now".to_string(),
        1..60 => format!("{minutes} min ago"),
        60..1440 => format!("{} h ago", minutes / 60),
        1440..2880 => "1 day ago".to_string(),
        _ => format!("{} days ago", minutes / 1440),
    }
}

/// A path under `/admin` that Identity does not serve. Anyone without a live session is sent to log in, returning here
/// after, as at `/admin`, so the admin pages cannot be mapped from outside; an admin gets the site's not-found
/// page, as any wrong address does.
async fn elsewhere<S: AdminStore>(
    State(served): Shared<S>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let web = &served.web;
    match served.admins.session(cookies(&headers)).await {
        Ok(Some(_)) => (
            StatusCode::NOT_FOUND,
            page(&NotFoundPage {
                demo: web.demo,
                stylesheet: web.stylesheet,
                scheme: scheme_of(&headers).as_str(),
            }),
        )
            .into_response(),
        Ok(None) => see_other(&return_to(uri.path())),
        Err(error) => unavailable_page(web, "could not answer an unknown admin path", &error),
    }
}

/// The live session a page's request comes with, which the request renews, and a form token for the page to carry;
/// none for a request with no live session.
async fn viewing<S: AdminStore>(
    served: &Served<S>,
    headers: &HeaderMap,
) -> Result<Option<(Session, String)>, Error> {
    let cookies = cookies(headers);
    let Some(token) = Realm::Admin
        .token_in(cookies.iter().copied())
        .and_then(SessionToken::parse)
    else {
        return Ok(None);
    };
    let Some(session) = served.admins.session(cookies.iter().copied()).await? else {
        return Ok(None);
    };
    token.form_token().map(|form| Some((session, form)))
}

async fn shown<S: AdminStore>(
    served: &Served<S>,
    headers: &HeaderMap,
) -> Result<Option<Response>, Error> {
    let Some((session, form)) = viewing(served, headers).await? else {
        return Ok(None);
    };
    // A session found live names an enabled admin, so an account is missing only if they were disabled since.
    let account = served.admins.account(&session).await?;
    let sessions = served.admins.sessions(&session).await?;
    let web = &served.web;
    let now = SystemTime::now();
    Ok(account.map(|account| {
        page(&SecurityPage {
            user: base64url(&account.user_handle),
            credentials: account
                .passkeys
                .iter()
                .map(|passkey| base64url(&passkey.credential))
                .collect::<Vec<_>>()
                .join(" "),
            can_add: account.passkeys.len() < 2,
            fresh: fresh(session.proven_at(), now),
            passkeys: account.passkeys.iter().map(described).collect(),
            sessions: sessions
                .into_iter()
                .map(|listed| Shown::of(listed, now))
                .collect(),
            shell: Shell::of(web, headers, account.name, form, SECURITY),
        })
    }))
}

/// How long the browser's session has left, read without renewing it, for the session island's clock: only the
/// person's own requests renew a session (NIST SP 800-63B-4).
async fn time_left<S: AdminStore>(State(served): Shared<S>, headers: HeaderMap) -> Response {
    let left = served.admins.time_left(cookies(&headers)).await;
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

/// How long after logging in an admin may add a passkey, which the database decides (ADR-0048, `v5.0.0-7.5.1`).
/// The page uses it only to offer the button, or say why it does not.
const FRESH: Duration = Duration::from_mins(10);

/// Whether a session a passkey last proved at `proven` is recent enough at `now` to add a passkey: under ten minutes
/// since. A session never proven, as one joining began, never is (ADR-0069). A proof this server's clock puts in the
/// future counts as fresh, and the database refuses it if the clocks disagree.
pub(crate) fn fresh(proven: Option<SystemTime>, now: SystemTime) -> bool {
    proven.is_some_and(|proven| now.duration_since(proven).map_or(true, |age| age < FRESH))
}

/// How the admin's page lists a passkey: whether it is synced to their other devices, as the authenticator
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

/// A step a logged-in admin takes, with the page's form token.
#[derive(Clone, Copy)]
enum LoggedIn {
    BeginAdding,
    FinishAdding,
    /// Staying logged in: the checks for a change have renewed the session, so this says how long it has now.
    Stay,
}

impl Before {
    /// The path each step is served at, as a refusal's record names it.
    fn path(self) -> &'static str {
        match self {
            Self::BeginLogin => LOGIN_BEGIN,
            Self::FinishLogin => LOGIN_FINISH,
            Self::BeginJoin => JOIN_BEGIN,
            Self::FinishJoin => JOIN_FINISH,
        }
    }

    /// The rate limit each step is held to (ADR-0072).
    fn limit(self) -> RateLimit {
        match self {
            Self::BeginLogin => RateLimit::LoginBegin,
            Self::FinishLogin => RateLimit::LoginFailure,
            Self::BeginJoin | Self::FinishJoin => RateLimit::Join,
        }
    }
}

impl LoggedIn {
    /// The path each step is served at, as a refusal's record names it.
    fn path(self) -> &'static str {
        match self {
            Self::BeginAdding => ADD_BEGIN,
            Self::FinishAdding => ADD_FINISH,
            Self::Stay => SESSION,
        }
    }
}

/// Runs a step before login, once the request is admitted and its network has room under the step's rate limit,
/// and counts a login that fails against its network.
async fn before<S: AdminStore>(
    served: Arc<Served<S>>,
    from: Requester,
    headers: HeaderMap,
    body: Bytes,
    step: Before,
) -> Result<Response, Refused> {
    let sent = admitted(&served, &from, &headers, &body, step.path()).await?;
    let admins = &served.admins;
    let result = match admins.admit(step.limit(), &from).await {
        Ok(()) => stepped(&served, &headers, &sent, step).await,
        Err(refused) => Err(refused),
    };
    let result = match result {
        Err(Error::Invalid(why)) if step.limit() == RateLimit::LoginFailure => {
            admins.failed(&from).await.and(Err(Error::Invalid(why)))
        }
        other => other,
    };
    Ok(answered(&served.web, result))
}

/// Begins or finishes logging in or joining.
async fn stepped<S: AdminStore>(
    served: &Served<S>,
    headers: &HeaderMap,
    sent: &Value,
    step: Before,
) -> Result<Response, Error> {
    let (web, admins) = (&served.web, &served.admins);
    let (ceremony, response) = answer_in(sent);
    let answer = Answer {
        ceremony,
        response: &response,
    };
    let actor = browser(headers, None);
    match step {
        Before::BeginLogin => admins
            .begin_login(&web.party)
            .await
            .map(|begun| begun_json(&begun)),
        Before::FinishLogin => admins
            .finish_login(&web.party, answer, &cookies(headers), &actor)
            .await
            .map(|token| {
                let to = sent["next"].as_str().and_then(return_path);
                logged_in_at(&token, to.unwrap_or(PROFILE))
            }),
        Before::BeginJoin => admins
            .begin_join(&web.party, sent["link"].as_str().unwrap_or_default())
            .await
            .map(|begun| begun_json(&begun)),
        // Joining logs the admin in, with a session that rests on the link and is never fresh (ADR-0069).
        Before::FinishJoin => admins
            .finish_join(&web.party, answer, &cookies(headers), &actor)
            .await
            .map(|(_, token)| logged_in_at(&token, PROFILE)),
    }
}

/// Runs a logged-in admin's step, once the request has passed the checks for a change, which renew the session:
/// begins or finishes adding a passkey, or stays logged in.
async fn logged_in<S: AdminStore>(
    served: Arc<Served<S>>,
    from: Requester,
    headers: HeaderMap,
    body: Bytes,
    step: LoggedIn,
) -> Result<Response, Refused> {
    // The origin before the body, so another site's form, which a browser sends without asking, is recorded as one.
    let sent = admitted(&served, &from, &headers, &body, step.path()).await?;
    let checked = acting(&served, &from, &headers, &sent, step.path()).await?;
    let (web, admins) = (&served.web, &served.admins);
    let answered_with = match step {
        LoggedIn::BeginAdding => admins
            .begin_adding_passkey(&web.party, &checked)
            .await
            .map(|begun| begun_json(&begun)),
        LoggedIn::FinishAdding => {
            let (ceremony, response) = answer_in(&sent);
            let answer = Answer {
                ceremony,
                response: &response,
            };
            let actor = browser(&headers, Some(checked.session().admin()));
            admins
                .finish_adding_passkey(&web.party, &checked, answer, &actor)
                .await
                .map(|()| next(SECURITY))
        }
        LoggedIn::Stay => admins.time_left(cookies(&headers)).await.map(left_json),
    };
    Ok(answered(web, answered_with))
}

/// Logs out from the admin's page's form, with its form token, and sends the browser to log in with its
/// cookie cleared, whether or not the session was still live, returning after to the checked page the form's address
/// names, as the session dialog's "Log in again" does. A form posts and shows pages, so a logout refused or failed
/// gets a page saying so, never a bare status or JSON.
async fn logout<S: AdminStore>(
    State(served): Shared<S>,
    Extension(from): Extension<Requester>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    let change = change(&headers, field(&body, "form").unwrap_or_default());
    match served
        .admins
        .end_session(&served.web.origins, &change)
        .await
    {
        Ok(Logout::Refused(check)) => refused_form(&served, &from, &headers, LOGOUT, check).await,
        // The browser also drops what it cached of the pages, so Back shows none of them. Only the cache: clearing
        // cookies would end its sessions in every realm on the host.
        Ok(Logout::Ended | Logout::NothingToEnd) => (
            [
                (header::SET_COOKIE, Realm::Admin.cleared_cookie()),
                (CLEAR_SITE_DATA, "\"cache\"".to_string()),
            ],
            see_other(&return_to(query(&uri, "next").unwrap_or_default())),
        )
            .into_response(),
        Err(error) => unavailable_page(&served.web, "could not log an admin out", &error),
    }
}

/// A page that says logging in is unavailable just now, for a request a person made by following a link or posting
/// a form, with the detail reported to the admin, never shown.
fn unavailable_page(web: &Web, what: &str, error: &Error) -> Response {
    (web.report)(&format!("identity: {what}: {error}"));
    let unavailable = UnavailablePage {
        demo: web.demo,
        stylesheet: web.stylesheet,
    };
    (StatusCode::SERVICE_UNAVAILABLE, page(&unavailable)).into_response()
}

/// A field of a posted form, as sent: the fields Identity's forms carry, a form token and a session id, need no
/// decoding, so one that would is refused where it is checked.
fn field<'a>(body: &'a [u8], name: &str) -> Option<&'a str> {
    std::str::from_utf8(body).ok()?.split('&').find_map(|pair| {
        pair.split_once('=')
            .filter(|(key, _)| *key == name)
            .map(|(_, value)| value)
    })
}

/// Ends one of the admin's other sessions, the one the form names, or every other one when it names none
/// (ADR-0059, `v5.0.0-7.5.2`), then returns to their page, which lists what is left, and says there why a session
/// not proven within ten minutes ended none. Its checks are logging out's: a request from elsewhere, without its
/// session's form token, or naming a session in no form an id takes is refused, and one whose session has ended goes
/// to log in.
async fn end_sessions<S: AdminStore>(
    State(served): Shared<S>,
    Extension(from): Extension<Requester>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let web = &served.web;
    let change = change(&headers, field(&body, "form").unwrap_or_default());
    let only = match field(&body, "session") {
        None => None,
        Some(text) => {
            // A request that fails a check is recorded as failing that one, and only one that passes them all as
            // naming no session id.
            let Some(id) = SessionId::parse(text) else {
                let check = change
                    .verified(Realm::Admin, &web.origins)
                    .err()
                    .unwrap_or(FailedCheck::Field);
                return refused_form(&served, &from, &headers, END_SESSIONS, check).await;
            };
            Some(id)
        }
    };
    let device = Device::from_user_agent(text(&headers, &header::USER_AGENT));
    match served
        .admins
        .end_others(&web.origins, &change, only.as_ref(), device)
        .await
    {
        Ok(Ending::Ended(_) | Ending::NotFresh) => see_other(SECURITY),
        Ok(Ending::NoSession) => see_other(LOGIN),
        Ok(Ending::Refused(check)) => {
            refused_form(&served, &from, &headers, END_SESSIONS, check).await
        }
        Err(error) => unavailable_page(web, "could not end an admin's sessions", &error),
    }
}

/// Records a form a check refused (#152), then the page it gets: forbidden, or unavailable if the record could not
/// be made.
async fn refused_form<S: AdminStore>(
    served: &Served<S>,
    from: &Requester,
    headers: &HeaderMap,
    route: &'static str,
    check: FailedCheck,
) -> Response {
    match record_refused(served, from, headers, route, check).await {
        Ok(()) => refused_page(&served.web),
        Err(error) => unavailable_page(&served.web, "could not record a refused request", &error),
    }
}

/// The page a refused form gets, forbidden (`v5.0.0-3.5.1`).
fn refused_page(web: &Web) -> Response {
    let refused = RefusedPage {
        demo: web.demo,
        stylesheet: web.stylesheet,
    };
    (StatusCode::FORBIDDEN, page(&refused)).into_response()
}

/// The live session a logged-in request acts for, or the answer it gets instead: forbidden when it comes from
/// elsewhere or without its session's form token, and unauthorized when its session has ended.
async fn acting<S: AdminStore>(
    served: &Served<S>,
    from: &Requester,
    headers: &HeaderMap,
    sent: &Value,
    route: &'static str,
) -> Result<crate::CheckedSession, Refused> {
    let change = change(headers, sent["form"].as_str().unwrap_or_default());
    match served
        .admins
        .session_for_change(&served.web.origins, &change)
        .await
    {
        Ok(ForChange::Acting(checked)) => Ok(checked),
        Ok(ForChange::NoSession) => {
            Err(Refused::with(refusal(StatusCode::UNAUTHORIZED, LOGGED_OUT)))
        }
        Ok(ForChange::Refused(check)) => {
            Err(refused_json(served, from, headers, route, check).await)
        }
        Err(error) => Err(Refused::with(unavailable(&served.web, &error))),
    }
}

/// A request before login, which has no session to check: its JSON, if it comes from one of the realm's
/// origins and says so in both ways a browser can (ADR-0059), or the answer it gets instead.
async fn admitted<S: AdminStore>(
    served: &Served<S>,
    from: &Requester,
    headers: &HeaderMap,
    body: &Bytes,
    route: &'static str,
) -> Result<Value, Refused> {
    let origin = text(headers, &header::ORIGIN);
    let site = fetch_site(headers);
    if served.web.origins.admit(origin, site) {
        json_in(headers, body)
    } else {
        Err(refused_json(served, from, headers, route, FailedCheck::Origin).await)
    }
}

/// Records a request a check refused (#152), then the answer a script gets: forbidden, or unavailable if the record
/// could not be made.
async fn refused_json<S: AdminStore>(
    served: &Served<S>,
    from: &Requester,
    headers: &HeaderMap,
    route: &'static str,
    check: FailedCheck,
) -> Refused {
    match record_refused(served, from, headers, route, check).await {
        Ok(()) => Refused::with(refusal(StatusCode::FORBIDDEN, REFUSED)),
        Err(error) => Refused::with(unavailable(&served.web, &error)),
    }
}

/// Records a request a check refused, naming the device it came from.
async fn record_refused<S: AdminStore>(
    served: &Served<S>,
    from: &Requester,
    headers: &HeaderMap,
    route: &'static str,
    check: FailedCheck,
) -> Result<(), Error> {
    let actor = browser(headers, None);
    served.admins.refused(from, route, check, &actor).await
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
    let cookie = Realm::Admin.session_cookie(token);
    ([(header::SET_COOKIE, cookie)], next(path)).into_response()
}

/// Where the script sends the browser once a ceremony is done.
fn next(path: &str) -> Response {
    json_out(StatusCode::OK, &json!({ "next": path }))
}

/// A use case's answer: its response, its one refusal, or unavailable when the server failed, which the admin
/// is told of through `report` and the browser only in general.
fn answered(web: &Web, result: Result<Response, Error>) -> Response {
    match result {
        Ok(response) => response,
        Err(Error::Invalid(why)) => refusal(StatusCode::BAD_REQUEST, &why),
        Err(Error::Limited(wait)) => limited(wait),
        Err(error) => unavailable(web, &error),
    }
}

/// A refusal by a rate limit, which says when to try again and never which limit it was (ADR-0072).
pub(crate) fn limited(wait: Duration) -> Response {
    let seconds = wait.as_secs().max(1);
    let minutes = seconds.div_ceil(60);
    let when = if minutes == 1 {
        "a minute".to_string()
    } else {
        format!("{minutes} minutes")
    };
    let why = format!("too many tries from your network. Try again in {when}");
    let mut response = refusal(StatusCode::TOO_MANY_REQUESTS, &why);
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from(seconds));
    response
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

/// Renders an admin page, under the admin pages' policy, kept by no cache, since it may carry a form token.
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

/// The login page, asked to return to `path` when that is a page a login may return to other than the realm's own
/// or Profile, where a login goes anyway.
fn return_to(path: &str) -> String {
    return_path(path)
        .filter(|path| *path != HOME && *path != PROFILE)
        .map_or_else(|| LOGIN.to_string(), |path| format!("{LOGIN}?next={path}"))
}

/// A value the address's query gives `name`, as written. Only values the pages check before use are read, and a
/// checked one needs no decoding.
fn query<'a>(uri: &'a Uri, name: &str) -> Option<&'a str> {
    uri.query()?.split('&').find_map(|pair| {
        pair.strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('='))
    })
}

/// The color scheme a request's cookies keep, which the admin pages' footer shows (ADR-0070).
fn scheme_of(headers: &HeaderMap) -> Scheme {
    Scheme::from_cookies(cookies(headers))
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
fn browser(headers: &HeaderMap, admin: Option<crate::AdminId>) -> Actor {
    Actor::browser(
        admin,
        Device::from_user_agent(text(headers, &header::USER_AGENT)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ago_names_the_largest_whole_unit() {
        let now = SystemTime::now();
        for (seconds, said) in [
            (0, "just now"),
            (59, "just now"),
            (60, "1 min ago"),
            (3599, "59 min ago"),
            (3600, "1 h ago"),
            (86_399, "23 h ago"),
            (86_400, "1 day ago"),
            (172_799, "1 day ago"),
            (172_800, "2 days ago"),
        ] {
            assert_eq!(
                ago(now - Duration::from_secs(seconds), now),
                said,
                "{seconds}"
            );
        }
        assert_eq!(ago(now + Duration::from_secs(90), now), "just now");
    }
}
