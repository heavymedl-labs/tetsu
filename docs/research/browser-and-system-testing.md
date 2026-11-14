# Browser and system testing

Research for [ADR-0053](../adr/0053-supported-browsers.md), [ADR-0054](../adr/0054-browser-tests.md), and
[#37](https://github.com/heavymedl-labs/tetsu/issues/37), 2026-11-13: how to catch, before they ship, the kind of
UI bugs fixed that day (a header that jolted on Tab, tabs that trapped the arrow keys, stories that played out of
sight, and a border clipped only on iPhones), and, as a separate track, how to test the server under load and
faults. It follows [ADR-0010](../adr/0010-evaluating-technology.md) steps 1, 2, 4, 5, and 7. Benchmarks are
skipped: none of these tools sits on a request path. Facts were read on 2026-11-13 unless dated otherwise.

## Criteria

Set before looking at options:

| ID | Criterion | Why |
| --- | --- | --- |
| C1 | Drives real keyboard input, focus, and scrolling in Chromium, Firefox, and WebKit | Today's bugs were all keyboard, scroll, or engine bugs |
| C2 | Runs on Linux, in `scripts/check.sh` and CI, against the Rust binary | The maintainer has no Mac; [ADR-0023](../adr/0023-ci-on-demand.md) |
| C3 | Free for a private repository | Free tiers until [ADR-0012](../adr/0012-first-hosting-and-deploys.md)'s conditions are met |
| C4 | Works under a CSP with no `script-src` | [ADR-0031](../adr/0031-front-end-model-revised.md) |
| C5 | Deterministic enough to block a pull request | Flaky gates get ignored |
| C6 | Fewest new tools; Node is already in the build | [ADR-0034](../adr/0034-front-end-build.md) |

## Test layers

Teams that keep browser suites healthy sort checks by cost and determinism, not by tool. No single engineering
post lays out this split, so it is a synthesis of Playwright's best practices, Lighthouse's warnings about shared CI,
and GOV.UK's release gates.

| Layer | What it catches | When |
| --- | --- | --- |
| Keyboard and CSS-state specs, under reduced motion | Focus order and visibility, scroll, `details` and popover state | Every pull request |
| axe-core on each page, with each `details` and popover open, in light, dark, and forced colors | About half of accessibility faults | Every pull request |
| Console, page error, failed request, and `securitypolicyviolation` fixture | Script or CSP regressions, missing hashed assets | Every pull request |
| Overflow check at 320 CSS px (WCAG 1.4.10) | Sideways scrolling on small screens | Every pull request |
| Brotli byte budget on Vite's output and each page | CSS and font bloat, the main risk without script | Every pull request |
| Nu HTML Checker (vnu) over each page's HTML | Invalid nesting and attributes that axe skips, from template branches | Every pull request |
| Firefox and WebKit runs, screenshot baselines, Lighthouse CI | Engine differences, visual drift, LCP and CLS | Nightly or on demand |
| Real iPhone or iOS Simulator | Compositing bugs: filters, blur, animated layers | CSS changes that touch them, and releases |
| Screen reader passes (VoiceOver, NVDA) and Windows contrast themes | How controls are announced; what axe cannot judge | Before a release |
| Smoke test after deploy | A deploy that does not render | Each deploy |

What the evidence says about each:

- **axe finds about half.** Deque reports 57% of issues by count over 13,000 pages (vendor data, 2021). GDS's 2017
  test page found the best free tool caught 37 to 41% of 143 planted barriers, all ten tools 71%, and 29% escaped
  every tool. axe reports uncertain results as incomplete rather than failing them, so it is safe to block on.
- **Lighthouse is noisy on shared runners.** Its own guide calls free CI "volatile", wants two dedicated cores, and
  finds the median of five runs twice as stable as one. Assert on LCP, CLS, and byte weights, never on scores, and
  never upload reports for a private repository to temporary public storage.
- **Real-user monitoring needs script.** The web-vitals library is blocked by the CSP, and Chrome's field data needs
  traffic the demo lacks. Server TTFB in Grafana is the real-user signal until the first island.
- **The display matrix is cheap.** Playwright emulates color scheme, reduced motion, forced colors, and contrast per
  page. Real Windows contrast themes still differ from the emulation.
- **CSP checks and axe may clash.** axe injects a script, which may need `bypassCSP`, and that would hide CSP
  violations. Running them in separate browser contexts is the likely answer, untested.

## Browser test tools

| Tool | Latest | Authors, 90 days | npm a week | WebKit | License |
| --- | --- | --- | --- | --- | --- |
| Playwright | 1.65.1, 2026-11-13, minors monthly | 55 | 129.6M | First class, on Linux | Apache-2.0 |
| Cypress | 16.3.0, 2026-11-10 | 24 | 7.1M | Experimental, no Safari | MIT |
| WebdriverIO | 10.2.0, 2026-11-06 | 54 | 3.3M | Real Safari on macOS only | MIT |
| Selenium | 4.52.0, 2026-10-29 | 33 | 2.1M | Real Safari on macOS only | Apache-2.0 |

Playwright is backed by Microsoft and is the only one that meets C1 and C2 together. Its 30 open issues against
Cypress's 1,150 reflect strict triage more than fewer bugs. Version 1.65 needs Node 22 or later, and Tetsu pins Node 26.
Cypress's native key presses covered only Tab until 15.1. Rust drivers (thirtyfour 0.38.2, active; fantoccini and
chromiumoxide, quiet since February) have no runner, waiting, traces, screenshots, or axe, and Node is already here.

Screenshot comparison: Playwright's built-in `toHaveScreenshot` with baselines in the repository is enough for one
maintainer, compared only inside its pinned Docker image, since fonts differ even between Linux machines. It freezes
CSS animations but its docs do not mention scroll-driven ones, so freeze those with a capture stylesheet or reduced
motion. Argos's free plan (5,000 screenshots a month, private repositories allowed) is the hosted fallback. Lost Pixel
is shutting down (2026-04-22) and BackstopJS has had no release since 2024.

Others: axe-core 4.14.0 (2026-10-05, MPL-2.0) through `@axe-core/playwright`. vnu (MIT) publishes only a rolling
`latest` build (2026-11-09), so pin its image by digest; whether it knows `details[name]` was not checked. Lighthouse
CI 0.16.1 (2026-11-11, Apache-2.0).

## WebKit on Linux is not iPhone Safari

Playwright's WebKit is built from WebKit's main branch, not from a Safari release, and Playwright says a Mac comes
closest to Safari. Its iPhone profiles set only the viewport, user agent, and touch. Layout, `@supports`, popover,
and `details` agree across platforms. Painting does not: Linux draws and composites with Skia, Apple with
CoreAnimation. So filters, blur, animated layers, and their clipping are unverified until seen on Apple hardware.
The dynamic toolbar, safe areas, rubber-band scrolling, sticky hover after a tap, and iOS fonts are not emulated.

[WebKit bug 312584](https://bugs.webkit.org/show_bug.cgi?id=312584) matches the day's iPhone bug: Safari 26.4
shipped a blurred child that ignored its parent's rounded clip, fixed on main on 2026-05-14. Playwright, built from
main, never had it. Version skew cuts both ways: Linux WebKit can pass features Safari has not shipped.

Cheap ways to see real iOS rendering:

1. **The maintainer's iPhone over USB,** inspected from Linux with `ios-webkit-debug-proxy` and
   `ios-safari-remote-debug-kit`, free. Support for iOS 26 and 27 was not confirmed.
2. **The iOS Simulator on a GitHub macOS runner,** by hand: $0.062 a minute, from the $12 of compute a private
   repository gets free each month, about 190 minutes if Linux uses none. A public repository runs it free.
3. **One-off device time:** AWS Device Farm's trial (250 minutes), or BrowserStack Freelancer ($12.50 a month,
   100 live minutes). Automated real-iOS clouds start near $175 a month.

## Supported browsers

Baseline "Widely available" means a feature has worked in every core browser for 30 months. Browserslist accepts
`baseline widely available on <date>`, and Lightning CSS reads browserslist. Vite 8 does not: its default `build.target`
is Baseline Widely available frozen at 2026-01-01 (Chrome 111, Firefox 114, Safari 16.4), so one statement needs a
small bridge in `vite.config.ts`. Whether `@tailwindcss/vite` passes its own targets was not checked. Tailwind 4 needs
Chrome 111, Safari 16.4, and Firefox 128, which binds tighter than Vite's Firefox 114.

| Feature Tetsu uses | Baseline | Oldest Safari or iOS | Widely available |
| --- | --- | --- | --- |
| `:has()` | Widely | 15.4 | since 2026-06-19 |
| `@property` | Newly | 16.4 | 2027-01-09 |
| `details name` | Newly | 17.2 | 2027-03-03 |
| `popover` | Newly | 17, iOS 18.3 | 2027-07-27 |
| Container style queries | Newly, 2026-05-19 | 18 | 2028 |
| Scroll-driven animations | Newly, 2026-10-27 | 26 | 2029 |

Traffic, from caniuse and StatCounter for October 2026: iOS Safari below 16.4 is about 0.3% of global traffic, and
below 17.2 about 0.6%. iOS 18.7, where the iPhone XS and XR stop, is 10.5% of US iOS traffic and lacks scroll-driven
animations, as does Firefox 153 ESR. So scroll-driven animations must stay an enhancement, never the floor.

Published policies: GOV.UK lists browsers covering about 98% of its traffic and reviews yearly; GitHub supports the
latest of each; Stripe's dashboard the last 20 Chrome and Firefox majors and last 4 of Safari; HEY names minimums
(Safari 17.2, Chrome 120, Firefox 125). None says which versions its CI tests. Playwright runs only current engines,
so a floor is enforced statically: build targets, `@supports` guards, and a reduced-motion run showing pages work
without the enhancements.

## System testing

A separate track: how the server behaves under load and when what it depends on fails.

- **Never against the free hosts.** Render's free tier suspends every free service for the month after 750 instance
  hours, and may suspend one for unusual outbound traffic. Neon's free tier suspends compute after 100 CU-hours or
  5 GB of transfer a month. Neither permits load testing in its documents. Run load locally against a release
  build and a Postgres capped like Neon's 0.25 CU (105 usable connections); send the demo only smoke requests.
- **Load tools.** k6 2.4.0 (2026-11-12, AGPL-3.0, very active) has open-model executors that avoid coordinated
  omission, and thresholds that fail a run; 2.0 in May 2026 was a breaking major. oha 1.16.0 (MIT, Rust) loads one
  URL with latency correction. Goose 0.19.1 (Apache-2.0) scripts scenarios in Rust, with its mitigation off by
  default. Gatling and Locust are healthy; wrk and wrk2 are dormant. Tail latency on shared runners is trend data,
  not a gate.
- **Faults first.** Toxiproxy 2.13.0 (2026-10-14, MIT, Shopify) sits between the app and Postgres and adds latency,
  timeouts, resets, and outages through an HTTP API. Its Rust client last released in 2021, so call the API from a
  test against a generic container. `fail-parallel` 0.6.0 (2026-07) gives in-process fail points for parallel tests;
  `fail` has not released since 2022. Chaos Mesh and LitmusChaos need Kubernetes. Worth testing: Postgres killed
  mid-transaction, latency past the pool's acquire timeout, reset connections, SIGTERM during requests, and a
  Postgres restart like Neon waking from zero.

## Weakest evidence

- The per-layer schedule above is a synthesis, not a published practice.
- Whether axe and the CSP fixture can share a browser context, whether vnu accepts `details[name]`, whether
  `@tailwindcss/vite` honors Vite's CSS target, and whether the iOS inspection kit works on iOS 26 were not tested.
- Accessibility catch rates come from vendor data (2021) and a 2017 audit.
- Prices and free tiers change; each was read on 2026-11-13.

## Sources

- Playwright: [best practices](https://playwright.dev/docs/best-practices),
  [accessibility testing](https://playwright.dev/docs/accessibility-testing),
  [emulation](https://playwright.dev/docs/emulation), [browsers](https://playwright.dev/docs/browsers),
  [visual comparisons](https://playwright.dev/docs/test-snapshots), [Docker](https://playwright.dev/docs/docker),
  [releases](https://github.com/microsoft/playwright/releases).
- Others: [Cypress browsers](https://docs.cypress.io/app/references/launching-browsers),
  [WebdriverIO](https://github.com/webdriverio/webdriverio/releases),
  [Selenium](https://github.com/SeleniumHQ/selenium/releases), [thirtyfour](https://github.com/stevepryde/thirtyfour),
  [axe-core](https://github.com/dequelabs/axe-core), [vnu](https://validator.github.io/validator/docs/vnu.1.html),
  [Lighthouse CI](https://github.com/GoogleChrome/lighthouse-ci/blob/main/docs/getting-started.md),
  [Lighthouse variability](https://github.com/GoogleChrome/lighthouse/blob/main/docs/variability.md),
  [Argos pricing](https://argos-ci.com/pricing),
  [Lost Pixel sunset](https://www.lost-pixel.com/blog/lost-pixel-team-is-joining-figma).
- Accessibility: [Deque, 2021](https://www.deque.com/blog/automated-testing-study-identifies-57-percent-of-digital-accessibility-issues),
  [GDS, 2017](https://accessibility.blog.gov.uk/2017/02/24/what-we-found-when-we-tested-tools-on-the-worlds-least-accessible-webpage/),
  [GOV.UK Service Manual](https://www.gov.uk/service-manual/technology/testing-for-accessibility),
  [W3C Reflow](https://www.w3.org/WAI/WCAG22/Understanding/reflow).
- iOS: [WebKit bug 312584](https://bugs.webkit.org/show_bug.cgi?id=312584),
  [WebKitGTK 2.46](https://webkitgtk.org/2024/10/04/webkitgtk-2.46.html),
  [GitHub Actions runner pricing](https://docs.github.com/en/billing/reference/actions-runner-pricing),
  [ios-safari-remote-debug-kit](https://github.com/HimbeersaftLP/ios-safari-remote-debug-kit),
  [ios-webkit-debug-proxy](https://github.com/google/ios-webkit-debug-proxy),
  [AWS Device Farm pricing](https://aws.amazon.com/device-farm/pricing/),
  [BrowserStack pricing](https://www.browserstack.com/pricing).
- Browsers: [Baseline](https://web.dev/baseline), [browserslist](https://github.com/browserslist/browserslist),
  [Vite build options](https://vite.dev/config/build-options),
  [Lightning CSS](https://lightningcss.dev/transpilation.html),
  [Tailwind compatibility](https://tailwindcss.com/docs/compatibility),
  [web-features](https://github.com/web-platform-dx/web-features), [caniuse usage](https://caniuse.com/usage-table),
  [StatCounter US iOS](https://gs.statcounter.com/ios-version-market-share/mobile-tablet/united-states-of-america),
  [GOV.UK browsers](https://www.gov.uk/service-manual/technology/designing-for-different-browsers-and-devices),
  [HEY browsers](https://help.hey.com/article/830-what-browsers-are-supported).
- System: [k6](https://github.com/grafana/k6),
  [k6 open and closed models](https://grafana.com/docs/k6/latest/using-k6/scenarios/concepts/open-vs-closed/),
  [oha](https://github.com/hatoo/oha), [Goose](https://book.goose.rs/coordinated-omission/overview.html),
  [Toxiproxy](https://github.com/Shopify/toxiproxy), [fail-parallel](https://github.com/slatedb/fail-parallel),
  [Render free](https://render.com/docs/free), [Neon plans](https://neon.com/docs/introduction/plans),
  [Neon pooling](https://neon.com/docs/connect/connection-pooling).
