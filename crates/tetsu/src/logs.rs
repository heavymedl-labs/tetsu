//! Logs (ADR-0045): one JSON object a line on standard output, which every host collects. Each request's lines
//! carry its request ID, the realm, the route that answered, and the tenant once one is found, and never the path,
//! query, or body, which could carry a name, an email, a code, or a token.

use std::hash::{BuildHasher, RandomState};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use tracing::Instrument;
use tracing::field::Empty;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;

/// The realm this server serves. Only the customer realm's public pages exist until operators sign in, when the
/// realm becomes a setting (ADR-0048, slice 5).
const REALM: &str = "customer";

/// Logs JSON lines to `writer`: each event's fields at the top level, and the fields of the span it happened in
/// under `span`. Tetsu's own crates log from info up. Other crates log only warnings and errors, since no one has
/// checked what they write for personal details.
pub fn subscriber<W>(writer: W) -> impl tracing::Subscriber + Send + Sync
where
    W: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
{
    let reviewed = Targets::new()
        .with_target("tetsu", tracing::Level::INFO)
        .with_default(tracing::Level::WARN);
    tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(true)
        .with_span_list(false)
        .with_target(false)
        .with_max_level(tracing::Level::INFO)
        .with_writer(writer)
        .finish()
        .with(reviewed)
}

/// Sends this process's logs to standard output.
///
/// # Errors
/// Fails if logging was already started.
pub fn start() -> Result<(), String> {
    tracing::subscriber::set_global_default(subscriber(std::io::stdout))
        .map_err(|error| format!("could not start logging: {error}"))
}

/// Runs a request inside its own span, then logs one line with its status and how long it took. A module records
/// the tenant on the span as `tenant_id` once it knows it, and [`route`] records the route that answered.
pub async fn request(request: Request, next: Next) -> Response {
    let span = tracing::info_span!(
        "request",
        request_id = request_id(),
        realm = REALM,
        method = method_name(request.method()),
        route = Empty,
        tenant_id = Empty,
    );
    let started = Instant::now();
    async move {
        let response = next.run(request).await;
        let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        tracing::info!(status = response.status().as_u16(), ms, "served");
        response
    }
    .instrument(span)
    .await
}

/// Records the route pattern that answered, such as `/{address}`, never the path that matched it.
pub async fn route(request: Request, next: Next) -> Response {
    if let Some(route) = request.extensions().get::<MatchedPath>() {
        tracing::Span::current().record("route", route.as_str());
    }
    next.run(request).await
}

/// The method as sent if HTTP defines it, and `OTHER` if not, since a client can send any text as a method.
fn method_name(method: &axum::http::Method) -> &'static str {
    const DEFINED: [&str; 9] = [
        "GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "CONNECT", "TRACE",
    ];
    DEFINED
        .into_iter()
        .find(|defined| *defined == method.as_str())
        .unwrap_or("OTHER")
}

/// A request's ID: a random number for this process, then a count of its requests. It is unique, not secret.
fn request_id() -> String {
    static PROCESS: OnceLock<u64> = OnceLock::new();
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let process = PROCESS.get_or_init(|| RandomState::new().hash_one(std::process::id()));
    format!("{process:016x}-{}", COUNT.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lines a test's subscriber writes.
    #[derive(Clone, Default)]
    struct Lines(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Lines {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Tetsu's own crates log from info up; anything else only from warn up, since no one reviewed what it writes
    /// for personal details.
    #[test]
    fn other_crates_log_only_warnings_and_errors() {
        let lines = Lines::default();
        let writer = lines.clone();
        tracing::subscriber::with_default(subscriber(move || writer.clone()), || {
            tracing::info!(target: "tetsu_tenancy::adapters", "kept");
            tracing::info!(target: "deadpool_postgres", "dropped");
            tracing::warn!(target: "deadpool_postgres", "a warning kept");
            tracing::debug!(target: "tetsu", "dropped too");
        });
        let text = String::from_utf8(lines.0.lock().unwrap().clone()).unwrap();
        assert!(text.contains("\"kept\""), "{text}");
        assert!(text.contains("a warning kept"), "{text}");
        assert!(!text.contains("dropped"), "{text}");
    }

    /// A method is client text, so one HTTP does not define is logged as OTHER.
    #[test]
    fn only_known_methods_are_logged_as_sent() {
        for (method, shown) in [
            ("GET", "GET"),
            ("HEAD", "HEAD"),
            ("POST", "POST"),
            ("PUT", "PUT"),
            ("PATCH", "PATCH"),
            ("DELETE", "DELETE"),
            ("OPTIONS", "OPTIONS"),
            ("CONNECT", "CONNECT"),
            ("TRACE", "TRACE"),
            ("LEE.PARK-EXAMPLE", "OTHER"),
        ] {
            let method = axum::http::Method::from_bytes(method.as_bytes()).unwrap();
            assert_eq!(method_name(&method), shown);
        }
    }

    #[test]
    fn request_ids_share_the_process_part_and_count_up() {
        let (first, second) = (request_id(), request_id());
        let (process, count) = first.split_once('-').expect("two parts");
        assert_eq!(process.len(), 16);
        assert!(second.starts_with(&format!("{process}-")));
        let next: u64 = second
            .split_once('-')
            .expect("two parts")
            .1
            .parse()
            .expect("a count");
        assert!(next > count.parse::<u64>().expect("a count"));
    }
}
