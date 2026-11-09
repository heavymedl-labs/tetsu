//! `tetsu serve`: the public site, Tetsu's own page at the root and each tenant's page at its address (#76).

use std::future::{Future, IntoFuture};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderName, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

const HOME: &str = include_str!("home.html");
const HOME_DEMO: &str = "<p class=\"demo\" role=\"note\"><strong>Demo.</strong> The businesses on this site are \
                         fictional, made up to show what Tetsu does.</p>";

/// How long requests in progress may take to finish once the server is asked to stop. A client that never
/// finishes its request cannot hold the server open past it.
pub const GRACE: Duration = Duration::from_secs(10);

/// Sent with every response (ADR-0029). The pages run no scripts and load nothing from elsewhere, so the policy
/// allows only their own inline styles, and no site may frame them (ADR-0030). HSTS lasts a year, the least ASVS
/// 3.4.1 allows.
const HEADERS: [(HeaderName, &str); 5] = [
    (
        header::STRICT_TRANSPORT_SECURITY,
        "max-age=31536000; includeSubDomains",
    ),
    (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    (
        header::CONTENT_SECURITY_POLICY,
        "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
    ),
    (header::REFERRER_POLICY, "no-referrer"),
    (header::X_FRAME_OPTIONS, "DENY"),
];

/// The whole site: the root page, then the modules' routers, which answer every other path. In a demo
/// environment every response also asks search engines not to index it (ADR-0018).
pub fn app(tenancy: Router, demo: bool) -> Router {
    let marking = if demo { HOME_DEMO } else { "" };
    let page = HOME.replace("<!-- demo -->", marking);
    Router::new()
        .route("/", get(move || std::future::ready(home(page.clone()))))
        .merge(tenancy)
        .layer(axum::middleware::map_response_with_state(demo, secure))
}

fn home(page: String) -> Response {
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], page).into_response()
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
    use axum::http::StatusCode;
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
            "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
        ),
        ("referrer-policy", "no-referrer"),
        ("x-frame-options", "DENY"),
    ];
    const POST: &str =
        "POST / HTTP/1.1\r\nHost: test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

    /// Stands in for a module's router: a page at one path, and its own not found page everywhere else.
    fn module() -> Router {
        Router::new()
            .route("/studio", get(|| async { "studio" }))
            .fallback(|| async { (StatusCode::NOT_FOUND, "module not found") })
    }

    /// Sends one request to the site on a port of its own, and returns the raw response.
    async fn send_to(demo: bool, request: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(run(
            listener,
            app(module(), demo),
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
        let server = tokio::spawn(run(listener, app(module(), true), shutdown, grace));
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
