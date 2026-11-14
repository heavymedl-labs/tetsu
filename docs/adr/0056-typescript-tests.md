# 0056. TypeScript tests

- Status: Accepted
- Date: 2026-11-13

## Context

TypeScript in `web/` has formatting, linting, and type checks, but no tests, coverage, or boundaries: CONSTRAINTS.md's
exception E5. The passkey script is Tier 1 ([ADR-0005](0005-testing-and-performance.md)), and
[ADR-0040](0040-identity-and-sign-in.md) lets it land only once E5 closes
([#108](https://github.com/heavymedl-labs/tetsu/issues/108)). Islands follow, such as the class schedule
([#88](https://github.com/heavymedl-labs/tetsu/issues/88)).

Criteria, before looking at options: unit tests, branch coverage, and mutation testing work on the same tests; they
build on Vite, TypeScript, Biome, and Node as they are; no install scripts; they fit a later Storybook (#86) and the
browser tests ([ADR-0054](0054-browser-tests.md)); and they can fake the browser APIs the passkey script calls.
Evidence is in the [research](../research/typescript-tests.md).

## Decision

- **[Vitest](https://vitest.dev) runs the unit tests,** in Node with [jsdom](https://github.com/jsdom/jsdom) for the
  DOM, from the CLI only: no UI, API server, or browser mode. Tests sit beside their source as `*.test.ts`.
- **Coverage is Vitest's V8 provider,** with ADR-0005's limits by path: 100% of lines and branches for Tier 1 code,
  starting with `web/src/passkey/`, and 90% elsewhere.
- **[StrykerJS](https://stryker-mutator.io) mutation-tests every TypeScript source,** through its Vitest runner, and a
  surviving mutant fails the check unless marked equivalent with its reason, as with `cargo-mutants`.
- **Biome enforces the boundaries,** with no new tool: browser code imports no Node modules, no import cycles, each
  island imports only shared code under `web/src/shared/`, and nothing imports another module's private files.
- **What only a browser shows** (real WebAuthn, focus, layout) is ADR-0054's, through Chromium's virtual
  authenticator, not these tests.
- **All of it runs in `scripts/check.sh` and CI,** each tool pinned in `package.json`. The first code under test is
  real: the base64url encoding the passkey script needs.
- **E5 narrows to CSS tests,** which ADR-0054's browser tests fill
  ([#129](https://github.com/heavymedl-labs/tetsu/issues/129)); TypeScript gets its CONSTRAINTS.md row.

## Alternatives considered

- **Vitest browser mode for every test.** Real browser APIs, but StrykerJS cannot mutate tests run that way, so Tier 1
  code would lose mutation testing.
- **happy-dom.** Faster, but eight advisories through 2026-10-28, against none for jsdom, and less faithful to the
  standards.
- **Jest.** Needs its own TypeScript and module transform beside Vite, and Storybook's tests run on Vitest.
- **Node's `node:test`.** No new runner, but no coverage limits by path and no mutation runner.
- **dependency-cruiser.** Expresses more, but Biome already has the import rules `web/` needs.

## Consequences

- New npm tools: vitest 5.1.0 and @vitest/coverage-v8 (2026-11-02, MIT), jsdom 31.0.1 (2026-11-13, MIT), and
  @stryker-mutator/core with its Vitest runner 10.1.1 (2026-10-23, Apache-2.0), read 2026-11-13. None has an install
  script.
- StrykerJS is the weakest link: the only maintained option, with a few maintainers. If it stalls, Tier 1 TypeScript
  loses mutation testing, and the bar must be raised another way before more Tier 1 script lands.
- Mutation testing runs single-threaded, so check runs slow as `web/` grows; like `cargo-mutants`, it can skip
  unchanged code.
- Changes to `package.json`'s scripts, the Biome config, and CI need the maintainer's `Floor-reason:`, as the floor
  guard requires.
- Replacing a tool touches its config and `scripts/check.sh`, not the tests, except moving off Vitest.
- Revisit if StrykerJS gains browser mode, or the passkey script needs browser APIs jsdom cannot fake.

## Sources

- StrykerJS, [Vitest runner](https://stryker-mutator.io/docs/stryker-js/vitest-runner/), read 2026-11-13.
- Vitest, [security advisories](https://github.com/vitest-dev/vitest/security/advisories), read 2026-11-13.
- Biome, [noRestrictedImports](https://biomejs.dev/linter/rules/no-restricted-imports), read 2026-11-13.
- Yue Jia and Mark Harman, "An Analysis and Survey of the Development of Mutation Testing," *IEEE Transactions on
  Software Engineering*, 2011.
