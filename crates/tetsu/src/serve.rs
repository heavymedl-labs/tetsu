//! `tetsu serve`: the public site, Tetsu's own page at the root and each tenant's page at its address (#76).

use std::future::{Future, IntoFuture};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use askama::Template;

use crate::assets;
use axum::Router;
use axum::body::Bytes;
use axum::extract::rejection::PathRejection;
use axum::extract::{ConnectInfo, Path, Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{MethodRouter, get};
use tetsu_kernel::{Requester, Scheme};
use tower_http::CompressionLevel;
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::{DefaultPredicate, NotForContentType, Predicate};

/// Tetsu's own page at the root.
#[derive(Template)]
#[template(path = "home.html")]
struct HomePage {
    demo: bool,
    stylesheet: &'static str,
    /// The radio the footer's color scheme control checks (ADR-0070).
    scheme: &'static str,
}

/// The site's page for a path nothing answers.
#[derive(Template)]
#[template(path = "not_found.html")]
struct NotFoundPage {
    demo: bool,
    stylesheet: &'static str,
    scheme: &'static str,
}

/// How long requests in progress may take to finish once the server is asked to stop. A client that never
/// finishes its request cannot hold the server open past it.
pub const GRACE: Duration = Duration::from_secs(10);

/// Sent with every response (ADR-0029). The site's pages run no scripts and load only Tetsu's own stylesheets and
/// fonts, so the policy allows nothing else, not even an inline style, and no site may frame them (ADR-0030). A
/// route that runs Tetsu's own script sets a policy of its own, which [`secure`] keeps, as it keeps a route's own
/// referrer policy. HSTS lasts a year, the
/// least ASVS 3.4.1 allows.
const HEADERS: [(HeaderName, &str); 5] = [
    (
        header::STRICT_TRANSPORT_SECURITY,
        "max-age=31536000; includeSubDomains",
    ),
    (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    (
        header::CONTENT_SECURITY_POLICY,
        "default-src 'none'; object-src 'none'; style-src 'self'; font-src 'self'; img-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
    ),
    (header::REFERRER_POLICY, "no-referrer"),
    (header::X_FRAME_OPTIONS, "DENY"),
];

/// A hashed file changes name whenever it changes, so it may be kept for a year (ADR-0034).
const IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// A fixed address keeps its name when its file changes, so a client keeps it a day.
const DAILY: &str = "public, max-age=86400";

/// How this service serves: whether its businesses are demos (`TETSU_DEMO`), how many proxies stand in front (ADR-0072), and, for a host
/// that serves a realm but not the site, the realm's home its root sends visitors to (#169).
#[derive(Debug, Clone, Copy)]
pub struct Site {
    pub demo: bool,
    pub proxies: usize,
    pub home: Option<&'static str>,
}

/// The whole site: its own routes, then the modules' routers, then `not_found` at any path none of them
/// answers, which is [`not_found`] outside tests. Modules set no fallback, since axum panics when two merged
/// routers both have one (#90). Where `TETSU_DEMO` marks the businesses as demos, every response also asks search
/// engines not to index it (ADR-0018).
pub fn app(modules: Router, site: Site, not_found: fn(bool, Scheme) -> Response) -> Router {
    let Site {
        demo,
        proxies,
        home,
    } = site;
    site_routes(demo, home)
        .into_iter()
        .fold(Router::new(), |site, (path, route)| site.route(path, route))
        .merge(modules)
        .route_layer(axum::middleware::from_fn(crate::logs::route))
        .fallback(move |headers: HeaderMap| {
            std::future::ready(not_found(demo, scheme_of(&headers)))
        })
        .layer(axum::middleware::map_response(minify))
        .layer(axum::middleware::map_response_with_state(demo, secure))
        .layer(axum::middleware::from_fn_with_state(proxies, requester))
        .layer(compression())
        .layer(axum::middleware::from_fn(crate::logs::request))
}

/// Notes the network each request came from, for the rate limits (ADR-0072): with `proxies` in front of Tetsu,
/// the `X-Forwarded-For` entry the first of them added, or else the connection's own address. A request served
/// without its connection's address, as a test's may be, comes from the unspecified address.
async fn requester(State(proxies): State<usize>, mut request: Request, next: Next) -> Response {
    let connection = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED), |info| info.0.ip());
    let forwarded = request.headers().get_all("x-forwarded-for");
    let from = Requester::from_request(
        connection,
        forwarded.iter().filter_map(|value| value.to_str().ok()),
        proxies,
    );
    request.extensions_mut().insert(from);
    next.run(request).await
}

/// Brotli or gzip, as the browser asks (ADR-0039). Fonts are already compressed, so they go as they are. Quality 4
/// keeps the cost of each response small, set here so no upgrade changes it unseen. No test checks it, since a
/// response shows its quality only as time, and timing tests are unreliable.
fn compression() -> CompressionLayer<impl Predicate> {
    CompressionLayer::new()
        .quality(CompressionLevel::Precise(4))
        .compress_when(DefaultPredicate::new().and(NotForContentType::const_new("font/")))
}

/// The site's page for a path nothing answers, in the visitor's color scheme. It is the site's own, since a module's
/// page would speak for every other module's paths too.
pub fn not_found(demo: bool, scheme: Scheme) -> Response {
    html(
        StatusCode::NOT_FOUND,
        &NotFoundPage {
            demo,
            stylesheet: assets::STYLESHEET,
            scheme: scheme.as_str(),
        },
    )
}

/// The color scheme a request's cookies keep (ADR-0070).
fn scheme_of(headers: &HeaderMap) -> Scheme {
    Scheme::from_cookies(
        headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok()),
    )
}

/// A built file at the exact path the manifest names; any other path under `/assets/` is not found.
/// The touch icon at `/favicon.ico`, for clients that ask there without reading a page's links. Every client that
/// does accepts a PNG.
fn favicon(demo: bool) -> Response {
    let mut response = asset(assets::TOUCH_ICON.trim_start_matches("/assets/"), demo);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(DAILY));
    response
}

fn asset(file: &str, demo: bool) -> Response {
    let Some(found) = assets::find(&format!("/assets/{file}")) else {
        return not_found(demo, Scheme::System);
    };
    (
        [
            (header::CONTENT_TYPE, found.content_type),
            (header::CACHE_CONTROL, IMMUTABLE),
        ],
        found.bytes,
    )
        .into_response()
}

/// Renders one of the site's pages. A compiled template fails only if writing fails, so a failure gets a bare
/// unavailable status rather than half a page. Tenancy renders its own pages the same way; ADR-0021 keeps such a
/// helper out of its public surface, so each crate has its own.
fn html(status: StatusCode, page: &impl Template) -> Response {
    html_body(status, page.render().ok().map(Bytes::from))
}

/// The site's pages' policy: the site's, with Tetsu's own script, which keeps the color scheme (ADR-0070), and still
/// nothing inline.
const PAGE_POLICY: &str = "default-src 'none'; object-src 'none'; script-src 'self'; style-src 'self'; \
                           font-src 'self'; img-src 'self'; base-uri 'none'; form-action 'none'; \
                           frame-ancestors 'none'";

/// A page already rendered, or `None` when it could not be, which answers that the site is unavailable. Pages differ
/// by the visitor's color scheme cookie, so a cache keeps one for each.
fn html_body(status: StatusCode, body: Option<Bytes>) -> Response {
    let Some(body) = body else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let headers = [
        (header::CONTENT_TYPE, "text/html; charset=utf-8"),
        (header::CONTENT_SECURITY_POLICY, PAGE_POLICY),
        (header::VARY, "Cookie"),
    ];
    (status, headers, body).into_response()
}

/// Tetsu's own routes. Every route the site adds goes here, never straight into [`app`], and each starts with a
/// name no tenant may hold, which a test checks, so none hides a tenant. Identity's routes are checked the same
/// way.
fn site_routes(demo: bool, home: Option<&'static str>) -> Vec<(&'static str, MethodRouter)> {
    let root = match home {
        Some(home) => get(move || std::future::ready(Redirect::to(home).into_response())),
        None => home_page(demo),
    };
    vec![
        ("/", root),
        (
            "/favicon.ico",
            get(move || std::future::ready(favicon(demo))),
        ),
        (
            "/assets/{file}",
            get(move |file: Result<Path<String>, PathRejection>| {
                // A path that does not decode, such as one that is not UTF-8, names no built file.
                std::future::ready(file.map_or_else(
                    |_| not_found(demo, Scheme::System),
                    |Path(file)| asset(&file, demo),
                ))
            }),
        ),
    ]
}

/// The site's home page. It depends only on `demo`, the stylesheet's name, which hold while the process runs, and the
/// visitor's color scheme, so it is rendered once in each scheme here rather than on every request. Each response
/// shares its bytes.
fn home_page(demo: bool) -> MethodRouter {
    let homes = [Scheme::System, Scheme::Light, Scheme::Dark].map(|scheme| {
        HomePage {
            demo,
            stylesheet: assets::STYLESHEET,
            scheme: scheme.as_str(),
        }
        .render()
        .ok()
        .map(Bytes::from)
    });
    get(move |headers: HeaderMap| {
        let home = homes[scheme_of(&headers) as usize].clone();
        std::future::ready(html_body(StatusCode::OK, home))
    })
}

/// Whether a response is a page, by its content type.
fn is_page(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .is_some_and(|kind| kind.as_bytes().starts_with(b"text/html"))
}

/// Sends every page minified, whichever module renders it, so the whitespace formatted templates add costs visitors
/// nothing (ADR-0083). A page whose body fails as it is read is an error, as it would be if sent.
async fn minify(response: Response) -> Response {
    if !is_page(response.headers()) {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let Ok(page) = axum::body::to_bytes(body, usize::MAX).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(parts, axum::body::Body::from(minified(page)))
}

/// `page` with each run of whitespace collapsed to one character by html-minifier, which keeps a newline where the
/// run had one, keeps attribute values, and leaves `pre`, `code`, `textarea`, and inline SVG as they are. Inline
/// scripts and styles, which no page has, stay as written. A page the minifier cannot finish, one that ends inside a
/// tag, goes as rendered.
fn minified(page: Bytes) -> Bytes {
    let mut minifier = html_minifier::HTMLMinifier::new();
    minifier.set_minify_code(false);
    minifier.set_minify_js(false);
    minifier.set_minify_css(false);
    if minifier.digest(&page).is_err() {
        return page;
    }
    minifier.into_html().map_or(page, Bytes::from)
}

async fn secure(State(demo): State<bool>, mut response: Response) -> Response {
    let headers = response.headers_mut();
    for (name, value) in HEADERS {
        // A page that runs Tetsu's own script, or posts a form, sets the policies it needs, which stay; every other
        // header is the site's alone.
        if name == header::CONTENT_SECURITY_POLICY || name == header::REFERRER_POLICY {
            headers
                .entry(name)
                .or_insert(HeaderValue::from_static(value));
        } else {
            headers.insert(name, HeaderValue::from_static(value));
        }
    }
    // Every page asks for its text's font as it starts, rather than once the stylesheet has arrived, so text paints
    // in it rather than switching to it, whichever module renders the page.
    if is_page(headers) {
        headers.insert(header::LINK, HeaderValue::from_static(assets::FONT_PRELOAD));
    }
    if demo {
        headers.insert(
            HeaderName::from_static("x-robots-tag"),
            HeaderValue::from_static("noindex"),
        );
    }
    response
}

/// Serves until `shutdown` finishes, then lets requests in progress complete for up to `grace`, and stops.
///
/// # Errors
/// Fails if accepting connections fails.
pub async fn run(
    listener: tokio::net::TcpListener,
    app: Router,
    shutdown: impl Future<Output = ()> + Send + 'static,
    grace: Duration,
) -> std::io::Result<()> {
    let (stopping, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        shutdown.await;
        let _ = stopping.send(());
    })
    .into_future();
    let deadline = async move {
        // The sender goes only when the server has already stopped, which the other branch reports.
        if stopped.await.is_ok() {
            tokio::time::sleep(grace).await;
        } else {
            std::future::pending::<()>().await;
        }
    };
    tokio::select! {
        result = server => result,
        () = deadline => Ok(()),
    }
}

/// Finishes when the process is asked to stop: SIGTERM, which a host sends before it replaces a server, or
/// Ctrl-C. On Unix, which every host Tetsu runs on uses, the handlers are set up when this is called, not when it
/// is first awaited, so a signal that arrives while the server starts lets it finish starting and then stop
/// cleanly, rather than killing it. Call it before anything slow. Elsewhere, Ctrl-C is watched only once this is
/// awaited. A handler that cannot be set up never finishes, rather than stopping the server at once.
pub fn stop_signal() -> impl Future<Output = ()> + Send + 'static {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{Signal, SignalKind, signal};
        async fn received(signal: Option<Signal>) {
            match signal {
                Some(mut signal) => {
                    signal.recv().await;
                }
                None => std::future::pending::<()>().await,
            }
        }
        let terminate = signal(SignalKind::terminate()).ok();
        let interrupt = signal(SignalKind::interrupt()).ok();
        async move {
            tokio::select! {
                () = received(terminate) => {},
                () = received(interrupt) => {},
            }
        }
    }
    #[cfg(not(unix))]
    async {
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    use super::*;

    /// The values ADR-0029 and ASVS ask for, written out so a weakened header fails the test.
    const EXPECTED: [(&str, &str); 5] = [
        (
            "strict-transport-security",
            "max-age=31536000; includeSubDomains",
        ),
        ("x-content-type-options", "nosniff"),
        (
            "content-security-policy",
            "default-src 'none'; object-src 'none'; style-src 'self'; font-src 'self'; img-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
        ),
        ("referrer-policy", "no-referrer"),
        ("x-frame-options", "DENY"),
    ];
    const POST: &str =
        "POST / HTTP/1.1\r\nHost: test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

    /// Stands in for a module's router: a page at one path, and no fallback, which is the site's alone.
    fn module() -> Router {
        Router::new()
            .route("/studio", get(|| async { "studio" }))
            .route(
                "/scripted",
                get(|| async {
                    let policies = [
                        (header::CONTENT_SECURITY_POLICY, SCRIPTED),
                        (header::REFERRER_POLICY, "same-origin"),
                    ];
                    (policies, "scripted")
                }),
            )
    }

    /// A policy a module's page sets for itself, as a page that runs Tetsu's own script does.
    const SCRIPTED: &str = "default-src 'none'; script-src 'self'";

    /// A route may set its own content security and referrer policies, which the site keeps; every other header is
    /// the site's alone, and a route that sets no policy gets the site's.
    #[tokio::test]
    async fn a_route_keeps_its_own_content_security_policy() {
        let raw = get_path("/scripted").await;
        assert_eq!(
            header_of(&raw, "content-security-policy"),
            Some(SCRIPTED),
            "{raw}"
        );
        assert_eq!(
            header_of(&raw, "referrer-policy"),
            Some("same-origin"),
            "{raw}"
        );
        for (name, value) in EXPECTED
            .iter()
            .filter(|(name, _)| !matches!(*name, "content-security-policy" | "referrer-policy"))
        {
            assert_eq!(header_of(&raw, name), Some(*value), "{name} in {raw}");
        }
    }

    /// Stands in for the site's own not found page, so these tests can tell which page answered, and in which scheme.
    fn not_found(demo: bool, scheme: Scheme) -> Response {
        (
            StatusCode::NOT_FOUND,
            format!("demo {demo}, {}: module not found", scheme.as_str()),
        )
            .into_response()
    }

    /// The site's settings in a test: no proxy, and the site's own root unless a realm's `home` is given.
    const fn site(demo: bool, home: Option<&'static str>) -> Site {
        Site {
            demo,
            proxies: 0,
            home,
        }
    }

    /// Serves the site on a port of its own until the test ends, and says where.
    async fn serving(site: Site) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(run(
            listener,
            app(module(), site, not_found),
            std::future::pending(),
            GRACE,
        ));
        address
    }

    /// Sends one raw request to `address`, and returns the raw response.
    async fn sent(address: std::net::SocketAddr, request: &str) -> String {
        let mut stream = TcpStream::connect(address).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut raw = String::new();
        stream.read_to_string(&mut raw).await.unwrap();
        raw
    }

    /// Sends one request to the site on a port of its own, and returns the raw response.
    async fn send_to(demo: bool, request: &str) -> String {
        sent(serving(site(demo, None)).await, request).await
    }

    async fn send(request: &str) -> String {
        send_to(true, request).await
    }

    /// A host that serves a realm but not the site sends `/` to the realm's home, and still serves the assets its
    /// pages load and the site's not-found page elsewhere.
    #[tokio::test]
    async fn a_realms_own_host_sends_its_root_to_the_realms_home() {
        let address = serving(site(true, Some("/admin"))).await;
        let ask = |path: &'static str| async move {
            sent(
                address,
                &format!("GET {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n"),
            )
            .await
        };
        let root = ask("/").await;
        assert!(root.starts_with("HTTP/1.1 303"), "{root}");
        assert!(
            root.to_ascii_lowercase()
                .contains("\r\nlocation: /admin\r\n"),
            "{root}"
        );
        assert!(ask(assets::STYLESHEET).await.starts_with("HTTP/1.1 200"));
        assert!(ask("/nowhere").await.starts_with("HTTP/1.1 404"));
    }

    /// Asks the site, marked as holding demo businesses, for a path as a browser that accepts brotli and gzip does, and returns the response's
    /// head and how many bytes its body took to send.
    async fn as_sent(path: &str) -> (String, usize) {
        let address = serving(site(true, None)).await;
        let mut stream = TcpStream::connect(address).await.unwrap();
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: test\r\nAccept-Encoding: br, gzip\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.unwrap();
        let head = raw.windows(4).position(|four| four == b"\r\n\r\n").unwrap();
        (
            String::from_utf8(raw[..head].to_vec()).unwrap(),
            raw.len() - head - 4,
        )
    }

    fn get_request(path: &str) -> String {
        format!("GET {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n")
    }

    async fn get_path(path: &str) -> String {
        send(&get_request(path)).await
    }

    fn header_of<'a>(raw: &'a str, name: &str) -> Option<&'a str> {
        let (head, _) = raw.split_once("\r\n\r\n").unwrap();
        head.lines()
            .filter_map(|line| line.split_once(": "))
            .find(|(found, _)| found.eq_ignore_ascii_case(name))
            .map(|(_, value)| value)
    }

    #[tokio::test]
    async fn the_root_is_tetsus_own_page() {
        let raw = get_path("/").await;
        assert!(raw.starts_with("HTTP/1.1 200"), "{raw}");
        assert_eq!(
            header_of(&raw, "content-type"),
            Some("text/html; charset=utf-8")
        );
        assert!(raw.contains("Booking software,"), "{raw}");
    }

    #[tokio::test]
    async fn other_paths_go_to_the_modules() {
        assert!(get_path("/studio").await.ends_with("studio"));
        let raw = get_path("/elsewhere").await;
        assert!(raw.starts_with("HTTP/1.1 404"), "{raw}");
        assert!(raw.ends_with("module not found"), "{raw}");
    }

    /// The site sets the one fallback, so a path no route answers, however deep, gets the page it was given.
    #[tokio::test]
    async fn a_path_nothing_answers_gets_the_not_found_page_for_its_environment() {
        let deeper = get_path("/studio/deeper").await;
        assert!(deeper.starts_with("HTTP/1.1 404"), "{deeper}");
        assert!(
            deeper.ends_with("demo true, system: module not found"),
            "{deeper}"
        );
        let outside = send_to(false, &get_request("/elsewhere")).await;
        assert!(
            outside.ends_with("demo false, system: module not found"),
            "{outside}"
        );
        // The page is in the visitor's kept scheme (ADR-0070).
        let request = "GET /elsewhere HTTP/1.1\r\nHost: test\r\nCookie: theme=light\r\nConnection: close\r\n\r\n";
        assert!(
            send_to(true, request)
                .await
                .ends_with("demo true, light: module not found")
        );
    }

    #[tokio::test]
    async fn the_sites_not_found_page_is_html_marked_for_its_environment() {
        for demo in [true, false] {
            let response = super::not_found(demo, Scheme::System);
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            assert_eq!(
                response.headers().get(header::CONTENT_TYPE).unwrap(),
                "text/html; charset=utf-8"
            );
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            // As sent, read with each run of whitespace as one space, since the minifier leaves inline SVG as
            // formatted.
            let body = String::from_utf8(minified(body).to_vec()).unwrap();
            let body = body.split_whitespace().collect::<Vec<_>>().join(" ");
            // The site's own header and footer around "404" in the brand's slots (ADR-0035).
            for shown in [
                "<title>Page not found – Tetsu</title>",
                "<nav aria-label=\"Tetsu\"",
                "<svg role=\"img\" aria-label=\"404\"",
                "There is nothing booked at this address.",
                "© 2026 Tetsu",
            ] {
                assert!(body.contains(shown), "{shown} in {body}");
            }
            assert_eq!(body.contains("noindex"), demo, "{body}");
        }
    }

    /// Built files are served at the exact paths Vite's manifest names, kept for a year (ADR-0034).
    #[tokio::test]
    async fn the_built_stylesheet_is_served_as_css_and_kept_for_a_year() {
        let raw = get_path(assets::STYLESHEET).await;
        assert!(raw.starts_with("HTTP/1.1 200"), "{raw}");
        assert_eq!(
            header_of(&raw, "content-type"),
            Some("text/css; charset=utf-8")
        );
        assert_eq!(
            header_of(&raw, "cache-control"),
            Some("public, max-age=31536000, immutable")
        );
        assert!(raw.contains("--tetsu-color-primary"), "{raw}");
        assert_eq!(header_of(&raw, "x-content-type-options"), Some("nosniff"));
        for path in [
            "/assets/tetsu.css",
            "/assets/missing-AbC123.css",
            "/assets/",
            "/assets/%FF.css",
        ] {
            let raw = get_path(path).await;
            assert!(raw.starts_with("HTTP/1.1 404"), "{path}: {raw}");
            assert_eq!(header_of(&raw, "cache-control"), None, "{path}");
        }
    }

    #[tokio::test]
    async fn the_conventional_favicon_address_serves_the_icon() {
        // Clients that ask for /favicon.ico without reading the page's links get the touch icon, a PNG.
        let (head, size) = as_sent("/favicon.ico").await;
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
        assert!(head.contains("content-type: image/png"), "{head}");
        assert!(
            head.contains("cache-control: public, max-age=86400"),
            "{head}"
        );
        let icon = assets::find(crate::assets::TOUCH_ICON).unwrap();
        assert_eq!(size, icon.bytes.len());
    }

    /// A route at a name a tenant could hold would hide that tenant (#90), so each of the site's own routes
    /// starts with a segment no slug may be.
    #[test]
    fn every_route_the_site_serves_starts_with_a_reserved_name() {
        for (path, _) in site_routes(true, None) {
            let segment = path.trim_start_matches('/').split('/').next().unwrap_or("");
            if !segment.is_empty() {
                assert!(tetsu_tenancy::Slug::parse(segment).is_err(), "{path}");
            }
        }
    }

    /// Identity's pages are routes of a module, which must not hide a tenant either.
    #[test]
    fn every_route_identity_serves_starts_with_a_reserved_name() {
        for path in tetsu_identity::PATHS {
            let segment = path.trim_start_matches('/').split('/').next().unwrap_or("");
            assert!(tetsu_tenancy::Slug::parse(segment).is_err(), "{path}");
        }
    }

    #[tokio::test]
    async fn every_response_carries_the_security_headers() {
        // The site's own pages run its color scheme script, so their policy allows Tetsu's own script (ADR-0070).
        for (raw, page) in [
            (get_path("/").await, true),
            (get_path("/studio").await, false),
            (get_path("/elsewhere").await, false),
            (send(POST).await, false),
        ] {
            for (name, value) in EXPECTED {
                let value = if page && name == "content-security-policy" {
                    PAGE_POLICY
                } else {
                    value
                };
                assert_eq!(header_of(&raw, name), Some(value), "{name} in {raw}");
            }
            assert_eq!(
                header_of(&raw, "access-control-allow-origin"),
                None,
                "{raw}"
            );
        }
    }

    #[tokio::test]
    async fn only_a_demo_environment_asks_search_engines_not_to_index() {
        for path in ["/", "/studio", "/elsewhere"] {
            assert_eq!(
                header_of(&get_path(path).await, "x-robots-tag"),
                Some("noindex"),
                "{path}"
            );
            assert_eq!(
                header_of(&send_to(false, &get_request(path)).await, "x-robots-tag"),
                None,
                "{path}"
            );
        }
    }

    #[tokio::test]
    async fn only_get_and_head_are_answered() {
        let head = send("HEAD / HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n").await;
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
        let post = send(POST).await;
        assert!(post.starts_with("HTTP/1.1 405"), "{post}");
    }

    /// A server on a port of its own that stops when the returned sender is used.
    async fn stoppable(
        grace: Duration,
    ) -> (
        std::net::SocketAddr,
        tokio::sync::oneshot::Sender<()>,
        tokio::task::JoinHandle<std::io::Result<()>>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let shutdown = async {
            let _ = stopped.await;
        };
        let server = tokio::spawn(run(
            listener,
            app(module(), site(true, None), not_found),
            shutdown,
            grace,
        ));
        (address, stop, server)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_sigterm_sent_before_the_signal_is_awaited_still_stops_it() {
        // Sent before anything awaits the signal: without the handler already set up, this kills the test.
        let stop = stop_signal();
        let pid = std::process::id().to_string();
        let sent = std::process::Command::new("kill")
            .args(["-TERM", &pid])
            .status()
            .unwrap();
        assert!(sent.success());
        assert!(
            tokio::time::timeout(Duration::from_secs(5), stop)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn the_root_page_carries_no_demo_note() {
        assert!(!get_path("/").await.contains("<strong>Demo.</strong>"));
        assert!(!get_path("/").await.contains("role=\"note\""));
        assert!(!send_to(false, &get_request("/")).await.contains("Demo"));
    }

    #[tokio::test]
    async fn the_root_page_describes_itself_and_names_its_icons() {
        for demo in [true, false] {
            let raw = send_to(demo, &get_request("/")).await;
            assert!(
                raw.contains("<title>Tetsu – booking software, built like iron</title>"),
                "{raw}"
            );
            assert!(
                raw.contains(r#"<meta name="description" content=""#),
                "{raw}"
            );
            assert!(
                raw.contains(r#"<meta property="og:title" content=""#),
                "{raw}"
            );
            for path in [crate::assets::ICON, crate::assets::TOUCH_ICON] {
                assert!(
                    raw.contains(&format!(r#"href="{path}""#)),
                    "{path} in {raw}"
                );
                assert!(crate::assets::find(path).is_some(), "{path}");
            }
        }
    }

    /// The root page as it renders, in a demo or not.
    /// The root page as it is sent, minified.
    fn home_page(demo: bool) -> String {
        let page = HomePage {
            demo,
            stylesheet: "",
            scheme: "system",
        }
        .render()
        .unwrap();
        String::from_utf8(minified(Bytes::from(page)).to_vec()).unwrap()
    }

    /// The root page is sent as it renders, minified (ADR-0083), in each color scheme.
    #[tokio::test]
    async fn the_root_page_served_is_the_page_as_it_renders() {
        // The page is rendered once per process and scheme (site_routes), so what goes out must match a fresh render.
        for demo in [true, false] {
            for (cookie, scheme) in [
                ("", Scheme::System),
                ("theme=light", Scheme::Light),
                ("a=1; theme=dark", Scheme::Dark),
            ] {
                let request = format!(
                    "GET / HTTP/1.1\r\nHost: test\r\nCookie: {cookie}\r\nConnection: close\r\n\r\n"
                );
                let raw = send_to(demo, &request).await;
                let fresh = HomePage {
                    demo,
                    stylesheet: assets::STYLESHEET,
                    scheme: scheme.as_str(),
                }
                .render()
                .unwrap();
                let fresh = String::from_utf8(minified(Bytes::from(fresh)).to_vec()).unwrap();
                assert_eq!(
                    raw.split_once("\r\n\r\n").unwrap().1,
                    fresh,
                    "demo {demo}, {cookie}"
                );
                assert_eq!(header_of(&raw, "vary"), Some("Cookie"), "{cookie}");
            }
        }
    }

    #[test]
    fn minifying_collapses_each_run_of_whitespace_to_one_character() {
        let page = Bytes::from_static(b"<ul>\n  <li>\n    One  two\n  </li>\n</ul>\n");
        assert_eq!(&minified(page)[..], b"<ul>\n<li>\nOne two\n</li>\n</ul>");
    }

    /// Whitespace a browser shows as written stays as written, and an attribute keeps its value.
    #[test]
    fn minifying_leaves_preformatted_text_and_attribute_values_alone() {
        let page = Bytes::from_static(
            b"<pre>a   b\n   c</pre> <code>a   b\n   c</code> <textarea>a   b</textarea> <p title=\"x   y\">z</p>",
        );
        assert_eq!(minified(page.clone()), page);
    }

    /// A page that ends inside a tag cannot be minified safely, so it goes as rendered.
    #[test]
    fn a_page_the_minifier_cannot_finish_goes_as_rendered() {
        let page = Bytes::from_static(b"<p>\n  Hello <a href=\"/x");
        assert_eq!(minified(page.clone()), page);
    }

    /// Only HTML is minified; the length a page was rendered with no longer holds, so it goes.
    #[tokio::test]
    async fn only_html_is_minified_and_its_old_length_goes() {
        let text = ([(header::CONTENT_TYPE, "text/plain")], "a\n  b").into_response();
        let text = minify(text).await;
        assert_eq!(
            &axum::body::to_bytes(text.into_body(), 64).await.unwrap()[..],
            b"a\n  b"
        );
        let page = (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CONTENT_LENGTH, "11"),
            ],
            "<p>\n  a</p>",
        )
            .into_response();
        let page = minify(page).await;
        assert_eq!(page.headers().get(header::CONTENT_LENGTH), None);
        assert_eq!(
            &axum::body::to_bytes(page.into_body(), 64).await.unwrap()[..],
            b"<p>\na</p>"
        );
    }

    /// A page whose body fails as it is read cannot be sent, so the visitor gets an error rather than part of it.
    #[tokio::test]
    async fn a_page_whose_body_fails_is_an_error() {
        struct Failing;
        impl axum::body::HttpBody for Failing {
            type Data = Bytes;
            type Error = std::io::Error;
            fn poll_frame(
                self: std::pin::Pin<&mut Self>,
                _: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Option<Result<http_body::Frame<Bytes>, std::io::Error>>>
            {
                std::task::Poll::Ready(Some(Err(std::io::Error::other("broken"))))
            }
        }
        let mut page = Response::new(axum::body::Body::new(Failing));
        page.headers_mut()
            .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html"));
        assert_eq!(
            minify(page).await.status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    /// Every page asks for its text's font as it starts, rather than once the stylesheet has arrived, so text paints
    /// in it rather than switching to it, through a `Link` header the whole site shares, whichever module renders
    /// the page; and it is the font the stylesheet uses. A file that is not a page, such as the stylesheet, asks
    /// for nothing.
    #[tokio::test]
    async fn every_page_preloads_the_font_its_stylesheet_uses() {
        let font = assets::FONT_PRELOAD
            .strip_prefix('<')
            .and_then(|rest| rest.split_once('>'))
            .map(|(font, _)| font)
            .unwrap();
        assert!(
            font.starts_with("/assets/inter-latin-opsz-normal-"),
            "{font}"
        );
        assert_eq!(
            header_of(&get_path("/").await, "link"),
            Some(assets::FONT_PRELOAD)
        );
        // An answer that is not a page, here the test module's text and its not-found text, and a built file ask
        // for nothing.
        for path in ["/studio", "/elsewhere", assets::STYLESHEET] {
            assert_eq!(header_of(&get_path(path).await, "link"), None, "{path}");
        }
        assert!(!home_page(true).contains("rel=\"preload\""));
        assert!(stylesheet().contains(font));
    }

    /// The page tells the browser its scheme before any stylesheet arrives, so a page kept light never flashes dark
    /// in a dark system, and the browser bar takes the kept scheme's color; with none kept, both follow the system.
    #[tokio::test]
    async fn the_root_page_names_its_kept_scheme_before_the_stylesheet() {
        let both = [
            "<meta name=\"theme-color\" media=\"(prefers-color-scheme: light)\" content=\"#f3f1f1\">",
            "<meta name=\"theme-color\" media=\"(prefers-color-scheme: dark)\" content=\"#111110\">",
        ];
        for (cookie, meta, bars) in [
            (
                "theme=light",
                "light",
                &["<meta name=\"theme-color\" content=\"#f3f1f1\">"][..],
            ),
            (
                "theme=dark",
                "dark",
                &["<meta name=\"theme-color\" content=\"#111110\">"][..],
            ),
            ("", "light dark", &both[..]),
        ] {
            let request = format!(
                "GET / HTTP/1.1\r\nHost: test\r\nCookie: {cookie}\r\nConnection: close\r\n\r\n"
            );
            let raw = send_to(true, &request).await;
            let color_scheme = format!("<meta name=\"color-scheme\" content=\"{meta}\">");
            assert!(raw.contains(&color_scheme), "{color_scheme} for {cookie}");
            // The first thing the browser reads names the kept scheme, before the footer's control exists.
            let picked = match meta {
                "light dark" => "system",
                other => other,
            };
            let root = format!("<html lang=\"en\" class=\"h-full\" data-scheme=\"{picked}\">");
            assert!(raw.contains(&root), "{root} for {cookie}");
            for bar in bars {
                assert!(raw.contains(bar), "{bar} for {cookie}");
            }
            assert_eq!(
                raw.matches("name=\"theme-color\"").count(),
                bars.len(),
                "{cookie}"
            );
        }
    }

    /// The root page checks the radio of the visitor's kept scheme, and loads the script that keeps a new one, under
    /// a policy that allows Tetsu's own script and nothing inline (ADR-0070).
    #[tokio::test]
    async fn the_root_page_arrives_in_the_kept_scheme_with_its_script() {
        let request =
            "GET / HTTP/1.1\r\nHost: test\r\nCookie: theme=dark\r\nConnection: close\r\n\r\n";
        let raw = send_to(true, request).await;
        assert!(
            raw.contains("id=\"tetsu-scheme-dark\" class=\"sr-only\" checked"),
            "{raw}"
        );
        assert!(
            !raw.contains("id=\"tetsu-scheme-system\" class=\"sr-only\" checked"),
            "{raw}"
        );
        let script = format!("<script type=\"module\" src=\"{}\">", assets::SCHEME_SCRIPT);
        assert!(raw.contains(&script), "{raw}");
        let policy = header_of(&raw, "content-security-policy").unwrap_or_default();
        assert!(
            policy.contains("script-src 'self'") && !policy.contains("unsafe-inline"),
            "{policy}"
        );
    }

    /// A heading's text as a search engine or a screen reader reads it: tags dropped, a line break adding nothing,
    /// and each run of whitespace one space.
    fn heading_texts(page: &str, level: &str) -> Vec<String> {
        page.split(&format!("<{level}"))
            .skip(1)
            .map(|rest| {
                let inner =
                    &rest[rest.find('>').unwrap() + 1..rest.find(&format!("</{level}>")).unwrap()];
                let mut text = String::new();
                let mut in_tag = false;
                for c in inner.chars() {
                    match c {
                        '<' => in_tag = true,
                        '>' => in_tag = false,
                        _ if !in_tag => text.push(c),
                        _ => {}
                    }
                }
                text.split_whitespace().collect::<Vec<_>>().join(" ")
            })
            .collect()
    }

    #[test]
    fn the_visible_headline_is_the_pages_one_h1() {
        for demo in [true, false] {
            assert_eq!(
                heading_texts(&home_page(demo), "h1"),
                ["Tetsu: Booking software, built like iron."],
                "demo {demo}"
            );
        }
    }

    #[test]
    fn no_heading_runs_two_words_together_across_a_line_break() {
        let page = home_page(true);
        let run_together: Vec<String> = ["h1", "h2", "h3"]
            .iter()
            .flat_map(|level| heading_texts(&page, level))
            .filter(|text| {
                text.as_bytes()
                    .windows(2)
                    .any(|pair| b",.".contains(&pair[0]) && pair[1].is_ascii_alphabetic())
            })
            .collect();
        assert!(run_together.is_empty(), "{run_together:?}");
    }

    /// The first SVG after `marker` in `page`, without its closing tag.
    fn svg_after<'a>(page: &'a str, marker: &str) -> &'a str {
        let from = page.find(marker).unwrap();
        let svg = &page[from..][page[from..].find("<svg").unwrap()..];
        &svg[..svg.find("</svg>").unwrap()]
    }

    /// A wordmark's booked cells, and the ones that run hot.
    type Cells = (std::collections::BTreeSet<(i64, i64)>, Vec<(i64, i64)>);

    /// The brand's slots booked in the first SVG after `marker`, as cells on its 30 pixel pitch counted from the
    /// top left one, with the cell that runs hot. A slot is booked if it is filled in the brand's colors, and hot
    /// if in forge orange; the hero board's other bookings, in other fills, are not counted.
    fn booked_cells(page: &str, marker: &str) -> Cells {
        let svg = svg_after(page, marker);
        let attr = |tag: &str, name: &str| {
            tag.split(&format!(" {name}=\""))
                .nth(1)
                .map(|rest| rest[..rest.find('"').unwrap()].to_string())
        };
        let mut cells = Vec::new();
        for tag in svg
            .split("<rect")
            .skip(1)
            .map(|t| &t[..t.find('>').unwrap()])
        {
            let class = attr(tag, "class").unwrap_or_default();
            let fill = attr(tag, "fill").unwrap_or_default();
            let hot = class.contains("tetsu-hot") || fill == "#E5622B";
            if hot || class.contains("fill-current") || fill == "#3A4250" {
                // Booked slots sit on whole pixels, so a coordinate's whole part places its cell.
                let at = |name| {
                    let value = attr(tag, name).unwrap();
                    value.split('.').next().unwrap().parse::<i64>().unwrap() / 30
                };
                cells.push(((at("x"), at("y")), hot));
            }
        }
        let left = cells.iter().map(|((x, _), _)| *x).min().unwrap();
        let top = cells.iter().map(|((_, y), _)| *y).min().unwrap();
        let shift = |(x, y): (i64, i64)| (x - left, y - top);
        let hot = cells
            .iter()
            .filter(|(_, hot)| *hot)
            .map(|(cell, _)| shift(*cell))
            .collect();
        (
            cells.into_iter().map(|(cell, _)| shift(cell)).collect(),
            hot,
        )
    }

    #[test]
    fn the_wordmark_fills_each_cell_of_its_grid_once() {
        // Its open slots come from rows of dots and its booked ones from brand.html, so they must not overlap.
        let page = home_page(false);
        let svg = svg_after(&page, r#"aria-label="Tetsu home""#);
        // A coordinate's whole part, over the 30 pixel pitch, places a rect's cell.
        let cell = |tag: &str, name: &str| {
            let value = tag.split(&format!(" {name}=\"")).nth(1).unwrap();
            value[..value.find(['.', '"']).unwrap()]
                .parse::<i64>()
                .unwrap()
                / 30
        };
        let cells: Vec<_> = svg
            .split("<rect")
            .skip(1)
            .map(|tag| (cell(tag, "x"), cell(tag, "y")))
            .collect();
        let grid: std::collections::BTreeSet<_> =
            (0..19).flat_map(|x| (0..5).map(move |y| (x, y))).collect();
        assert_eq!(cells.len(), grid.len(), "one rect a cell");
        assert_eq!(
            cells.into_iter().collect::<std::collections::BTreeSet<_>>(),
            grid
        );
    }

    #[test]
    fn every_class_on_the_root_page_is_in_the_stylesheet() {
        // scripts/check_templates.py checks only names written inside a class attribute, so a mistyped name in a
        // macro's body, such as art/app.html's grid placement, would pass it. Tailwind drops an unknown name without a
        // word. Marker classes such as group and peer style nothing themselves, as in that check.
        let css = std::str::from_utf8(assets::find(assets::STYLESHEET).unwrap().bytes).unwrap();
        // Each class selector, with Tailwind's backslash escapes removed.
        let mut defined = std::collections::BTreeSet::new();
        let mut chars = css.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '.' {
                continue;
            }
            let mut name = String::new();
            while let Some(&next) = chars.peek() {
                if next == '\\' {
                    chars.next();
                    name.extend(chars.next());
                } else if next.is_ascii_alphanumeric() || next == '-' || next == '_' {
                    name.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            defined.insert(name);
        }
        let page = home_page(true);
        let missing: Vec<_> = page
            .split(" class=\"")
            .skip(1)
            .flat_map(|rest| rest[..rest.find('"').unwrap()].split_whitespace())
            .filter(|name| !defined.contains(*name))
            .filter(|name| !["group", "peer"].contains(&name.split('/').next().unwrap()))
            .collect();
        assert!(
            page.matches(" class=\"").count() > 1000,
            "the page's classes were read"
        );
        assert!(missing.is_empty(), "{missing:?}");
    }

    #[tokio::test]
    async fn the_root_page_and_its_stylesheet_fit_their_budget_as_sent() {
        // ADR-0039: at most 40 KB of HTML and 20 KB of stylesheet, as sent to a browser that accepts brotli.
        let (head, page) = as_sent("/").await;
        assert!(head.contains("content-encoding: br"), "{head}");
        assert!(page <= 40 * 1024, "the root page took {page} bytes");
        let (head, sheet) = as_sent(assets::STYLESHEET).await;
        assert!(head.contains("content-encoding: br"), "{head}");
        assert!(sheet <= 20 * 1024, "the stylesheet took {sheet} bytes");
    }

    #[tokio::test]
    async fn a_font_is_sent_as_it_is_since_it_is_already_compressed() {
        let font = assets::ASSETS
            .iter()
            .find(|asset| asset.content_type == "font/woff2")
            .unwrap();
        let (head, size) = as_sent(font.path).await;
        assert!(!head.contains("content-encoding"), "{head}");
        assert_eq!(size, font.bytes.len());
    }

    #[test]
    fn the_headline_is_on_screen_from_the_first_frame() {
        // The largest text on the page, so it should paint at once: its rise moves it, and never hides it.
        let page = home_page(true);
        assert!(page.contains(r#"<h1 class="tetsu-lift "#), "{page}");
        let css = std::str::from_utf8(assets::find(assets::STYLESHEET).unwrap().bytes).unwrap();
        // The rule ends at its first double brace, built here so the braces in this test's strings stay paired.
        let lift = &css[css.find("@keyframes tetsu-lift{").unwrap()..];
        let lift = &lift[..lift.find(&"}".repeat(2)).unwrap()];
        assert!(!lift.contains("opacity"), "{lift}");
        assert!(!lift.contains("filter"), "{lift}");
    }

    // These tests write markup in escaped strings, not raw ones: lizard (scripts/check.sh) misreads a raw string
    // with a quote inside, and then measures every test after it as one long function.

    /// The keyframes rule `name` in the built stylesheet, up to its first double brace.
    fn keyframes(name: &str) -> String {
        let css = stylesheet();
        let rule = &css[css.find(&format!("@keyframes {name}{{")).unwrap()..];
        rule[..rule.find("}}").unwrap()].to_owned()
    }

    /// The built stylesheet.
    fn stylesheet() -> String {
        String::from_utf8_lossy(assets::find(assets::STYLESHEET).unwrap().bytes).into_owned()
    }

    /// The declarations of the rule whose selector is `selector`, as the build writes it.
    fn rule(selector: &str) -> String {
        let css = stylesheet();
        let body = &css[css.find(&format!("{selector}{{")).unwrap() + selector.len() + 1..];
        body[..body.find('}').unwrap()].to_owned()
    }

    #[test]
    fn the_week_and_the_panels_rise_without_a_blur() {
        // A blur erases a panel's thin border until it clears, and WebKit clips a blurred layer's edges.
        assert!(!keyframes("tetsu-rise-clear").contains("filter"));
        let page = home_page(true);
        assert!(
            page.contains("class=\"tetsu-clock tetsu-rise-clear "),
            "{page}"
        );
        for selector in [
            ".tetsu-rise-clear",
            ".tetsu-rise-clear.tetsu-clock",
            ".tetsu-reveal.tetsu-clock",
        ] {
            let animation = rule(selector);
            assert!(
                animation.contains("tetsu-rise-clear"),
                "{selector}: {animation}"
            );
            assert!(
                !animation.contains("tetsu-rise "),
                "{selector}: {animation}"
            );
        }
    }

    #[test]
    fn each_panel_plays_its_own_story_once_it_is_on_screen() {
        // A section's clock started every panel in it at once, so a panel low on a phone played out of sight.
        let page = home_page(true);
        assert!(
            page.contains("class=\"tetsu-reveal tetsu-clock @container "),
            "{page}"
        );
        assert!(!page.contains("class=\"tetsu-reveal @container "), "{page}");
        let sections = page
            .split("<section")
            .skip(1)
            .map(|rest| &rest[..rest.find('>').unwrap()]);
        assert!(sections.clone().count() > 0);
        for tag in sections {
            assert!(!tag.contains("tetsu-clock"), "{tag}");
        }
        let panel = rule(".tetsu-reveal.tetsu-clock");
        assert!(panel.contains("entry min(100%,75vh)"), "{panel}");
    }

    #[test]
    fn tabbing_to_the_header_does_not_scroll_the_page() {
        // A scroll padding on the page counted the sticky header's own controls as hidden behind it, so jumps and
        // focus inside the page keep clear of the header with a margin instead.
        let page = home_page(true);
        let html = &page[page.find("<html").unwrap()..];
        assert!(
            !html[..html.find('>').unwrap()].contains("scroll-p"),
            "{html}"
        );
        assert_eq!(rule("main [id],main :focus"), "scroll-margin-top:5rem");
    }

    #[test]
    fn each_feature_section_shows_its_panels_in_its_own_arrangement() {
        // Panels arranged in a picture, with no tabs to choose between, and each section in its own arrangement. Every panel appears somewhere on the page, in the order given.
        let page = home_page(true);
        for (id, arrangement, panels) in [
            (
                "fill",
                "card",
                &["classes at a yoga studio", "A 6 am class"][..],
            ),
            (
                "spots",
                "layer",
                &["The layout editor", "Two spot maps"][..],
            ),
            ("book", "card", &["Courts booked", "day by stylist"][..]),
            (
                "paid",
                "layer",
                &["payments arriving", "Three membership plans"][..],
            ),
        ] {
            let section = &page[page.find(&format!("<section id=\"{id}\"")).unwrap()..];
            let section = &section[..section.find("</section>").unwrap()];
            let scene = format!("class=\"tetsu-scene tetsu-scene-{arrangement}\"");
            assert!(section.contains(&scene), "{id}");
            let mut rest = section;
            for panel in panels {
                let at = rest.find(panel).unwrap_or_else(|| panic!("{id}: {panel}"));
                rest = &rest[at..];
            }
            assert!(section.contains(">Features</p>"), "{id}");
            assert!(!section.contains("<details"), "{id}");
        }
        // The stories play inside .tetsu-stories: the week, and each picture's panel in front. A panel behind shows
        // its finished state.
        assert!(page.contains("class=\"tetsu-stories mx-auto"), "{page}");
        assert_eq!(
            page.matches("tetsu-scene-front tetsu-stories").count(),
            page.matches("tetsu-scene-front").count()
        );
        assert!(!page.contains("tetsu-scene-back tetsu-stories"), "{page}");
        assert!(rule(".tetsu-stories .tetsu-in").contains("opacity:0"));
        assert!(!stylesheet().contains("}.tetsu-in{opacity:0"));
    }

    #[test]
    fn phones_show_only_the_panel_in_front() {
        // Below 48rem each section shows its front panel alone: the view behind is hidden, and only the stage
        // for wider screens, which also zooms the panels to fit, shows it.
        assert_eq!(rule(".tetsu-scene-back"), "display:none");
        let css = stylesheet();
        let wide = &css[css
            .find("@media (width>=48rem){.tetsu-scene{zoom:")
            .unwrap()..];
        let wide = &wide[..wide[1..].find("@media").unwrap()];
        assert!(
            wide.contains(".tetsu-scene-back{width:68rem;display:block"),
            "{wide}"
        );
    }

    #[test]
    fn the_phone_menu_is_a_popover_that_its_button_opens() {
        // A popover, so Escape and a tap outside close it without script (site.html).
        let page = home_page(true);
        assert!(
            page.contains("<button type=\"button\" popovertarget=\"tetsu-menu\""),
            "{page}"
        );
        assert!(page.contains("<div id=\"tetsu-menu\" popover "), "{page}");
    }

    /// Whether a declaration moves something: any animation but tetsu-play, or a transition of a transform or of
    /// every property. Fades of color and opacity are not movement.
    fn moves(declaration: &str) -> bool {
        let (property, value) = declaration.split_once(':').unwrap_or_default();
        match property.trim() {
            "animation" | "animation-name" => {
                // Only tetsu-play by its whole name; a name that merely contains it moves things.
                value
                    .split(',')
                    .any(|one| !one.split_whitespace().any(|word| word == "tetsu-play"))
            }
            "transition" | "transition-property" => {
                ["all", "transform", "translate", "scale", "rotate"]
                    .iter()
                    .any(|word| value.contains(word))
            }
            _ => false,
        }
    }

    /// Each declaration in a stylesheet that moves something, and whether a reduced-motion guard wraps it.
    fn movements(css: &str) -> Vec<(&str, bool)> {
        let mut blocks: Vec<&str> = Vec::new();
        let (mut start, mut found) = (0, Vec::new());
        for (at, c) in css.char_indices().filter(|(_, c)| "{;}".contains(*c)) {
            let text = css[start..at].trim();
            start = at + 1;
            if c == '{' {
                blocks.push(text);
                continue;
            }
            if moves(text) {
                let guarded = blocks
                    .iter()
                    .any(|block| block.contains("prefers-reduced-motion:no-preference"));
                found.push((text, guarded));
            }
            if c == '}' {
                blocks.pop();
            }
        }
        found
    }

    #[test]
    fn nothing_loops_and_nothing_moves_when_the_visitor_asks_for_less() {
        // motion.css's promises (WCAG 2.2.2 and 2.3.3). tetsu-play animates only a custom property that tells a
        // figure to show itself on touch screens, so it moves nothing itself. Only the pending slots loop, while a
        // request a person started runs, and they only fade: every loop is theirs, and their keyframes set opacity
        // alone.
        let css = std::str::from_utf8(assets::find(assets::STYLESHEET).unwrap().bytes).unwrap();
        let loops: Vec<&str> = css
            .match_indices("infinite")
            .map(|(at, _)| {
                let start = css[..at].rfind([';', '{']).unwrap() + 1;
                let end = at + css[at..].find([';', '}']).unwrap();
                &css[start..end]
            })
            // A class Tailwind names after its value holds the word too, but a selector loops nothing.
            .filter(|found| !found.contains('{'))
            .collect();
        assert_eq!(loops.len(), 3, "{loops:?}");
        for one in &loops {
            assert!(
                one.starts_with("animation:") && one.ends_with(" infinite tetsu-pending"),
                "{one} loops"
            );
        }
        // Every block of each name, whole, since a later block of the same name would win.
        for (name, whole) in [
            (
                "tetsu-pending",
                "@keyframes tetsu-pending{0%,to{opacity:.25}40%{opacity:1}}",
            ),
            ("tetsu-play", "@keyframes tetsu-play{to{--tetsu-play:1}}"),
        ] {
            let blocks: Vec<&str> = css
                .match_indices(&format!("@keyframes {name}{{"))
                .map(|(at, _)| &css[at..at + css[at..].find("}}").unwrap() + 2])
                .collect();
            assert_eq!(blocks, [whole], "{name}");
        }
        let found = movements(css);
        let unguarded: Vec<_> = found.iter().filter(|(_, guarded)| !guarded).collect();
        assert!(found.len() > 5, "only {} movements were read", found.len());
        assert!(unguarded.is_empty(), "{unguarded:?}");
    }

    #[test]
    fn the_dark_tokens_follow_the_system_unless_the_visitor_picks() {
        // Tailwind's dark variant sets the dark tokens (tetsu.css): in a dark system scheme unless the visitor picks
        // Light, and whenever they pick Dark. The pick is the footer control's radio, and before the browser has read
        // the footer, the scheme the server wrote on <html> (ADR-0070). Each rule that sets one, with its wrapper.
        let css = std::str::from_utf8(assets::find(assets::STYLESHEET).unwrap().bytes).unwrap();
        let rules: Vec<(&str, &str)> = css
            .match_indices("--tetsu-color-canvas:#111110")
            .map(|(at, _)| {
                let open = css[..at].rfind('{').unwrap();
                let start = css[..open].rfind(['{', '}']).unwrap() + 1;
                (&css[..start], &css[start..open])
            })
            .collect();
        let page = home_page(true);
        for id in [
            "tetsu-scheme-system",
            "tetsu-scheme-light",
            "tetsu-scheme-dark",
        ] {
            assert!(page.contains(&format!("id=\"{id}\"")), "{id}");
        }
        assert_eq!(rules.len(), 2, "{rules:?}");
        let (before, system) = rules[0];
        assert!(
            before.ends_with("@media (prefers-color-scheme:dark){"),
            "{system}"
        );
        assert!(
            system.contains(
                ":root:not(:has(#tetsu-scheme-light:checked)):not([data-scheme=light]:not(:has(#tetsu-scheme-system)))"
            ),
            "{system}"
        );
        for picked in [
            ":root:has(#tetsu-scheme-dark:checked)",
            ":root[data-scheme=dark]:not(:has(#tetsu-scheme-system))",
        ] {
            assert!(rules[1].1.contains(picked), "{picked} in {rules:?}");
        }
    }

    #[test]
    fn no_text_on_the_root_page_is_escaped_twice() {
        // A macro escapes its arguments, `&` as `&#38;`, so an argument written as an entity, such as `&amp;`,
        // would show the entity's code.
        let page = home_page(true);
        for entity in ["amp;", "lt;", "gt;", "quot;", "#"] {
            assert!(
                !page.contains(&format!("&#38;{entity}")),
                "&{entity} escaped twice"
            );
        }
    }

    #[tokio::test]
    async fn the_not_found_page_books_404_in_the_brands_slots() {
        let response = super::not_found(true, Scheme::System);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let page = String::from_utf8(body.to_vec()).unwrap();
        let (booked, hot) = booked_cells(&page, "<main");
        assert_eq!((booked.len(), hot), (30, vec![(2, 1)]));
    }

    #[test]
    fn the_wordmark_and_the_hero_board_book_the_brand_files_slots() {
        let brand = booked_cells(
            include_str!("../../../docs/brand/tetsu-wordmark-light.svg"),
            "<svg",
        );
        assert_eq!(brand.0.len(), 50);
        assert_eq!(brand.1.len(), 1);
        assert_eq!(
            booked_cells(&home_page(true), r#"aria-label="Tetsu home""#),
            brand,
            "the header's wordmark"
        );
        assert_eq!(
            booked_cells(&home_page(true), r#"class="tetsu-board""#),
            brand,
            "the hero board"
        );
    }

    #[tokio::test]
    async fn only_a_demo_root_page_shows_fictional_customers_and_figures() {
        let demo = get_path("/").await;
        assert!(
            demo.contains("Ironbark") && demo.contains("1,200"),
            "{demo}"
        );
        // Outside a demo, the page's main holds the hero and an empty spacer, and nothing else (ADR-0037).
        let outside = send_to(false, &get_request("/")).await;
        let main = &outside[outside.find("<main").unwrap()..outside.find("</main>").unwrap()];
        let hero =
            main.find("<section").unwrap()..main.find("</section>").unwrap() + "</section>".len();
        assert!(main[hero.clone()].contains("Booking software,"), "{main}");
        let rest = format!(
            "{}{}",
            &main[main.find('>').unwrap() + 1..hero.start],
            &main[hero.end..]
        );
        assert_eq!(
            rest.split_whitespace().collect::<String>(),
            "<divclass=\"pb-32\"></div>",
            "{main}"
        );
    }

    #[tokio::test]
    async fn it_stops_when_asked() {
        let (_, stop, server) = stoppable(GRACE).await;
        stop.send(()).unwrap();
        server.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn a_client_that_never_finishes_its_request_cannot_hold_the_server_open() {
        let (address, stop, server) = stoppable(Duration::from_millis(200)).await;
        let mut stalled = TcpStream::connect(address).await.unwrap();
        stalled
            .write_all(b"GET / HTTP/1.1\r\nHost: test\r\n")
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        stop.send(()).unwrap();
        let finished = tokio::time::timeout(Duration::from_secs(5), server).await;
        assert!(
            finished.is_ok(),
            "the server was still running 5 seconds after it was asked to stop"
        );
        drop(stalled);
    }
}
