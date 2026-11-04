//! Postgres driver suite. Each operation is one transaction that sets the
//! tenant for row-level security, then runs the workload's statements.

use crate::{app_url, cpu_us, emit};
use chrono::{DateTime, Utc};
use rand::Rng;
use serde_json::json;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio_postgres::types::ToSql;

const TENANTS: i64 = 200;
const SET: &str = "SELECT set_config('app.tenant_id', $1, true)";
const LOOKUP: &str = "SELECT id, tenant_id, name, starts_at, capacity, booked, coach, room FROM class_session WHERE id = $1";
const LIST: &str = "SELECT id, tenant_id, name, starts_at, capacity, booked, coach, room FROM class_session WHERE starts_at > $1 ORDER BY starts_at LIMIT 50";
const BOOK_UPDATE: &str = "UPDATE class_session SET booked = booked + 1 WHERE id = $1";
const BOOK_INSERT: &str = "INSERT INTO booking (tenant_id, session_id, customer_id) VALUES ($1, $2, $3)";

#[derive(Debug, sqlx::FromRow, diesel::Queryable)]
#[allow(dead_code)]
struct Session {
    id: i64,
    tenant_id: i64,
    name: String,
    starts_at: DateTime<Utc>,
    capacity: i32,
    booked: i32,
    coach: String,
    room: String,
}

impl From<tokio_postgres::Row> for Session {
    fn from(r: tokio_postgres::Row) -> Self {
        Session {
            id: r.get(0),
            tenant_id: r.get(1),
            name: r.get(2),
            starts_at: r.get(3),
            capacity: r.get(4),
            booked: r.get(5),
            coach: r.get(6),
            room: r.get(7),
        }
    }
}

diesel::table! {
    class_session (id) {
        id -> Int8, tenant_id -> Int8, name -> Text, starts_at -> Timestamptz,
        capacity -> Int4, booked -> Int4, coach -> Text, room -> Text,
    }
}
diesel::table! {
    booking (id) {
        id -> Int8, tenant_id -> Int8, session_id -> Int8, customer_id -> Int8,
        created_at -> Timestamptz,
    }
}

/// A random tenant and one of its class ids.
fn pick() -> (i64, i64) {
    let mut r = rand::rng();
    let tenant = r.random_range(1..=TENANTS);
    let id = r.random_range(0..1999i64) * TENANTS + (tenant - 1);
    (tenant, if id == 0 { TENANTS } else { id })
}

fn since() -> DateTime<Utc> {
    Utc::now() + chrono::Duration::hours(rand::rng().random_range(0..400))
}

/// Creates the `app` role, the `tetsu` database, and the seeded schema.
pub fn setup(admin_url: &str) {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let (c, conn) = tokio_postgres::connect(admin_url, tokio_postgres::NoTls).await.unwrap();
        tokio::spawn(conn);
        c.batch_execute("DROP DATABASE IF EXISTS tetsu").await.unwrap();
        c.batch_execute("DO $$ BEGIN CREATE ROLE app LOGIN; EXCEPTION WHEN duplicate_object THEN NULL; END $$")
            .await
            .unwrap();
        c.batch_execute("CREATE DATABASE tetsu").await.unwrap();
        let url = format!("{}/tetsu", admin_url.rsplit_once('/').unwrap().0);
        let (c, conn) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
        tokio::spawn(conn);
        c.batch_execute(include_str!("../sql/schema.sql")).await.unwrap();
        eprintln!("setup: seeded tetsu database");
    });
}

async fn drive<F, Fut>(ops: usize, tasks: usize, f: F) -> (Duration, Vec<Duration>)
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send,
{
    let f = Arc::new(f);
    let left = Arc::new(AtomicUsize::new(ops));
    let start = Instant::now();
    let handles: Vec<_> = (0..tasks)
        .map(|_| {
            let (f, left) = (f.clone(), left.clone());
            tokio::spawn(async move {
                let mut lat = Vec::new();
                while left.try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1)).is_ok() {
                    let t = Instant::now();
                    f().await;
                    lat.push(t.elapsed());
                }
                lat
            })
        })
        .collect();
    let mut all = Vec::with_capacity(ops);
    for h in handles {
        all.extend(h.await.unwrap());
    }
    (start.elapsed(), all)
}

fn deadpool(pool_size: usize) -> deadpool_postgres::Pool {
    let mut cfg = deadpool_postgres::Config::new();
    cfg.url = Some(app_url());
    cfg.pool = Some(deadpool_postgres::PoolConfig::new(pool_size));
    cfg.create_pool(Some(deadpool_postgres::Runtime::Tokio1), tokio_postgres::NoTls).unwrap()
}

async fn warm(pool: &deadpool_postgres::Pool, n: usize) {
    let mut held = Vec::new();
    for _ in 0..n {
        held.push(pool.get().await.unwrap());
    }
}

pub fn run(variant: &str, work: &str, ops: usize, tasks: usize, pool_size: usize) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (variant, work) = (variant.to_string(), work.to_string());
    let (wall, mut lat, cpu) = rt.block_on(async {
        let w = work.clone();
        let cpu0;
        let (wall, lat) = match variant.as_str() {
            "sqlx" => {
                let pool = sqlx::postgres::PgPoolOptions::new()
                    .max_connections(pool_size as u32)
                    .min_connections(pool_size as u32)
                    .connect(&app_url())
                    .await
                    .unwrap();
                cpu0 = cpu_us();
                drive(ops, tasks, move || sqlx_op(pool.clone(), w.clone())).await
            }
            "tokio-pg" | "tokio-pg-pipe" => {
                let pipe = variant.ends_with("pipe");
                let pool = deadpool(pool_size);
                warm(&pool, pool_size).await;
                cpu0 = cpu_us();
                drive(ops, tasks, move || tokio_pg_op(pool.clone(), w.clone(), pipe)).await
            }
            "cornucopia" | "cornucopia-prepared" | "cornucopia-prepared-pipe" => {
                let prepared = variant.contains("prepared");
                let pipe = variant.ends_with("pipe");
                let pool = deadpool(pool_size);
                warm(&pool, pool_size).await;
                cpu0 = cpu_us();
                drive(ops, tasks, move || cornucopia_op(pool.clone(), w.clone(), prepared, pipe)).await
            }
            "diesel" => {
                use diesel_async::AsyncPgConnection;
                use diesel_async::pooled_connection::AsyncDieselConnectionManager;
                use diesel_async::pooled_connection::deadpool::Pool;
                let mgr = AsyncDieselConnectionManager::<AsyncPgConnection>::new(app_url());
                let pool = Pool::builder(mgr).max_size(pool_size).build().unwrap();
                let mut held = Vec::new();
                for _ in 0..pool_size {
                    held.push(pool.get().await.unwrap());
                }
                drop(held);
                cpu0 = cpu_us();
                drive(ops, tasks, move || diesel_op(pool.clone(), w.clone())).await
            }
            other => panic!("unknown db variant {other}"),
        };
        (wall, lat, cpu_us() - cpu0)
    });
    lat.sort();
    let p = |q: f64| lat[((lat.len() as f64 * q) as usize).min(lat.len() - 1)].as_secs_f64() * 1e6;
    let mut m = serde_json::Map::new();
    m.insert("ops_per_s".into(), json!(lat.len() as f64 / wall.as_secs_f64()));
    m.insert("p50_us".into(), json!(p(0.50)));
    m.insert("p99_us".into(), json!(p(0.99)));
    m.insert("p999_us".into(), json!(p(0.999)));
    m.insert("cpu_us_per_op".into(), json!(cpu / lat.len() as f64));
    emit("db", &work, &variant, m);
}

async fn sqlx_op(pool: sqlx::PgPool, work: String) {
    let (t, id) = pick();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(SET).bind(t.to_string()).execute(&mut *tx).await.unwrap();
    match work.as_str() {
        "lookup" => {
            let s: Session = sqlx::query_as(LOOKUP).bind(id).fetch_one(&mut *tx).await.unwrap();
            assert_eq!(s.tenant_id, t);
        }
        "list" => {
            let v: Vec<Session> = sqlx::query_as(LIST).bind(since()).fetch_all(&mut *tx).await.unwrap();
            assert!(!v.is_empty());
        }
        _ => {
            sqlx::query(BOOK_UPDATE).bind(id).execute(&mut *tx).await.unwrap();
            sqlx::query(BOOK_INSERT).bind(t).bind(id).bind(42i64).execute(&mut *tx).await.unwrap();
        }
    }
    tx.commit().await.unwrap();
}

async fn tokio_pg_op(pool: deadpool_postgres::Pool, work: String, pipe: bool) {
    let (t, id) = pick();
    let (ts, s, customer) = (t.to_string(), since(), 42i64);
    let p_set: [&(dyn ToSql + Sync); 1] = [&ts];
    let p_id: [&(dyn ToSql + Sync); 1] = [&id];
    let p_since: [&(dyn ToSql + Sync); 1] = [&s];
    let p_insert: [&(dyn ToSql + Sync); 3] = [&t, &id, &customer];
    let mut c = pool.get().await.unwrap();
    let tx = c.transaction().await.unwrap();
    let set = tx.prepare_cached(SET).await.unwrap();
    match work.as_str() {
        "lookup" => {
            let q = tx.prepare_cached(LOOKUP).await.unwrap();
            let r = if pipe {
                let (a, b) = futures::join!(tx.execute(&set, &p_set), tx.query_one(&q, &p_id));
                a.unwrap();
                b.unwrap()
            } else {
                tx.execute(&set, &p_set).await.unwrap();
                tx.query_one(&q, &p_id).await.unwrap()
            };
            assert_eq!(Session::from(r).tenant_id, t);
        }
        "list" => {
            let q = tx.prepare_cached(LIST).await.unwrap();
            let rows = if pipe {
                let (a, b) = futures::join!(tx.execute(&set, &p_set), tx.query(&q, &p_since));
                a.unwrap();
                b.unwrap()
            } else {
                tx.execute(&set, &p_set).await.unwrap();
                tx.query(&q, &p_since).await.unwrap()
            };
            let v: Vec<Session> = rows.into_iter().map(Session::from).collect();
            assert!(!v.is_empty());
        }
        _ => {
            let u = tx.prepare_cached(BOOK_UPDATE).await.unwrap();
            let i = tx.prepare_cached(BOOK_INSERT).await.unwrap();
            if pipe {
                let (a, b, c) = futures::join!(
                    tx.execute(&set, &p_set),
                    tx.execute(&u, &p_id),
                    tx.execute(&i, &p_insert)
                );
                a.unwrap();
                b.unwrap();
                c.unwrap();
            } else {
                tx.execute(&set, &p_set).await.unwrap();
                tx.execute(&u, &p_id).await.unwrap();
                tx.execute(&i, &p_insert).await.unwrap();
            }
        }
    }
    tx.commit().await.unwrap();
}

async fn cornucopia_op(pool: deadpool_postgres::Pool, work: String, prepared: bool, pipe: bool) {
    use cornucopia::queries::sessions as q;
    let (t, id) = pick();
    let (ts, s) = (t.to_string(), since().fixed_offset());
    let mut c = pool.get().await.unwrap();
    let tx = c.transaction().await.unwrap();
    let (mut set, mut look, mut list) = (q::set_tenant(), q::lookup(), q::list());
    let (update, insert) = (q::book_update(), q::book_insert());
    if prepared {
        set = set.prepare(&tx).await.unwrap();
        look = look.prepare(&tx).await.unwrap();
        list = list.prepare(&tx).await.unwrap();
    }
    match work.as_str() {
        "lookup" => {
            let r = if pipe {
                let (a, b) = futures::join!(set.bind(&tx, &ts).one(), look.bind(&tx, &id).one());
                a.unwrap();
                b.unwrap()
            } else {
                set.bind(&tx, &ts).one().await.unwrap();
                look.bind(&tx, &id).one().await.unwrap()
            };
            assert_eq!(r.tenant_id, t);
        }
        "list" => {
            let v = if pipe {
                let (a, b) = futures::join!(set.bind(&tx, &ts).one(), list.bind(&tx, &s).all());
                a.unwrap();
                b.unwrap()
            } else {
                set.bind(&tx, &ts).one().await.unwrap();
                list.bind(&tx, &s).all().await.unwrap()
            };
            assert!(!v.is_empty());
        }
        _ => {
            if pipe {
                let (a, b, c) = futures::join!(
                    set.bind(&tx, &ts).one(),
                    update.bind(&tx, &id),
                    insert.bind(&tx, &t, &id, &42i64)
                );
                a.unwrap();
                b.unwrap();
                c.unwrap();
            } else {
                set.bind(&tx, &ts).one().await.unwrap();
                update.bind(&tx, &id).await.unwrap();
                insert.bind(&tx, &t, &id, &42i64).await.unwrap();
            }
        }
    }
    tx.commit().await.unwrap();
}

async fn diesel_op(pool: diesel_async::pooled_connection::deadpool::Pool<diesel_async::AsyncPgConnection>, work: String) {
    use diesel::prelude::*;
    use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
    let (t, id) = pick();
    let mut pooled = pool.get().await.unwrap();
    let c: &mut AsyncPgConnection = &mut pooled;
    c.transaction::<_, diesel::result::Error, _>(async move |c: &mut AsyncPgConnection| {
        diesel::sql_query(SET).bind::<diesel::sql_types::Text, _>(t.to_string()).execute(c).await?;
        match work.as_str() {
            "lookup" => {
                let s: Session = class_session::table.find(id).first(c).await?;
                assert_eq!(s.tenant_id, t);
            }
            "list" => {
                let v: Vec<Session> = class_session::table
                    .filter(class_session::starts_at.gt(since()))
                    .order(class_session::starts_at)
                    .limit(50)
                    .load(c)
                    .await?;
                assert!(!v.is_empty());
            }
            _ => {
                diesel::update(class_session::table.find(id))
                    .set(class_session::booked.eq(class_session::booked + 1))
                    .execute(c)
                    .await?;
                diesel::insert_into(booking::table)
                    .values((booking::tenant_id.eq(t), booking::session_id.eq(id), booking::customer_id.eq(42i64)))
                    .execute(c)
                    .await?;
            }
        }
        Ok(())
    })
    .await
    .unwrap();
}
