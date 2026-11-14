# TypeScript tests

Research for [ADR-0056](../adr/0056-typescript-tests.md) and [#108](https://github.com/heavymedl-labs/tetsu/issues/108),
2026-11-13: which tools give TypeScript in `web/` tests, coverage, mutation testing, and boundaries, so the passkey
script can meet the Tier 1 bar ([ADR-0005](../adr/0005-testing-and-performance.md),
[ADR-0040](../adr/0040-identity-and-sign-in.md)) and CONSTRAINTS.md's exception E5 can close. It follows
[ADR-0010](../adr/0010-evaluating-technology.md) steps 1, 2, 4, 5, and 7, plus step 6 for the one interaction that
decides the design. Benchmarks are skipped: none of these tools runs in production. Facts were read on 2026-11-13.

## Criteria

| ID | Criterion | Why |
| --- | --- | --- |
| C1 | Unit tests, branch coverage, and mutation testing work together on the same tests | Tier 1 needs all three |
| C2 | Builds on what is there: Vite 8, TypeScript 7, Biome 2.6, Node 26 | Fewest tools ([ADR-0034](../adr/0034-front-end-build.md)) |
| C3 | No npm install scripts, and licenses on ADR-0034's list | ADR-0034 |
| C4 | Fits a later Storybook (#86) and browser tests (ADR-0054) | Avoid a second runner |
| C5 | Can fake the browser APIs the passkey script calls, `navigator.credentials` among them | No real authenticator runs in tests |

## Options

- **Runner:** Vitest; Jest; Node's own `node:test`; Playwright's runner.
- **Simulated DOM:** jsdom; happy-dom; Vitest browser mode in real Chromium.
- **Coverage:** Vitest's V8 provider; its Istanbul provider; c8.
- **Mutation:** StrykerJS, the only maintained JavaScript mutation tester found.
- **Boundaries:** Biome's import rules; dependency-cruiser.

## Health

| Package | Latest | License | npm a week | Repository, last 90 days |
| --- | --- | --- | --- | --- |
| vitest | 5.1.0, 2026-11-02 | MIT | 180.4M | 372 commits by 60 authors; two lead |
| @vitest/coverage-v8 | 5.1.0, 2026-11-02 | MIT | 61.7M | Same repository |
| jsdom | 31.0.1, 2026-11-13 | MIT | 154.5M | 155 commits by 21 authors |
| happy-dom | 21.0.4, 2026-10-20 | MIT | 33.8M | 36 commits by 10 authors; one leads |
| @stryker-mutator/core and vitest-runner | 10.1.1, 2026-10-23 | Apache-2.0 | 6.6M | 13 commits by 7 authors |
| dependency-cruiser | 18.7.2, 2026-11-02 | MIT | 8.5M | Not checked once Biome was found to suffice |

None has an npm install script. Advisories, from GitHub and OSV:

- **Vitest:** several in 2026, all in its servers and browser mode: the mocker's redirect, manual mock registration,
  the API server's file access, and browser mode's provider commands (critical, 2026-07-08). Tetsu would run the
  CLI only, with no UI, API server, or browser mode.
- **happy-dom:** eight, the latest on 2026-10-28.
- **jsdom and StrykerJS:** none.

## How it fails

StrykerJS's Vitest runner does not support browser mode, by its own documentation; it also forces one worker,
bail on the first failure, and its own per-test coverage in place of Vitest's. So mutation testing needs tests
that run in Node. That decides the design: unit tests run in Node with jsdom, and real-browser behavior, including
WebAuthn through Chromium's virtual authenticator, belongs to ADR-0054's Playwright tests. Vitest's browser mode is
not needed now; Storybook could add it later as a separate project that mutation testing skips.

Biome 2.6, already installed, has `noRestrictedImports` with gitignore-style patterns, `noImportCycles`,
`noNodejsModules`, and `noPrivateImports`, and applies rules per path through `overrides`. That covers the boundaries
`web/` needs without dependency-cruiser.

## Community practice

Vitest is the common default beside Vite, and Storybook's test addon runs on it. From prior knowledge, not
checked this time: Jest needs its own transform for
TypeScript and ES modules, and `node:test` has no coverage thresholds per file or mutation runner. jsdom is the
older, more spec-faithful simulated DOM and the one most test suites use; happy-dom trades fidelity for speed.

## Weakest evidence

- StrykerJS is maintained by a few people, with 13 commits in 90 days, though releases continue. It is the only
  option, so a stall would mean no TypeScript mutation testing.
- That jsdom plus fakes is faithful enough for the passkey script is untested until the script exists; the browser
  tests (#129) are the backstop.

## Sources

- StrykerJS, [Vitest runner](https://stryker-mutator.io/docs/stryker-js/vitest-runner/): limitations, read
  2026-11-13.
- Vitest, [browser mode](https://vitest.dev/guide/browser/why) and
  [security advisories](https://github.com/vitest-dev/vitest/security/advisories), read 2026-11-13.
- Biome, [noRestrictedImports](https://biomejs.dev/linter/rules/no-restricted-imports) and
  [noImportCycles](https://biomejs.dev/linter/rules/no-import-cycles/), read 2026-11-13.
- npm registry and downloads API, GitHub API, and [OSV](https://osv.dev), queried 2026-11-13.
