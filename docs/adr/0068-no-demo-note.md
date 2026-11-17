# 0068. No demo note

- Status: Proposed
- Date: 2026-11-16
- Amends [ADR-0037](0037-fictional-customers-on-tetsus-page.md): the note its fictional content sat under

## Context

Every page in a demo environment opened with a note saying the site was a demo and what on it was fictional:
Tetsu's customers, quotes, figures, plans, and changelog on the root page, and the business on a tenant's page.
ADR-0037 showed the root page's fictional content under that note. The maintainer wants the demo's pages to look
as they will in use, with nothing above them.

## Decision

**No page carries a demo note.**

- The banner leaves the shared page shell, and Tetsu's root page and tenant pages lose their wording for it.
- Nothing else changes. The root page still shows its fictional content only where `TETSU_DEMO` is set, and every
  demo page still asks search engines not to index it.
- Tests check that no page shows the note, and that a demo page is still marked `noindex`.

## Alternatives considered

- **The note as one line in the footer.** The top of each page is clean and the content is still labelled, but
  the maintainer wants no note at all.
- **The note on the root page only.** A tenant page reached directly would carry none, so it labels the site
  unevenly for most of the same cost.
- **Keep the banner.** It is what ADR-0037 relied on, but it is not how the pages will look in use.

## Consequences

A visitor is no longer told that the demo's customers, quotes, figures, and businesses are fictional; `noindex`
keeps them out of search results. ADR-0037's rule stands otherwise: before Tetsu launches outside a demo, real
copy replaces the fictional content.

## Sources

- [Google Search Central: Block search indexing with noindex](https://developers.google.com/search/docs/crawling-indexing/block-indexing)
