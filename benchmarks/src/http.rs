//! HTTP framework suite: the same two endpoints in each framework and in raw
//! hyper with homegrown routing. `run.sh` loads them with `oha`.

use serde::Serialize;

#[derive(Serialize)]
struct Item {
    id: u64,
    name: &'static str,
    coach: &'static str,
    spots: u32,
}

fn items() -> Vec<Item> {
    (0..50)
        .map(|i| Item {
            id: i,
            name: "Strength & Conditioning",
            coach: "Lee Park",
            spots: (i % 7) as u32,
        })
        .collect()
}

fn runtime(threads: usize) -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(threads)
        .enable_all()
        .build()
        .unwrap()
}

pub fn serve(server: &str, threads: usize, port: u16) {
    let addr = format!("[::]:{port}");
    match server {
        "axum" => axum(threads, &addr, false),
        "axum-mw" => axum(threads, &addr, true),
        "axum-vec-json" => axum_vec_json(threads, &addr),
        "actix" => actix(threads, &addr),
        "salvo" => salvo(threads, &addr),
        "hyper" => hyper(threads, &addr),
        other => panic!("unknown http server {other}"),
    }
}

fn axum(threads: usize, addr: &str, middleware: bool) {
    use axum::http::{HeaderName, Request, StatusCode};
    use axum::{Json, Router, routing::get};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;
    use tower_http::request_id::{
        MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer,
    };

    #[derive(Clone, Default)]
    struct Sequential(Arc<AtomicU64>);
    impl MakeRequestId for Sequential {
        fn make_request_id<B>(&mut self, _: &Request<B>) -> Option<RequestId> {
            let n = self.0.fetch_add(1, Ordering::Relaxed);
            Some(RequestId::new(n.to_string().parse().unwrap()))
        }
    }

    let addr = addr.to_string();
    runtime(threads).block_on(async move {
        let mut app = Router::new()
            .route("/plain", get(|| async { "Hello, World!" }))
            .route("/json", get(|| async { Json(items()) }));
        if middleware {
            let header = HeaderName::from_static("x-request-id");
            app = app
                .layer(PropagateRequestIdLayer::new(header.clone()))
                .layer(tower_http::trace::TraceLayer::new_for_http())
                .layer(tower_http::timeout::TimeoutLayer::with_status_code(
                    StatusCode::REQUEST_TIMEOUT,
                    Duration::from_secs(10),
                ))
                .layer(SetRequestIdLayer::new(header, Sequential::default()));
        }
        let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });
}

/// Axum with a JSON response built by `serde_json::to_vec` instead of
/// `axum::Json`, which serializes through a `BytesMut` writer.
fn axum_vec_json(threads: usize, addr: &str) {
    use axum::http::header::CONTENT_TYPE;
    use axum::response::{IntoResponse, Response};
    use axum::{Router, routing::get};

    struct VecJson<T>(T);
    impl<T: Serialize> IntoResponse for VecJson<T> {
        fn into_response(self) -> Response {
            (
                [(CONTENT_TYPE, "application/json")],
                serde_json::to_vec(&self.0).unwrap(),
            )
                .into_response()
        }
    }

    let addr = addr.to_string();
    runtime(threads).block_on(async move {
        let app = Router::new()
            .route("/plain", get(|| async { "Hello, World!" }))
            .route("/json", get(|| async { VecJson(items()) }));
        let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });
}

fn actix(threads: usize, addr: &str) {
    use actix_web::{App, HttpResponse, HttpServer, web};
    let addr = addr.to_string();
    actix_web::rt::System::new().block_on(async move {
        HttpServer::new(|| {
            App::new()
                .route(
                    "/plain",
                    web::get().to(|| async {
                        HttpResponse::Ok()
                            .content_type("text/plain")
                            .body("Hello, World!")
                    }),
                )
                .route(
                    "/json",
                    web::get().to(|| async { HttpResponse::Ok().json(items()) }),
                )
        })
        .workers(threads)
        .bind(addr)
        .unwrap()
        .run()
        .await
        .unwrap();
    });
}

fn salvo(threads: usize, addr: &str) {
    use salvo::prelude::*;
    #[handler]
    async fn plain() -> &'static str {
        "Hello, World!"
    }
    #[handler]
    async fn json(res: &mut Response) {
        res.render(Json(items()));
    }
    let addr = addr.to_string();
    runtime(threads).block_on(async move {
        let router = Router::new()
            .push(Router::with_path("plain").get(plain))
            .push(Router::with_path("json").get(json));
        let acceptor = TcpListener::new(addr).bind().await;
        Server::new(acceptor).serve(router).await;
    });
}

fn hyper(threads: usize, addr: &str) {
    use bytes::Bytes;
    use http_body_util::Full;
    use hyper::{Request, Response, body::Incoming, server::conn::http1, service::service_fn};
    use hyper_util::rt::TokioIo;

    async fn handle(
        req: Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
        let r = Response::builder();
        Ok(match req.uri().path() {
            "/plain" => r
                .header("content-type", "text/plain")
                .body(Full::new(Bytes::from_static(b"Hello, World!"))),
            "/json" => r
                .header("content-type", "application/json")
                .body(Full::new(Bytes::from(
                    serde_json::to_vec(&items()).unwrap(),
                ))),
            _ => r.status(404).body(Full::new(Bytes::new())),
        }
        .unwrap())
    }

    let addr = addr.to_string();
    runtime(threads).block_on(async move {
        let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let _ = stream.set_nodelay(true);
            tokio::spawn(async move {
                let _ = http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service_fn(handle))
                    .await;
            });
        }
    });
}
