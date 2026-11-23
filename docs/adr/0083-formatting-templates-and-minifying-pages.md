# 0083. Formatting templates and minifying pages

- Status: Accepted
- Date: 2026-11-22

## Context

Every language in Tetsu has a formatter except its Askama templates, which are written by hand, many on long
packed lines. They are about to grow, as the account area ([#156](https://github.com/heavymedl-labs/tetsu/issues/156))
and the other realms add pages. The maintainer asked for formatted source with optimized delivery. Formatting adds
indentation every page carries: 13% on the root page and 7% on operator pages after brotli. The
[research](../research/template-formatting.md) has the trials of four formatters and six ways to minify.

## Decision

- **markup_fmt formats every HTML template,** as Jinja, in strict whitespace mode, 120 columns wide, indented by
  two spaces. Strict mode never adds or removes whitespace a browser would show, and markup_fmt leaves a tag's
  Rust as written. A generated template, whose first line says so, stays as its generator writes it.
- **A Cargo tool runs it:** `cargo run -p tetsu-format-templates` formats in place, and with `--check` fails on a
  template that is not formatted. It lives in `tools/format-templates`, outside the default members like the
  benchmarks, so markup_fmt never reaches the binary. `scripts/check.sh` runs the check, its lint, and its tests,
  so the pre-commit hook catches an unformatted template.
- **The composition root minifies every HTML response with html-minifier,** in one layer inside compression,
  whichever module rendered the page. It collapses each run of whitespace to one character and leaves `pre`,
  `code`, `textarea`, and inline SVG alone, and keeps attribute values. A page it cannot finish, one ending inside a tag, goes as rendered.
  Other responses are untouched.
- **Tests read pages as a browser does:** whitespace runs as one space and none before a tag's `>`, so a
  template's formatting never breaks a test. Tests of pages the server renders check them minified, as sent.

## Alternatives considered

- **Ship formatted pages unminified.** No new crate in the binary, but every page pays for the indentation.
- **minify-html.** The most thorough minifier, but its CSS minifier brings a crate with an advisory cargo-deny
  rejects, and about 205 crates with it.
- **askama-minify, at compile time.** No cost per request, but it minifies none of Tetsu's pages, since any tag
  inside an attribute stops it for the whole template tree, and it leaves files in the system temp folder.
- **dprint with markup_fmt's plugin.** The same formatter, but downloaded as WebAssembly outside Cargo and
  cargo-deny, with Node to run it.
- **djLint.** Mature, but it rewrites the Rust inside tags.
- **Leave the templates unformatted.** Nothing to adopt, but templates keep drifting as they grow.

## Consequences

Templates read consistently, and a formatting change is one command. Pages ship within about 2% of their old
size after brotli, and 5.5% on the root page, whose SVG art stays formatted; the page budget
([ADR-0039](0039-response-compression-and-page-budget.md)) still holds. Minifying costs 33 to 56 µs per operator
page and about 1 ms on the root page. Pages that do not vary by visitor could be rendered and minified once
([#163](https://github.com/heavymedl-labs/tetsu/issues/163)).

The weakest evidence is html-minifier's small following: one maintainer and 19 stars. It is one function in the
composition root, so replacing it, or dropping minifying, touches nothing else. Revisit if it stops being
maintained, or when minify-html passes cargo-deny. The first formatting rewrote 51 templates in one commit, which
`.git-blame-ignore-revs` lists, so `git blame` and GitHub skip it.

## Sources

- [Research: template formatting and page minifying](../research/template-formatting.md), read 2026-11-22.
- [markup_fmt](https://docs.rs/markup_fmt/0.28.0/markup_fmt/), `format_text` and its options.
- [html-minifier](https://github.com/magiclen/html-minifier), read at 6.0.1.
- [Cloudflare: Auto Minify](https://developers.cloudflare.com/speed/optimization/content/troubleshooting/disable-auto-minify/),
  deprecated 2024-08-05.
- [RustSec RUSTSEC-2026-0235](https://rustsec.org/advisories/RUSTSEC-2026-0235.html), in rkyv before 0.8.17.
