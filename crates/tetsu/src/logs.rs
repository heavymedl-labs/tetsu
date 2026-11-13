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
use tracing_subscriber::fmt::MakeWriter;

/// The realm this server serves. Only the customer realm's public pages exist until operators sign in, when the
/// realm becomes a setting (ADR-0048, slice 5).
const REALM: &str = "customer";

/// Logs at info level and above as JSON lines to `writer`, each event's fields at the top level beside the
/// fields of the span it happened in.
pub fn subscriber<W>(writer: W) -> impl tracing::Subscriber + Send + Sync
where
    W: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
{
    tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(true)
        .with_span_list(false)
        .with_target(false)
        .with_max_level(tracing::Level::INFO)
        .with_writer(writer)
        .finish()
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
        method = request.method().as_str(),
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
