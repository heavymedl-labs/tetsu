# Response compression

Research for the fresh review of [#97](https://github.com/heavymedl-labs/tetsu/issues/97), 2026-11-11: how Tetsu
sends its pages and assets compressed. It follows [ADR-0010](../adr/0010-evaluating-technology.md), steps 1, 4,
and 7 only, since one layer in `serve` is cheap to replace. Benchmarks are skipped: compressing about 270 KB takes
milliseconds, and no hot path exists yet.

## The problem

The demo root page is 271,764 bytes of HTML, sent uncompressed, beside a 72,635-byte stylesheet. Inline SVG
artwork is 183,968 bytes of the page, drawn cell by cell, so it repeats and compresses well:

| Encoding | Root page HTML |
| --- | --- |
| None | 271,764 |
| gzip -9 | 24,546 |
| brotli, default quality | 16,684 |

Measured 2026-11-11 against the local demo. Nothing set a budget, so nothing caught the growth.

## Criteria

Set before looking at options:

| ID | Criterion | Why |
| --- | --- | --- |
| C1 | Sends brotli or gzip, as the browser's `Accept-Encoding` asks, and nothing it did not ask for | Every current browser takes both; older clients take neither |
| C2 | Works on any host, and in local and test runs | Hosting may change ([ADR-0012](../adr/0012-first-hosting-and-deploys.md)), and a test must see it |
| C3 | Skips what is already compressed, such as the WOFF2 font | Compressing it again costs CPU and saves nothing |
| C4 | Licenses on ADR-0028's list, no open advisories, and maintained | [ADR-0028](../adr/0028-dependency-checks-revised.md) |
| C5 | Fits axum, which `serve` uses ([server stack](server-stack.md)) | One stack |

## Options

1. **`tower-http`'s `CompressionLayer`,** with its `compression-br` and `compression-gzip` features. It is the
   middleware axum's own documentation points to, so it is the common default.
2. **The host or a CDN in front.** It fails C2: nothing in the repository guarantees it, and local and test runs
   stay uncompressed.
3. **Homegrown:** compress each built asset and each page once at start, with `flate2` and `brotli` directly. It
   avoids per-request work, but repeats what option 1 does, and still needs both codec crates.

## Project health

Read 2026-11-11 from crates.io, GitHub, and the RustSec advisory database.

| Crate | Version | License | Recent downloads | Contributors, top share | Advisories |
| --- | --- | --- | --- | --- | --- |
| tower-http | 0.7.1 (2026-08-31) | MIT | 215 million | 100+, 25% | Two, both from 2021 in `ServeDir` on Windows, fixed in 0.2.1 |
| async-compression | 0.4.52 (2026-11-02) | MIT OR Apache-2.0 | 75 million | 56, 27% | None |
| brotli | 9.1.1 (2026-10-27) | BSD-3-Clause AND MIT | 91 million | 34, 86% | None |
| flate2 | 1.1.11 (2026-11-06) | MIT OR Apache-2.0 | 226 million | 100+, 39% | None |

`tower-http` is already in `Cargo.lock` at 0.7.1. The brotli crate rests mostly on one maintainer at Dropbox,
which is the weakest point; gzip alone would still meet C1.

## Weighing it

Option 1 meets every criterion, is the common default, and needs one layer and two features. Its cost is CPU on
each request. Brotli at high quality is slow, so the layer's quality must stay moderate, which the budget test
shows is enough.

One risk comes with sign-in ([#20](https://github.com/heavymedl-labs/tetsu/issues/20)): compressing a response
that holds a secret beside text an attacker controls can leak the secret (the BREACH attack). Today's pages hold no
secret. A page that does must turn compression off or mask the secret.
