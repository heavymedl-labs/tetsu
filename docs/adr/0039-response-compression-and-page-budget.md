# 0039. Response compression and a page budget

- Status: Accepted
- Date: 2026-11-11

## Context

Tetsu's demo root page grew to 272 KB of HTML, most of it inline SVG artwork, and `serve` sends it and the
72 KB stylesheet uncompressed. Compressed, the page is 17 to 25 KB. A fresh review of
[#97](https://github.com/heavymedl-labs/tetsu/issues/97) found the growth, and nothing in
[ADR-0005](0005-testing-and-performance.md) set a budget that would have caught it. The
[research](../research/compression.md) has the measurements and the evaluation.

## Decision

- **`serve` compresses its responses with `tower-http`'s `CompressionLayer`,** with the `compression-br` and
  `compression-gzip` features only. It sends brotli or gzip as the browser asks, and skips the WOFF2 font, which
  is already compressed.
- **The root page has a budget:** at most 40 KB of HTML, and the stylesheet at most 20 KB, each as sent to a
  browser that accepts brotli and gzip. A test requests both that way and fails above the budget. A page whose
  content grows past it needs a decision, not a quiet raise.
- **A response that will hold a secret, such as a sign-in token, turns compression off or masks the secret,**
  since compressing a secret beside text an attacker controls can leak it (BREACH). No page holds one today.

## Alternatives considered

- **Leave compression to the host or a CDN.** Nothing in the repository would guarantee it, and local and test
  runs could not check the budget.
- **Compress each asset and page once at start, with the codec crates directly.** It saves work per request, but
  repeats what `tower-http` does with the same codecs. It stays open once a hot path needs it.
- **Draw the artwork in separate files.** The page's CSS animates the artwork, and cannot style an SVG loaded as
  its own file, so keeping the motion would mean a rewrite.

## Consequences

Visitors download about a tenth of today's bytes, on any host. `serve` spends a little CPU on each response, and
brotli's quality stays moderate so that cost stays small. The weakest evidence is the brotli crate's reliance on
one maintainer; gzip alone would still serve every browser. Replacing the layer touches one line of `serve`. The
budget test turns page growth into a decision, and other pages gain budgets as they arrive. Revisit when a page
holds a secret, or when the page's compressed size nears the budget.

## Sources

- [Research: response compression](../research/compression.md), read 2026-11-11.
- `tower-http` documentation, `CompressionLayer`, <https://docs.rs/tower-http/0.7.1/tower_http/compression/>.
- Y. Gluck, N. Harris, A. Prado, "BREACH: Reviving the CRIME Attack", Black Hat USA 2013,
  <https://breachattack.com/>.
- RFC 7932, "Brotli Compressed Data Format", <https://www.rfc-editor.org/rfc/rfc7932>.
