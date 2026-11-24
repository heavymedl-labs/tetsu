# Template formatting and page minifying

An evaluation, under [ADR-0010](../adr/0010-evaluating-technology.md), of tools that format Tetsu's Askama
templates and minify the pages they render, read and tried on 2026-11-22. The maintainer asked for formatted
source with optimized delivery. This is research, not a decision; where it disagrees with an ADR, the ADR wins.
[ADR-0083](../adr/0083-formatting-templates-and-minifying-pages.md) records the decision.

## Criteria

Set before any trial. A formatter must:

- leave Askama's tags, which hold Rust, exactly as written
- change no page a browser shows
- give the same output when run again, and check without writing, for the pre-commit hook

A minifier must:

- change no page a browser shows, leaving `pre` and other whitespace-sensitive text alone
- pass cargo-deny's advisories and licenses, as every crate must (ADR-0028)
- cost little per page, since it runs on every page sent

## The templates

56 templates, 1,504 lines, as of 2026-11-22. They use ordinary Askama: inheritance, includes, and macros. What
stresses a tool: template tags inside attribute values 82 times, whitespace control (`{%-`, `-%}`) 126 times, Rust
in tags such as character literals, and 339 lines over 120 columns, mostly the home page's inline SVG art.

## Industry practice

Server-rendered frameworks compress pages and mostly do not minify HTML. Cloudflare retired Auto Minify in August
2024 and still compresses with gzip and brotli. One measured test found minifying saves 9 to 12% after brotli only
on deeply indented pages. Teams that format templates, such as Django teams with djLint, usually ship them
formatted and compressed.

## Formatters

| Tool | Health | Trial on Tetsu's templates |
| --- | --- | --- |
| djLint 1.54.0, Python, GPL-3.0 | About 960 stars, active, seeking maintainers | Rewrites quotes inside tags: `'t'` became `"t"` 12 times, breaking the build; single-quote mode broke 67 strings |
| markup_fmt 0.28.0, Rust, MIT | About 190 stars, 439k downloads, 14 crates | Leaves tags as written. Its default whitespace mode adds spaces inside text; strict mode renders every page identically |
| askama_fmt 0.4.1, Rust, MIT | 3 stars, one contributor, created May 2026 | Not tried: too young |
| prettier-plugin-jinja-template, Node, MIT | 160 stars | Not tried: reported to mangle tags |

markup_fmt is a library with no command of its own; dprint runs it as a WebAssembly plugin downloaded from
dprint's site. Calling the crate from a small Cargo tool keeps it in `Cargo.lock` and cargo-deny, and gives the
same output as the plugin.

Formatted, the templates grow from 1,504 lines by about 1,860, and pages grow too:

| Page | Brotli 4, today | Formatted | Formatted, then minified by html-minifier, against today |
| --- | --- | --- | --- |
| Root | 22,611 bytes | 25,536 bytes | 23,862 bytes (+5.5%) |
| Account | 3,675 bytes | 3,939 bytes | 3,701 bytes (+0.7%) |
| Login | 2,338 bytes | 2,507 bytes | 2,389 bytes (+2.2%) |

## Minifiers

| Tool | Result |
| --- | --- |
| minify-html 0.18.1 | Fails cargo-deny: lightningcss pulls in rkyv 0.7.46 (RUSTSEC-2026-0235), fixed only in 0.8. About 205 crates, with no feature to drop its CSS and JavaScript minifiers |
| min-html 0.18.4, a fork | The same advisory, and a license cargo-deny rejects |
| askama-minify 0.3.4 | Passes cargo-deny with default features off. Minifies nothing of Tetsu's: it leaves a whole template tree as written when any template in it puts a tag inside an attribute, and every page imports `components.html`. Writes a folder to the system temp folder on every build and never removes it |
| Askama's `whitespace = "minimize"` | Trims only around tags, not between plain HTML tags, and needs the config feature ADR-0028 turned off |
| lol_html 3.0.3 | Passes cargo-deny, but rewrites rather than minifies; the whitespace rules would be ours to write |
| html-minifier 6.0.1, MIT | Passes cargo-deny with 12 crates. 19 stars, one maintainer, 210k downloads, released 2026-11-08 |

html-minifier collapses each run of whitespace to one character, keeping a newline where the run had one, and
keeps attribute values. It leaves `pre`, `textarea`, and inline SVG as they are, and `code` once told to, so the
root page's art stays formatted. It minified an operator page in 33 to 56 µs and the root page in 0.9 ms, in a release build.

## Trial

Seven pages (root, login, join, account, mailbox, not found, refused) were rendered from today's templates and
again from formatted ones, minified, and drawn by headless Chromium at 1,280 and 390 pixels wide. Every one of the
14 pairs matched today's page to the pixel, and the mailbox's `pre` text was unchanged. Scripts did not run in the
operator pages drawn, since their test asset names do not exist.

Tests that matched exact HTML failed once the templates were formatted: 15 of them, all on whitespace, none on
content.

Served locally after adopting both, the root page was 424 KB before compression against 286 KB before formatting,
and its source still looked formatted: 76% of it is inline SVG, which html-minifier sends as written, and markup_fmt
had put each shape's attributes on lines of their own. Leaving the drawings in `art` and `home/panels`, since moved to `home/art`, unformatted
brought it to 286.4 KB, and 25.8 KB gzipped against 25.7 KB (2026-11-23).

## Weighing it

markup_fmt in strict mode meets every formatter criterion. Of the minifiers, only html-minifier meets every
criterion. Its weakest points are its size, one maintainer, and its gentle minifying, which leaves pages slightly
above today's. It sits in one function of the composition root, so it is easy to remove.
