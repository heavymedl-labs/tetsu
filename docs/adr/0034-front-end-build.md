# 0034. Front-end build

- Status: Accepted
- Date: 2026-11-10
- Supersedes the standalone CLI and its build script in [ADR-0032](0032-styling-engine.md)

## Context

[ADR-0032](0032-styling-engine.md) builds Tetsu's stylesheet with Tailwind's standalone CLI and kept npm out of
the build: install scripts, an MPL-2.0 dependency, and no registry, license, or advisory checks. Since then the
maintainer expects TypeScript islands, some interacting, and npm packages that wrap `<tetsu-embed>` for
frameworks such as React, which mean npm and a bundler. Hashing file names for long caching would otherwise be
code of our own, which in other stacks a tool owns ([research](../research/front-end-build.md)).
[#86](https://github.com/heavymedl-labs/tetsu/issues/86) asks which tools build the design system. ASVS:
`v5.0.0-15.2.1` (components kept within their update and remediation time frames), `v5.0.0-15.2.3` (no
development functionality in production), and `v5.0.0-15.2.4` (dependencies only from the expected registry).

## Decision

- **Vite 8 builds the assets of Tetsu's own pages,** Tailwind through `@tailwindcss/vite` now and TypeScript
  islands later, into file names hashed by content and a manifest from each source to its built name. Sources
  live in `web/`. Every asset stays a file, since the policy blocks the `data:` URIs Vite otherwise makes:
  `default-src 'none'` today, and `'self'` sources once pages need fonts and images. ADR-0032's styling rules stand; only how the stylesheet is built changes.
- **The build is committed,** so Cargo and the image need no Node, and the push check rebuilds it from clean and
  fails on any difference. Watch builds keep removed classes, so only a clean build is committed.
- **Rust embeds the build and finds files through the manifest,** at compile time, in code the crate's tests
  cover, reading it with `serde_json` as a build dependency. The composition root hands each module the URLs its
  templates need, so no module depends on it. A hashed file is served at the exact path the manifest names,
  cached for a year as immutable. For now any other path under `/assets/` is a 404; whether it later serves
  unhashed files, such as `<tetsu-embed>`, stays open. [#90](https://github.com/heavymedl-labs/tetsu/issues/90) reserves `assets` as a slug.
- **A page open across a deploy may ask for files that are gone.** Pages are not cached, and islands reload the
  page when a chunk fails to load, as Vite's guide advises.
- **There is no development server.** Vite's 2026 advisories are all in it; one in 2024 was in built output.
- **One root `package.json` and lockfile hold every Node tool** at exact versions, including markdownlint-cli2
  and jscpd, which `check.sh` now fetches with `npx --yes`. Tools run only from the lockfile. Where packages
  published to npm live stays open.
- **Install scripts never run.** `.npmrc` sets `ignore-scripts=true`, npm's own `allowScripts` lists nothing, and
  `check.sh` installs with `--ignore-scripts`, which the environment cannot override. Any locked package with an
  install script fails the check unless listed with its reason: today only `fsevents`, an optional macOS file
  watcher Vite works without. A package that needs its script to run needs an ADR.
- **npm's version is pinned** like Node's, and `check.sh` checks it, as it does cargo-deny's.
- **npm dependencies get the same checks as Rust's** ([ADR-0028](0028-dependency-checks-revised.md)), on every
  push, reusing [#82](https://github.com/heavymedl-labs/tetsu/issues/82)'s code for dated entries:
  - **Sources:** the npm registry with an integrity hash; packages bundled inside one are covered by its hash,
    and local workspace packages are allowed. Registry signatures are verified for the packages installed on
    the machine checking, so CI covers Linux.
  - **Licenses:** one list for every npm package, wider than Rust's: ADR-0028's, plus MPL-2.0, 0BSD, Python-2.0,
    and PSF-2.0, each listed with its reason. MPL-2.0 is the one that matters: Lightning CSS, which Vite and
    Tailwind both use to write the CSS we ship, and which Tailwind's standalone CLI already contained. It asks
    only that changes to its own files be shared, and we change none. ADR-0032 counted it against npm; this
    accepts it.
  - **Advisories and deprecations** fail. An advisory we cannot clear gets an entry with ADR-0028's reason,
    issue, and 30-day renewal rules, and an entry that no longer matches fails. Without the network, the last
    passing advisory and deprecation check of the same lockfile counts for 7 days; sources, licenses, scripts,
    and entry dates always run.
  - **A fix the direct dependency lacks** is an exact override, with the same reason and renewal as an exception,
    removed once the dependency's own range admits the fix.
  - **The floor guard flags any change** to the npm configuration, the entries and licenses, `package.json`'s
    `overrides`, `allowScripts`, and `scripts`, and the TypeScript and Biome configuration, so the maintainer
    approves each.
- **Dependabot alerts cover npm,** on since 2026-11-10; its automatic npm pull requests stay off, since a Tailwind release changes the
  committed build. npm tools are updated by hand monthly, with a clean rebuild.
- **TypeScript is the front-end language,** checked by TypeScript 7's `tsc --noEmit`, and Biome formats and lints
  TypeScript and CSS, with warnings failing. jscpd and lizard cover `web/`. Its CONSTRAINTS.md row lands with the
  Vite config; tests, coverage, and boundaries are an exception until the first island
  ([#88](https://github.com/heavymedl-labs/tetsu/issues/88)), expiring 2027-02-10.

[#84](https://github.com/heavymedl-labs/tetsu/issues/84) holds how each rule is implemented and tested. Still
open in #86: plain custom elements or Lit, the token generator, where packages live, how `<tetsu-embed>` is
versioned and served, and tsdown or Vite's library mode. So this decision does not yet cover the research's C4,
packages for frameworks.

## Alternatives considered

- **Standalone binaries,** Tailwind's CLI and later esbuild, as Phoenix does. Fewer and smaller tools, and esbuild
  splits and hashes too, but Rust would read two outputs and packages need tsdown or `tsc` beside it.
- **No build,** with import maps, as Rails 8 does. Fewest tools, but TypeScript and npm packages need a build.
- **Rsbuild, Parcel, or Bun.** Each could do the job, but none is the common default with Tailwind 4 or beside a
  server-rendered backend.
- **Vite's development server with hot reload.** Faster feedback, but it is where Vite's advisories are, and
  Rust would need to proxy it.
- **Building the assets in the image.** Nothing generated in git, but every Rust build would need Node.
- **A crate such as rust-embed** to embed the files. Less code, but a library adoption for a short build step.
- **ESLint and Prettier,** the common default, as two tools with plugins. oxlint and oxfmt are from Vite's own
  toolchain, but oxfmt is before 1.0.

## Consequences

- #84 builds the stylesheet with Vite and drops the CLI pin and `scripts/build-css.sh`; #90 and #82 land first.
- Each machine and CI keep `node_modules`, about 235 MB, and checks at commit need it installed.
- Vite's advisories, mostly in its development server, block pushes until a release or an exception, often
  several a year.
- A rebuilt stylesheet gets a new name, so a diff that touches templates also replaces one built file.
- New protected files, the CONSTRAINTS.md row and exception, and jscpd's ignore of the build need a
  `Floor-reason:`. On acceptance, ADR-0032 notes that its build step is superseded here.

**Weakest evidence.** No island or package exists yet, so the bundler is chosen for expected needs, and the
research's C6, fewest tools, favoured the standalone binaries. Builds were
compared on one machine; #84 checks a macOS build matches CI's. tsdown and oxfmt are before 1.0, and Vite's work
leans on one main contributor.

**Cost to replace.** Low. Templates get URLs from one place, and the manifest is all Rust reads.

**Revisit** if islands need hot reload, if an advisory reaches `vite build`, if a needed package requires an
install script, if Linux and macOS builds differ, or if committed builds make reviews hard.

## Sources

- [Front-end build research](../research/front-end-build.md), 2026-11-10.
- Vite: [announcing Vite 8](https://vite.dev/blog/announcing-vite8),
  [backend integration](https://vite.dev/guide/backend-integration), and
  [load error handling](https://vite.dev/guide/build#load-error-handling).
- Laravel's Vite integration and [django-vite](https://github.com/MrBin99/django-vite): backends that read
  Vite's manifest.
- Rails' [Propshaft](https://judoscale.com/blog/how-propshaft-works) and Phoenix's
  [asset management](https://hexdocs.pm/phoenix/asset_management.html): the little-build alternative.
- [GHSA-64vr-g452-qvp3](https://github.com/advisories/GHSA-64vr-g452-qvp3): the 2024 advisory in Vite's built
  output.
