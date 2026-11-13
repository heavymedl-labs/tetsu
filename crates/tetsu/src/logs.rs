//! Logs (ADR-0045): one JSON object a line on standard output, which every host collects, and the same lines
//! exported over OTLP when an endpoint is set (ADR-0049). Each request's lines carry its request ID, the realm, the
//! method, the route that answered, and the tenant once one is found, and never the path, query, or body, which
//! could carry a name, an email, a code, or a token.
//!
//! The span fields in [`SPAN_FIELDS`] and the values of `alert` are a contract with the backend's alert rules,
//! written down in ADR-0049. Renaming one breaks a rule there without failing anything here.

use std::hash::{BuildHasher, RandomState};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use opentelemetry::KeyValue;
use opentelemetry_appender_tracing::layer::{OpenTelemetryTracingBridge, TracingSpanAttributes};
use opentelemetry_otlp::{Compression, LogExporter, Protocol, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use tracing::Instrument;
use tracing::field::Empty;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::Layer;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;

use crate::settings::Export;

/// The realm this server serves. Only the customer realm's public pages exist until operators sign in, when the
/// realm becomes a setting (ADR-0048, slice 5).
const REALM: &str = "customer";

/// The span fields an exported line carries. Any other span field stays on this host until a reviewer checks it
/// names no person and adds it here (ADR-0049). Each is set on the request span alone, and recorded once: the
/// exporter copies a field again for each time it is recorded and each span that declares it.
pub const SPAN_FIELDS: [&str; 5] = [
    "request_id",
    "realm",
    "http.request.method",
    "http.route",
    "tenant_id",
];

/// Logs JSON lines to `writer`, and exports them through `export` when it is given. Each event's fields are at the
/// top level of a line, and the fields of the span it happened in under `span`. Tetsu's own crates log from info
/// up. Other crates log only warnings and errors, since no one has checked what they write for personal details.
/// The exporter's own crates and the HTTP stack it sends through are never exported, so a failing export cannot
/// log itself in a loop; their warnings and errors, such as a refused key, stay on standard output.
pub fn subscriber<W>(
    writer: W,
    export: Option<&SdkLoggerProvider>,
) -> impl tracing::Subscriber + Send + Sync
where
    W: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
{
    let reviewed = Targets::new()
        .with_target("tetsu", LevelFilter::INFO)
        .with_default(LevelFilter::WARN);
    let not_the_exporter = [
        "opentelemetry",
        "reqwest",
        "hyper",
        "hyper_util",
        "hyper_rustls",
        "h2",
        "rustls",
    ]
    .into_iter()
    .fold(Targets::new(), |targets, target| {
        targets.with_target(target, LevelFilter::OFF)
    })
    .with_default(LevelFilter::TRACE);
    let lines = tracing_subscriber::fmt::layer()
        .json()
        .flatten_event(true)
        .with_current_span(true)
        .with_span_list(false)
        .with_target(false)
        .with_writer(writer);
    let exported = export.map(|provider| {
        OpenTelemetryTracingBridge::builder(provider)
            .with_tracing_span_attributes(TracingSpanAttributes::allowlist(SPAN_FIELDS))
            .build()
            .with_filter(not_the_exporter)
    });
    tracing_subscriber::registry()
        .with(lines)
        .with(exported)
        .with(reviewed)
}

/// This process's logs, which flush what is left to export when stopped.
pub struct Logs(Option<SdkLoggerProvider>);

impl Logs {
    /// Sends what is still batched, waiting 5 seconds at most. It blocks the thread it runs on, which is fine as
    /// the last thing the process does.
    pub fn stop(self) {
        if let Some(provider) = self.0
            && let Err(error) = provider.shutdown()
        {
            tracing::warn!(error = %error, "could not send the last logs");
        }
    }
}

/// Sends this process's logs to standard output, and exports them as `export` says. Logging is for operating
/// Tetsu, so a bad export setting is logged as a warning and the process runs without export, rather than
/// failing to start. A panic is logged as a server error from here on.
///
/// # Errors
/// Fails if logging was already started.
pub fn start(export: Result<Option<Export>, String>) -> Result<Logs, String> {
    let (provider, problem) = match export.and_then(|export| export.map(exporter).transpose()) {
        Ok(provider) => (provider, None),
        Err(problem) => (None, Some(problem)),
    };
    tracing::subscriber::set_global_default(subscriber(std::io::stdout, provider.as_ref()))
        .map_err(|error| format!("could not start logging: {error}"))?;
    if let Some(problem) = problem {
        tracing::warn!("logs are not exported: {problem}");
    }
    report_panics();
    Ok(Logs(provider))
}

/// Exports over OTLP with HTTP and protobuf, in batches on the SDK's own thread, so no request waits on the
/// backend.
fn exporter(export: Export) -> Result<SdkLoggerProvider, String> {
    let mut builder = LogExporter::builder()
        .with_http()
        .with_protocol(Protocol::HttpBinary)
        .with_endpoint(export.endpoint)
        .with_headers(export.headers);
    if export.gzip {
        builder = builder.with_compression(Compression::Gzip);
    }
    // The exporter's error can quote the endpoint, which may carry a credential, so only its kind is kept.
    let exporter = builder
        .build()
        .map_err(|_| "could not set up export with these settings".to_string())?;
    Ok(SdkLoggerProvider::builder()
        .with_resource(resource(export.service_name, export.resource))
        .with_batch_exporter(exporter)
        .build())
}

/// The attributes naming this process to the backend. The service name is applied last, so an attribute cannot
/// replace it, as OpenTelemetry specifies, and alert rules that select on it keep matching.
fn resource(service_name: String, attributes: Vec<(String, String)>) -> Resource {
    Resource::builder_empty()
        .with_attributes(
            attributes
                .into_iter()
                .map(|(key, value)| KeyValue::new(key, value)),
        )
        .with_service_name(service_name)
        .build()
}

/// Logs each panic as a server error with where it happened, then lets the default hook print it. The panic's
/// message stays off the log, since it can quote any value.
fn report_panics() {
    let printed = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        let location = panic.location().map(ToString::to_string);
        tracing::error!(alert = "server_error", location, "panicked");
        printed(panic);
    }));
}

/// Runs a request inside its own span, then logs one line with its status and how long it took, marked as a
/// server error alert when the status is 5xx. A module records the tenant on the span as `tenant_id` once it knows
/// it, and [`route`] records the route that answered.
pub async fn request(request: Request, next: Next) -> Response {
    let span = tracing::info_span!(
        "request",
        request_id = request_id(),
        realm = REALM,
        http.request.method = method_name(request.method()),
        http.route = Empty,
        tenant_id = Empty,
    );
    let started = Instant::now();
    async move {
        let response = next.run(request).await;
        served(response.status(), started.elapsed());
        response
    }
    .instrument(span)
    .await
}

/// Logs a response's status and how long it took, marked for the server error alert when the status is 5xx.
fn served(status: axum::http::StatusCode, took: std::time::Duration) {
    let ms = u64::try_from(took.as_millis()).unwrap_or(u64::MAX);
    let alert = status.is_server_error().then_some("server_error");
    tracing::info!(status = status.as_u16(), ms, alert, "served");
}

/// Records the route pattern that answered, such as `/{address}`, never the path that matched it.
pub async fn route(request: Request, next: Next) -> Response {
    if let Some(route) = request.extensions().get::<MatchedPath>() {
        tracing::Span::current().record("http.route", route.as_str());
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
        tracing::subscriber::with_default(subscriber(move || writer.clone(), None), || {
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

    /// What `subscriber` exports, captured in memory, after `run` logs through it.
    fn exported(run: impl FnOnce()) -> Vec<opentelemetry_sdk::logs::SdkLogRecord> {
        let memory = opentelemetry_sdk::logs::InMemoryLogExporter::default();
        let provider = SdkLoggerProvider::builder()
            .with_simple_exporter(memory.clone())
            .build();
        tracing::subscriber::with_default(subscriber(std::io::sink, Some(&provider)), run);
        memory
            .get_emitted_logs()
            .unwrap()
            .into_iter()
            .map(|log| log.record)
            .collect()
    }

    fn keys(record: &opentelemetry_sdk::logs::SdkLogRecord) -> Vec<String> {
        record
            .attributes_iter()
            .map(|(key, _)| key.as_str().to_string())
            .collect()
    }

    /// Only allowlisted span fields leave the host, each once, and a nested span's own fields stay here (ADR-0049).
    #[test]
    fn an_exported_line_carries_only_allowlisted_span_fields_once_each() {
        let records = exported(|| {
            let request = tracing::info_span!(
                "request",
                request_id = "r-1",
                realm = REALM,
                http.request.method = "GET",
                http.route = Empty,
                tenant_id = Empty,
                email = "lee.park@example.com",
            );
            let _request = request.enter();
            request.record("http.route", "/{address}");
            request.record("tenant_id", 7);
            let inner = tracing::info_span!("inner", note = "kept here");
            let _inner = inner.enter();
            tracing::info!(target: "tetsu::serve", status = 200, "served");
        });
        assert_eq!(records.len(), 1);
        let mut span_keys: Vec<String> = keys(&records[0])
            .into_iter()
            .filter(|key| !["status"].contains(&key.as_str()))
            .collect();
        span_keys.sort();
        let mut allowed: Vec<String> = SPAN_FIELDS.iter().map(ToString::to_string).collect();
        allowed.sort();
        assert_eq!(span_keys, allowed);
    }

    /// The exporter's own crates and its HTTP stack are never exported, so a failing export cannot log itself in
    /// a loop, and what standard output leaves out is not exported either.
    #[test]
    fn the_exporter_never_exports_its_own_lines() {
        let records = exported(|| {
            tracing::warn!(target: "opentelemetry_sdk", "export failed");
            tracing::warn!(target: "hyper_util::client", "connection failed");
            tracing::info!(target: "deadpool_postgres", "not reviewed");
            tracing::warn!(target: "deadpool_postgres", "a warning kept");
        });
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].target().map(AsRef::as_ref),
            Some("deadpool_postgres")
        );
    }

    /// An attribute cannot replace the service name, which the alert rules select on.
    #[test]
    fn the_service_name_wins_over_an_attribute_of_the_same_name() {
        let resource = resource(
            "tetsu".into(),
            vec![
                ("service.name".into(), "other".into()),
                ("deployment.environment.name".into(), "demo".into()),
            ],
        );
        let get = |key: &'static str| {
            resource
                .get(&opentelemetry::Key::new(key))
                .map(|value| value.to_string())
        };
        assert_eq!(get("service.name"), Some("tetsu".into()));
        assert_eq!(get("deployment.environment.name"), Some("demo".into()));
    }

    /// A 5xx response's line names the server error alert; others carry no alert.
    #[test]
    fn a_server_error_is_marked_for_its_alert() {
        let lines = Lines::default();
        let writer = lines.clone();
        tracing::subscriber::with_default(subscriber(move || writer.clone(), None), || {
            for status in [500, 503, 404, 200] {
                let status = axum::http::StatusCode::from_u16(status).unwrap();
                served(status, std::time::Duration::from_millis(3));
            }
        });
        let text = String::from_utf8(lines.0.lock().unwrap().clone()).unwrap();
        let alerts: Vec<bool> = text
            .lines()
            .map(|line| line.contains("\"alert\":\"server_error\""))
            .collect();
        assert_eq!(alerts, [true, true, false, false], "{text}");
        assert!(text.contains("\"ms\":3"), "{text}");
    }

    /// A panic is logged as a server error, with where it happened and never its message, which can quote any
    /// value.
    #[test]
    fn a_panic_is_logged_as_a_server_error_without_its_message() {
        report_panics();
        let lines = Lines::default();
        let writer = lines.clone();
        let panicked = std::thread::spawn(move || {
            tracing::subscriber::with_default(subscriber(move || writer.clone(), None), || {
                panic!("lee.park@example.com");
            });
        })
        .join();
        assert!(panicked.is_err());
        let text = String::from_utf8(lines.0.lock().unwrap().clone()).unwrap();
        assert!(text.contains("\"alert\":\"server_error\""), "{text}");
        assert!(text.contains("logs.rs"), "{text}");
        assert!(!text.contains("lee.park"), "{text}");
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
