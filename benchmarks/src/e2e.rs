//! End-to-end suite: an Axum handler opens a tenant-scoped transaction, loads
//! 50 classes, and renders an Askama page. Only the driver varies.

use crate::app_url;
use askama::Template;
use axum::extract::{Query, State};
use axum::{Router, response::Html, routing::get};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio_postgres::types::ToSql;

const SET: &str = "SELECT set_config('app.tenant_id', $1, true)";
const LIST: &str = "SELECT name, starts_at, capacity, booked, coach, room FROM class_session WHERE starts_at > $1 ORDER BY starts_at LIMIT 50";

#[derive(sqlx::FromRow)]
struct Row {
    name: String,
    starts_at: DateTime<Utc>,
    capacity: i32,
    booked: i32,
    coach: String,
    room: String,
}

#[derive(Template)]
#[template(
    source = r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Schedule</title></head><body><h1>Tenant {{ tenant }}</h1><table><tbody>
{% for r in rows %}<tr><td>{{ r.name }}</td><td>{{ r.starts_at.format("%a %H:%M") }}</td><td>{{ r.coach }}</td><td>{{ r.room }}</td><td>{% if r.booked >= r.capacity %}Waitlist{% else %}{{ r.capacity - r.booked }}{% endif %}</td></tr>
{% endfor %}</tbody></table></body></html>"#,
    ext = "html"
)]
struct Page {
    tenant: i64,
    rows: Vec<Row>,
}

#[derive(serde::Deserialize)]
struct Params {
    t: i64,
    h: i64,
}

enum Db {
    Sqlx(sqlx::PgPool),
    TokioPg(deadpool_postgres::Pool, bool),
}

async fn schedule(State(db): State<Arc<Db>>, Query(q): Query<Params>) -> Html<String> {
    let since = Utc::now() + chrono::Duration::hours(q.h);
    let tenant = q.t.to_string();
    let rows = match &*db {
        Db::Sqlx(pool) => {
            let mut tx = pool.begin().await.unwrap();
            sqlx::query(SET)
                .bind(&tenant)
                .execute(&mut *tx)
                .await
                .unwrap();
            let rows: Vec<Row> = sqlx::query_as(LIST)
                .bind(since)
                .fetch_all(&mut *tx)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            rows
        }
        Db::TokioPg(pool, pipe) => {
            let p_set: [&(dyn ToSql + Sync); 1] = [&tenant];
            let p_list: [&(dyn ToSql + Sync); 1] = [&since];
            let mut c = pool.get().await.unwrap();
            let tx = c.transaction().await.unwrap();
            let set = tx.prepare_cached(SET).await.unwrap();
            let list = tx.prepare_cached(LIST).await.unwrap();
            let rows = if *pipe {
                let (a, b) = futures::join!(tx.execute(&set, &p_set), tx.query(&list, &p_list));
                a.unwrap();
                b.unwrap()
            } else {
                tx.execute(&set, &p_set).await.unwrap();
                tx.query(&list, &p_list).await.unwrap()
            };
            tx.commit().await.unwrap();
            rows.into_iter()
                .map(|r| Row {
                    name: r.get(0),
                    starts_at: r.get(1),
                    capacity: r.get(2),
                    booked: r.get(3),
                    coach: r.get(4),
                    room: r.get(5),
                })
                .collect()
        }
    };
    Html(Page { tenant: q.t, rows }.render().unwrap())
}

pub fn serve(variant: &str, threads: usize, port: u16) {
    let variant = variant.to_string();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(threads)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async move {
            let db = match variant.as_str() {
                "sqlx" => Db::Sqlx(
                    sqlx::postgres::PgPoolOptions::new()
                        .max_connections(16)
                        .min_connections(16)
                        .connect(&app_url())
                        .await
                        .unwrap(),
                ),
                "tokio-pg" | "tokio-pg-pipe" => {
                    let mut cfg = deadpool_postgres::Config::new();
                    cfg.url = Some(app_url());
                    cfg.pool = Some(deadpool_postgres::PoolConfig::new(16));
                    let pool = cfg
                        .create_pool(
                            Some(deadpool_postgres::Runtime::Tokio1),
                            tokio_postgres::NoTls,
                        )
                        .unwrap();
                    Db::TokioPg(pool, variant.ends_with("pipe"))
                }
                other => panic!("unknown e2e variant {other}"),
            };
            let app = Router::new()
                .route("/schedule", get(schedule))
                .with_state(Arc::new(db));
            let listener = tokio::net::TcpListener::bind(format!("[::]:{port}"))
                .await
                .unwrap();
            axum::serve(listener, app).await.unwrap();
        });
}
