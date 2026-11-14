# 0053. Supported browsers

- Status: Proposed
- Date: 2026-11-13

## Context

Tetsu states no browser floor. The site leans on newer CSS (exclusive `details`, popovers, style queries,
scroll-driven animations), and a review on 2026-11-13 could not judge how its tabs behave in older browsers, because
nothing said which browsers matter. Three places pick targets on their own: Tailwind 4 needs Chrome 111, Safari 16.4,
and Firefox 128; Vite 8 defaults to Baseline Widely available as of 2026-01-01 (Firefox 114); and Lightning CSS
minifies to Vite's target. Tests need a floor too: [ADR-0054](0054-browser-tests.md) runs current engines only.

Criteria, before looking at options: one statement sets the floor, and the build and tests follow it; a dependency
update cannot raise it silently; it covers the phones customers actually carry, including iPhones that stopped at
iOS 18; and newer CSS stays usable as an enhancement. Evidence is in the
[research](../research/browser-and-system-testing.md#supported-browsers).

## Decision

- **Two levels.**
  - **Fully supported:** the current and previous major versions of Safari and iOS Safari, Chrome, Edge, Firefox,
    and Samsung Internet. Everything works as designed.
  - **Functional:** down to Tailwind's floor, Safari 16.4, Chrome and Edge 111, and Firefox 128. Content,
    navigation, forms, and keyboard access work; motion, tab exclusivity, and popover polish may be missing.
  - Below the floor, best effort only.
- **One statement.** A `.browserslistrc` holds the floor as Baseline Widely available frozen at a date, plus
  Tailwind's minimums, so the floor moves only when someone changes the file. `web/vite.config.ts` reads it for
  `build.target`, `build.cssTarget`, and Lightning CSS's targets.
- **Newer than the floor means an enhancement.** A feature that is not Widely available at the frozen date sits
  behind `@supports` or degrades by itself, and a page must read and work without it. Scroll-driven animations stay
  behind `@supports` until they are Widely available (about 2029): iOS 18.7, which lacks them, was 10.5% of US iOS
  traffic in October 2026.
- **The test matrix follows.** CI runs the current engines (ADR-0054). The floor is checked statically, by targets
  and `@supports`, and by a reduced-motion run that shows pages working without enhancements. Real Safari on Apple
  hardware, an iPhone on iOS 18, and Firefox ESR are checked by hand before a release.
- **Review yearly,** or when a feature Tetsu needs becomes Widely available, against usage data then.

## Alternatives considered

- **Latest versions only,** as GitHub states. Simplest, but drops iPhones that stop at iOS 18, about one in ten US
  iPhones in 2026, for a booking site customers open from a text message.
- **Baseline Widely available alone.** One live query, but it moves with every browserslist update, and it sits
  below Tailwind's Firefox floor.
- **Name versions and test each in a device cloud.** Exact, but real-device automation starts near $175 a month,
  past the free tiers ([ADR-0012](0012-first-hosting-and-deploys.md)).
- **No statement.** What we had: each review guesses.

## Consequences

- The work is [#128](https://github.com/heavymedl-labs/tetsu/issues/128): a small bridge in `web/vite.config.ts` and a `.browserslistrc`, and a check that the built CSS targets match it.
  Whether `@tailwindcss/vite` honors Vite's CSS target is unchecked and must be confirmed.
- Exclusive `details` (Safari 17.2) and popover (iOS 18.3) are Newly available, so pages using them must still work
  where they are missing, as the feature panels planned in [#133](https://github.com/heavymedl-labs/tetsu/issues/133)
  must.
- Customers on a phone below the floor may see a broken page. In October 2026 iOS Safari below 16.4 was about 0.3%
  of global traffic.
- Revisit if analytics from real tenants show a different mix, or a needed feature has no fallback.

## Sources

- web.dev, [Baseline](https://web.dev/baseline), read 2026-11-13.
- [browserslist](https://github.com/browserslist/browserslist), Baseline queries, read 2026-11-13.
- Vite, [build options](https://vite.dev/config/build-options), and
  [Lightning CSS transpilation](https://lightningcss.dev/transpilation.html), read 2026-11-13.
- Tailwind CSS, [compatibility](https://tailwindcss.com/docs/compatibility), read 2026-11-13.
- GOV.UK Service Manual,
  [designing for different browsers and devices](https://www.gov.uk/service-manual/technology/designing-for-different-browsers-and-devices),
  updated 2026-01-30.
- [StatCounter, iOS versions in the US](https://gs.statcounter.com/ios-version-market-share/mobile-tablet/united-states-of-america),
  October 2026.
