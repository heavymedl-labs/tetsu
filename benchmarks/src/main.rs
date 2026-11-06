//! Stack benchmarks for ADR-0007. Each subcommand runs one option and prints
//! JSON records, one per line, that `aggregate.py` combines across runs.
//! `run.sh` drives the suites; see README.md.

mod db;
mod e2e;
mod http;
#[cfg(target_os = "linux")]
mod rt;
mod tpl;

use serde_json::{Map, Value, json};

/// Connection string for the `app` role, which row-level security applies to.
pub fn app_url() -> String {
    std::env::var("BENCH_DB_URL").unwrap_or_else(|_| "postgres://app@localhost:5432/tetsu".into())
}

/// Prints one result record.
pub fn emit(suite: &str, case: &str, variant: &str, metrics: Map<String, Value>) {
    println!(
        "{}",
        json!({ "suite": suite, "case": case, "variant": variant, "metrics": metrics })
    );
}

/// Client CPU time used by this process so far, in microseconds.
pub fn cpu_us() -> f64 {
    let mut u: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut u) };
    let t = |v: libc::timeval| v.tv_sec as f64 * 1e6 + v.tv_usec as f64;
    t(u.ru_utime) + t(u.ru_stime)
}

fn usage() -> ! {
    eprintln!(
        "usage:
  tetsu-bench setup <admin-url>
  tetsu-bench db <variant> <lookup|list|book> <ops> <tasks> <pool>
  tetsu-bench tpl <renders>
  tetsu-bench http <axum|axum-mw|axum-vec-json|actix|salvo|hyper> <threads> <port>
  tetsu-bench rt <tokio-ct|tokio-mt|monoio|compio> <port>
  tetsu-bench e2e <sqlx|tokio-pg|tokio-pg-pipe> <threads> <port>"
    );
    std::process::exit(2)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let arg = |i: usize| a.get(i).cloned().unwrap_or_else(|| usage());
    let num = |i: usize| arg(i).parse::<usize>().unwrap_or_else(|_| usage());
    match a.get(1).map(String::as_str) {
        Some("setup") => db::setup(&arg(2)),
        Some("db") => db::run(&arg(2), &arg(3), num(4), num(5), num(6)),
        Some("tpl") => tpl::run(num(2)),
        Some("http") => http::serve(&arg(2), num(3), num(4) as u16),
        #[cfg(target_os = "linux")]
        Some("rt") => rt::serve(&arg(2), num(3) as u16),
        Some("e2e") => e2e::serve(&arg(2), num(3), num(4) as u16),
        _ => usage(),
    }
}
