//! Template suite: render a 50-row class schedule page with strings that need
//! escaping, in each engine and a homegrown baseline.

use crate::emit;
use askama::Template;
use sailfish::TemplateSimple;
use serde_json::json;
use std::fmt::Write;
use std::hint::black_box;
use std::time::Instant;

pub struct Row {
    name: String,
    starts: String,
    coach: String,
    room: String,
    spots: u32,
}

#[derive(Template)]
#[template(path = "schedule.html")]
struct AskamaPage<'a> {
    studio: &'a str,
    rows: &'a [Row],
}

#[derive(TemplateSimple)]
#[template(path = "../templates/schedule.stpl")]
struct SailfishPage<'a> {
    studio: &'a str,
    rows: &'a [Row],
}

fn maud_page(studio: &str, rows: &[Row]) -> String {
    maud::html! {
        (maud::DOCTYPE)
        html lang="en" {
            head { meta charset="utf-8"; title { (studio) " schedule" } }
            body {
                h1 { (studio) }
                table {
                    thead { tr { th { "Class" } th { "Starts" } th { "Coach" } th { "Room" } th { "Spots" } } }
                    tbody {
                        @for r in rows {
                            tr {
                                td { (r.name) } td { (r.starts) } td { (r.coach) } td { (r.room) }
                                td { @if r.spots == 0 { "Waitlist" } @else { (r.spots) } }
                            }
                        }
                    }
                }
            }
        }
    }
    .into_string()
}

fn escape(out: &mut String, s: &str) {
    let mut last = 0;
    for (i, b) in s.bytes().enumerate() {
        let rep = match b {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            b'"' => "&quot;",
            b'\'' => "&#39;",
            _ => continue,
        };
        out.push_str(&s[last..i]);
        out.push_str(rep);
        last = i + 1;
    }
    out.push_str(&s[last..]);
}

fn homegrown(studio: &str, rows: &[Row]) -> String {
    let mut o = String::with_capacity(8192);
    o.push_str("<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><title>");
    escape(&mut o, studio);
    o.push_str(" schedule</title></head>\n<body><h1>");
    escape(&mut o, studio);
    o.push_str("</h1>\n<table><thead><tr><th>Class</th><th>Starts</th><th>Coach</th><th>Room</th><th>Spots</th></tr></thead><tbody>\n");
    for r in rows {
        for (open, v) in [
            ("<tr><td>", &r.name),
            ("</td><td>", &r.starts),
            ("</td><td>", &r.coach),
            ("</td><td>", &r.room),
        ] {
            o.push_str(open);
            escape(&mut o, v);
        }
        o.push_str("</td><td>");
        if r.spots == 0 {
            o.push_str("Waitlist");
        } else {
            write!(o, "{}", r.spots).unwrap();
        }
        o.push_str("</td></tr>\n");
    }
    o.push_str("</tbody></table></body></html>");
    o
}

/// Best of five timed batches, in nanoseconds per render.
fn time(renders: usize, f: impl Fn() -> String) -> f64 {
    for _ in 0..renders / 10 {
        black_box(f());
    }
    (0..5)
        .map(|_| {
            let t = Instant::now();
            for _ in 0..renders {
                black_box(f());
            }
            t.elapsed().as_nanos() as f64 / renders as f64
        })
        .fold(f64::MAX, f64::min)
}

pub fn run(renders: usize) {
    let rows: Vec<Row> = (0..50)
        .map(|i| Row {
            name: format!("Strength & Conditioning <{i}>"),
            starts: format!("2026-11-{:02} 06:00", i % 28 + 1),
            coach: "Lee \"Coach\" Park".into(),
            room: format!("Room {}", i % 4),
            spots: (i % 7) as u32,
        })
        .collect();
    let studio = "Sample Studio";
    let engines: [(&str, Box<dyn Fn() -> String>); 4] = [
        (
            "askama",
            Box::new(|| {
                AskamaPage {
                    studio,
                    rows: &rows,
                }
                .render()
                .unwrap()
            }),
        ),
        (
            "sailfish",
            Box::new(|| {
                SailfishPage {
                    studio,
                    rows: &rows,
                }
                .render_once()
                .unwrap()
            }),
        ),
        ("maud", Box::new(|| maud_page(studio, &rows))),
        ("homegrown", Box::new(|| homegrown(studio, &rows))),
    ];
    for (name, f) in &engines {
        let page = f();
        assert!(!page.contains("<0>"), "{name} did not escape");
        let mut m = serde_json::Map::new();
        m.insert("ns_per_render".into(), json!(time(renders, f)));
        m.insert("bytes".into(), json!(page.len()));
        emit("tpl", "schedule-50", name, m);
    }
}
