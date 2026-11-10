//! `tetsu serve`: the public site, Tetsu's own page at the root and each tenant's page at its address (#76).

use std::future::{Future, IntoFuture};
use std::time::Duration;

use askama::Template;

use crate::assets;
use axum::Router;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{MethodRouter, get};

/// Tetsu's own page at the root.
#[derive(Template)]
#[template(path = "home.html")]
struct HomePage {
    demo: bool,
    stylesheet: &'static str,
}

/// The site's page for a path nothing answers.
#[derive(Template)]
#[template(path = "not_found.html")]
struct NotFoundPage {
    demo: bool,
    stylesheet: &'static str,
}

/// How long requests in progress may take to finish once the server is asked to stop. A client that never
/// finishes its request cannot hold the server open past it.
pub const GRACE: Duration = Duration::from_secs(10);

/// Sent with every response (ADR-0029). The pages run no scripts and load only Tetsu's own stylesheets, so the
/// policy allows nothing else, not even an inline style, and no site may frame them (ADR-0030). HSTS lasts a year, the least ASVS
/// 3.4.1 allows.
const HEADERS: [(HeaderName, &str); 5] = [
    (
        header::STRICT_TRANSPORT_SECURITY,
        "max-age=31536000; includeSubDomains",
    ),
    (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    (
        header::CONTENT_SECURITY_POLICY,
        "default-src 'none'; style-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
    ),
    (header::REFERRER_POLICY, "no-referrer"),
    (header::X_FRAME_OPTIONS, "DENY"),
];

/// A hashed file changes name whenever it changes, so it may be kept for a year (ADR-0034).
const IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// The whole site: its own routes, then the modules' routers, then `not_found` at any path none of them
/// answers, which is [`not_found`] outside tests. Modules set no fallback, since axum panics when two merged
/// routers both have one (#90). In a demo environment every response also asks search engines not to index it
/// (ADR-0018).
pub fn app(modules: Router, demo: bool, not_found: fn(bool) -> Response) -> Router {
    site_routes(demo)
        .into_iter()
        .fold(Router::new(), |site, (path, route)| site.route(path, route))
        .merge(modules)
        .fallback(move || std::future::ready(not_found(demo)))
        .layer(axum::middleware::map_response_with_state(demo, secure))
}

/// The site's page for a path nothing answers. It is the site's own, since a module's page would speak for
/// every other module's paths too.
pub fn not_found(demo: bool) -> Response {
    html(
        StatusCode::NOT_FOUND,
        &NotFoundPage {
            demo,
            stylesheet: assets::STYLESHEET,
        },
    )
}

/// A built file at the exact path the manifest names; any other path under `/assets/` is not found.
fn asset(file: &str, demo: bool) -> Response {
    let Some(found) = assets::find(&format!("/assets/{file}")) else {
        return not_found(demo);
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
    let Ok(body) = page.render() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let mut response = (status, body).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
}

/// Tetsu's own routes. Every route the site adds goes here, never straight into [`app`], and each starts with a
/// name no tenant may hold, which a test checks, so none hides a tenant. Routes in the modules' routers are not
/// checked yet; #20, the first module to add top-level routes, adds that check.
fn site_routes(demo: bool) -> Vec<(&'static str, MethodRouter)> {
    vec![
        (
            "/",
            get(move || {
                let page = HomePage {
                    demo,
                    stylesheet: assets::STYLESHEET,
                };
                std::future::ready(html(StatusCode::OK, &page))
            }),
        ),
        (
            "/assets/{file}",
            get(move |file: Result<Path<String>, PathRejection>| {
                // A path that does not decode, such as one that is not UTF-8, names no built file.
                std::future::ready(
                    file.map_or_else(|_| not_found(demo), |Path(file)| asset(&file, demo)),
                )
            }),
        ),
    ]
}

async fn secure(State(demo): State<bool>, mut response: Response) -> Response {
    let headers = response.headers_mut();
    for (name, value) in HEADERS {
        headers.insert(name, HeaderValue::from_static(value));
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
    let server = axum::serve(listener, app)
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
            "default-src 'none'; style-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
        ),
        ("referrer-policy", "no-referrer"),
        ("x-frame-options", "DENY"),
    ];
    const POST: &str =
        "POST / HTTP/1.1\r\nHost: test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

    /// Stands in for a module's router: a page at one path, and no fallback, which is the site's alone.
    fn module() -> Router {
        Router::new().route("/studio", get(|| async { "studio" }))
    }

    /// Stands in for the site's own not found page, so these tests can tell which page answered.
    fn not_found(demo: bool) -> Response {
        (
            StatusCode::NOT_FOUND,
            format!("demo {demo}: module not found"),
        )
            .into_response()
    }

    /// Sends one request to the site on a port of its own, and returns the raw response.
    async fn send_to(demo: bool, request: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(run(
            listener,
            app(module(), demo, not_found),
            std::future::pending(),
            GRACE,
        ));
        let mut stream = TcpStream::connect(address).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut raw = String::new();
        stream.read_to_string(&mut raw).await.unwrap();
        raw
    }

    async fn send(request: &str) -> String {
        send_to(true, request).await
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
        assert!(raw.contains("<h1>Tetsu</h1>"), "{raw}");
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
        assert!(deeper.ends_with("demo true: module not found"), "{deeper}");
        let outside = send_to(false, &get_request("/elsewhere")).await;
        assert!(
            outside.ends_with("demo false: module not found"),
            "{outside}"
        );
    }

    #[tokio::test]
    async fn the_sites_not_found_page_is_html_marked_for_its_environment() {
        for demo in [true, false] {
            let response = super::not_found(demo);
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            assert_eq!(
                response.headers().get(header::CONTENT_TYPE).unwrap(),
                "text/html; charset=utf-8"
            );
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let body = String::from_utf8(body.to_vec()).unwrap();
            assert!(body.contains("Nothing is at this address."), "{body}");
            assert_eq!(body.contains("Demo."), demo, "{body}");
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

    /// A route at a name a tenant could hold would hide that tenant (#90), so each of the site's own routes
    /// starts with a segment no slug may be.
    #[test]
    fn every_route_the_site_serves_starts_with_a_reserved_name() {
        for (path, _) in site_routes(true) {
            let segment = path.trim_start_matches('/').split('/').next().unwrap_or("");
            if !segment.is_empty() {
                assert!(tetsu_tenancy::Slug::parse(segment).is_err(), "{path}");
            }
        }
    }

    #[tokio::test]
    async fn every_response_carries_the_security_headers() {
        for raw in [
            get_path("/").await,
            get_path("/studio").await,
            get_path("/elsewhere").await,
            send(POST).await,
        ] {
            for (name, value) in EXPECTED {
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
            app(module(), true, not_found),
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
    async fn the_root_page_says_when_the_site_shows_demos() {
        assert!(get_path("/").await.contains("<strong>Demo.</strong>"));
        assert!(!send_to(false, &get_request("/")).await.contains("Demo"));
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
