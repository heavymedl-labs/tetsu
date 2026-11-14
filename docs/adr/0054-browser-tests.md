# 0054. Browser tests

- Status: Proposed
- Date: 2026-11-13

## Context

[ADR-0005](0005-testing-and-performance.md) asks for end-to-end tests of critical UI journeys, and none exist. The
checks read rendered HTML and CSS, but nothing loads a page in a browser. On 2026-11-13 four bugs reached the demo
that only a browser shows: tabbing into the header jolted the page, tabs trapped the arrow keys, stories played out
of sight, and an animated blur clipped a border on iPhones. [#37](https://github.com/heavymedl-labs/tetsu/issues/37)
had deferred a browser tool until the first client-side code, but these bugs came from CSS alone.

Criteria, before looking at options: real keyboard input, focus, and scrolling in Chromium, Firefox, and WebKit; runs
on Linux in `scripts/check.sh` and CI against the Rust binary; free for a private repository; works under a CSP with
no `script-src`; deterministic enough to block a pull request; and the fewest new tools. Evidence is in the
[research](../research/browser-and-system-testing.md).

## Decision

- **Use [Playwright](https://playwright.dev)** (`@playwright/test`, Apache-2.0) as the one browser runner, in `web/`,
  pinned to an exact version, with **[axe-core](https://github.com/dequelabs/axe-core)** through
  `@axe-core/playwright` (MPL-2.0) for accessibility.
- **Every check run,** in Chromium:
  - keyboard and state specs for each page's controls (header, menus, popovers), under reduced motion;
  - axe on each page, with each `details` and popover open, in light, dark, and forced colors;
  - a shared fixture that fails a test on a console error, a page error, a failed request, or a CSP violation;
  - no sideways scrolling at 320 CSS px;
  - a size budget on Vite's output, compressed as served.
- **Later, each with its own ADR when started:** Firefox and WebKit runs and screenshot baselines in Playwright's
  pinned Docker image, run nightly or on demand; the Nu HTML Checker over each page; Lighthouse CI as a median of
  five, on metrics, never scores; and an iOS Simulator job on a macOS runner.
- **Apple hardware for compositing.** Linux WebKit cannot stand in for iPhone Safari when CSS animates or composites
  a layer. A change that touches `filter`, `backdrop-filter`, transform or opacity animation, or a scroll-driven
  animation is checked on a real iPhone before it ships, and the review checklist says so.
- **Before a release,** by hand: a ten-minute iPhone pass (scroll during animation, the toolbar, landscape, popovers
  and `details`, Reduce Motion, Dark Mode) and a screen reader pass for new interactive patterns.
- [ADR-0053](0053-supported-browsers.md) sets which browsers count.

## Alternatives considered

- **Cypress.** Mature, but its WebKit is experimental and it has no Safari route; its native key presses covered only
  Tab until version 15.1.
- **WebdriverIO or Selenium.** Real Safari only on macOS, which the maintainer does not have, and more setup for the
  same Chromium and Firefox coverage.
- **A Rust driver such as thirtyfour.** Keeps tests in Rust, but brings no runner, waiting, traces, screenshots, or
  axe, and Node is already in the build ([ADR-0034](0034-front-end-build.md)).
- **HTTP tests only,** as now. Fast, but blind to focus, scroll, CSS state, and painting, where all four bugs were.
- **A real-device cloud.** The only automated route to iPhone Safari, but it starts near $175 a month.

## Consequences

- Playwright 1.65.1 was released on 2026-11-13, with minor releases monthly, 55 commit authors in 90 days, and
  129.6M npm downloads a week; axe-core 4.14.0 on 2026-10-05. Microsoft and Deque back them. Read 2026-11-13.
- The work is [#129](https://github.com/heavymedl-labs/tetsu/issues/129).
- Check runs need the browsers, which `scripts/install-tools.sh` must then install, and grow by the suite's time;
  Chromium alone keeps it short.
- The CSP fixture and axe may need separate browser contexts, since axe injects a script. To confirm when building.
- Automated accessibility finds about half of problems, so the manual passes stay.
- Overlaps [#108](https://github.com/heavymedl-labs/tetsu/issues/108), which sets up TypeScript tests in `web/`;
  whichever lands first sets the runner the other uses.
- Replacing Playwright means rewriting the specs; the fixture and page list carry over.
- Revisit if Playwright's WebKit stops tracking WebKit, or the repository goes public, which makes macOS runners free.

## Sources

- Playwright, [best practices](https://playwright.dev/docs/best-practices),
  [accessibility testing](https://playwright.dev/docs/accessibility-testing),
  [browsers](https://playwright.dev/docs/browsers), and [emulation](https://playwright.dev/docs/emulation), read
  2026-11-13.
- Deque, [automated testing study](https://www.deque.com/blog/automated-testing-study-identifies-57-percent-of-digital-accessibility-issues),
  2021, and GDS, [tools tested on the least accessible page](https://accessibility.blog.gov.uk/2017/02/24/what-we-found-when-we-tested-tools-on-the-worlds-least-accessible-webpage/),
  2017.
- WebKit, [bug 312584](https://bugs.webkit.org/show_bug.cgi?id=312584): a blurred child ignoring its parent's
  rounded clip in Safari 26.4.
