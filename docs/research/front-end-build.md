# Front-end build

Research for [#86](https://github.com/heavymedl-labs/tetsu/issues/86), 2026-11-10: which tools build Tetsu's
front-end assets, now one stylesheet and soon TypeScript islands, `<tetsu-embed>`, and packages for frameworks
such as React. It follows [ADR-0010](../adr/0010-evaluating-technology.md). Benchmarks are skipped: builds take
milliseconds and sit on no request path.

## Criteria

Set before looking at options:

| ID | Criterion | Why |
| --- | --- | --- |
| C1 | Builds Tailwind 4 from the templates ([ADR-0032](../adr/0032-styling-engine.md)) | Already decided |
| C2 | Hashes file names and says which name each source became | Long caching without stale styles after a deploy |
| C3 | Compiles TypeScript, and splits code islands share | The maintainer expects islands, some interacting |
| C4 | Builds packages that wrap `<tetsu-embed>` for frameworks such as React | The maintainer expects them on npm; [ADR-0031](../adr/0031-front-end-model-revised.md) allows only the framed embed on businesses' pages |
| C5 | The Rust build and the image need none of it | One runtime in production, and builds stay simple |
| C6 | Fewest tools one maintainer must learn and update | One maintainer |
| C7 | No install scripts, and registry, license, and advisory checks as strict as Rust's | ADR-0032 turned down npm for lacking these |

## Options

1. **Vite 8 owns every front-end asset,** and Rust reads its manifest. Tailwind runs as its plugin.
2. **Standalone binaries,** as Phoenix does: Tailwind's CLI, and later esbuild, which splits shared code, hashes
   names, and writes a metafile (checked with 0.28.2). Rust reads two outputs, and packages need `tsc` for types.
3. **No build,** as Rails 8 does: import maps and hashing alone. TypeScript and npm packages still need a build,
   so this fails C3 and C4.
4. **Rsbuild, Parcel, or Bun.** Each fits C1 to C4, but none is the common default with Tailwind 4 or beside
   server-rendered backends, and no reason to depart from it was found.

## Community practice

Server-rendered stacks fall into two groups. The evidence is project docs, maintained integrations, and a forum
answer: strong for what each does, weak for how well it holds up.

- **The bundler owns assets.** Laravel's Vite plugin reads Vite's manifest in Blade's `@vite`
  ([Laracasts](https://laracasts.com/discuss/replies/882661)), and
  [django-vite](https://github.com/MrBin99/django-vite) does the same. In Rust,
  [axum-vite](https://github.com/gko/axum-vite), a small crate by one author, shows the same pattern with an
  Askama example; Rust has no common default here.
- **Little or no build.** Rails 8's Propshaft only hashes files, beside import maps
  ([DHH](https://world.hey.com/dhh/you-can-t-get-faster-than-no-build-7a44131c),
  [Judoscale](https://judoscale.com/blog/how-propshaft-works)). Phoenix ships standalone esbuild and Tailwind with
  `mix phx.digest` ([docs](https://hexdocs.pm/phoenix/asset_management.html)), though its newer docs run `npm ci`
  as apps grow.
- In both groups a tool owns hashing and the manifest, not each app. No build works where pages need little
  JavaScript; TypeScript, shared island code, and published packages are what move teams to a bundler.
- **Packages for frameworks.** React 19 passes the
  [Custom Elements Everywhere](https://custom-elements-everywhere.com/) tests and sets properties and custom events
  on elements itself ([Callstack](https://callstack.com/blog/the-complete-developer-guide-to-react-19-part-3-upgraded-support)),
  so a package for it is mostly types. Older React needs a wrapper, as Stencil generates with `@lit/react`
  ([Stencil](https://stenciljs.com/docs/react)). [tsdown](https://github.com/rolldown/tsdown), on the same
  Rolldown core as Vite 8, is the current tool for publishing TypeScript libraries.

## Project health

Read 2026-11-10 from npm and GitHub. Contributors count those with commits in the last 26 weeks.

| Project | Latest | License | Weekly downloads | Open issues | Node | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| Vite | 8.3.7 | MIT | 264 million | 832 | 20.19 or 22.12+ | 26 contributors, but one made 46% of 607 commits |
| Rolldown | 1.2.18 in Vite 8.3.7 | MIT | 162 million | 376 | 20.19 or 22.12+ | 1.3.1 is out; Vite's bundler since [March 2026](https://vite.dev/blog/announcing-vite8) |
| `@tailwindcss/vite` | 4.3.5 | MIT | 62 million | Tailwind's | 20+ | Released with Tailwind itself |
| esbuild | 0.28.2 | MIT | 415 million | 659 | 18+ | 5 contributors; mostly one maintainer; last push 2026-08-09 |
| tsdown | 0.23.7 | MIT | 6.8 million | 41 | 22.18+ | Before 1.0; 30 contributors |
| TypeScript | 7.0.2 | Apache-2.0 | 417 million | n/a | 16.20+ | 7 is the native compiler |
| Biome | 2.6.6 | MIT or Apache-2.0 | 26 million | 517 | Any | Formatter and linter in one tool |
| oxlint, oxfmt | 1.93.0, 0.78.0 | MIT | 46, 29 million | 1,169 (Oxc) | 20.19 or 22.12+ | oxfmt is before 1.0 |
| ESLint, Prettier | 10.14.0, 3.9.10 | MIT | 204, 182 million | n/a | n/a | The common default, as two tools with plugins |

markdownlint-cli2 and jscpd are already in use through `npx --yes`; moving them into the lockfile changes how
they are installed, not whether.

**Advisories.** Every Vite advisory in 2026 is in its development server, such as `server.fs.deny` bypasses and
`launch-editor` on Windows. That is not a lasting property: GHSA-64vr-g452-qvp3, in 2024, was a cross-site
scripting flaw in scripts Vite puts in built output. esbuild's are in its development server. Rolldown,
`@tailwindcss/vite`, and tsdown have none. markdownlint-cli2 0.24.0, which `check.sh` runs today, brings in re2js
2.8.6 (GHSA-gg6h-894w-4m9c, high) and KaTeX 0.16.47 (GHSA-238p-pmpm-9mq7, low).

## How it fails

Spikes in scratch folders, installed with `--ignore-scripts`:

- **Install:** Vite, its Tailwind plugin, and Tailwind added 31 packages, 49 MB, against 109 MB for Tailwind's CLI.
  With Biome, TypeScript, markdownlint-cli2, and jscpd the tree is 127 packages and 235 MB, most of it Biome's and
  TypeScript's native binaries. Only `fsevents`, an optional macOS file watcher Vite pulls in, has an install script
  (`node-gyp rebuild`); with scripts off it is installed but never built, and Vite works without it. Native code
  comes as prebuilt packages per platform. Six packages ship inside `@tailwindcss/oxide-wasm32-wasi` (`inBundle`),
  so the lockfile has no source or hash for them; the parent's hash covers them. `npm audit signatures` verified
  every installed package's registry signature, 22 with provenance for Vite's tree and 29 for the full one. It
  checks only packages installed, so other platforms' binaries go unchecked.
- **Build:** 30 ms. The manifest maps `assets/tetsu.css` to `assets/tetsu-Bac1t9gS.css`, and the `@theme inline`
  mapping works: `.bg-primary{background-color:var(--tetsu-color-primary)}`. The manifest is `.vite/manifest.json`
  inside the output folder.
- **Reproducible on one machine:** the same bytes and name after deleting `node_modules` and the output and running
  `npm ci`. Linux and macOS builds were not compared.
- **Stale files:** an output folder outside Vite's root is not emptied, so each rebuild left the old hashed file
  beside the new one. A folder only Vite writes, emptied each build, avoids it.
- **Inlining:** with the default `assetsInlineLimit` of 4 KiB, a small font and an SVG became `data:` URIs, which
  `font-src 'self'` and `img-src 'self'` block. Setting it to 0 keeps every asset a file.
- **Watching:** `vite build --watch` rebuilt when an existing template changed, but not when a new one was added. It
  also kept classes removed from a template, so its output differs from a clean build's.
- **Shared code:** two island entries importing one module built to three files, the shared one split out and listed
  in each entry's `imports` in the manifest.
- **Unknown classes:** `text-accent`, which nothing defines, is dropped without a warning, so ADR-0032's template
  check is still needed.
- **Licenses:** the lockfile records each package's license. The tree has MIT, Apache-2.0, ISC, BSD-3-Clause, 0BSD,
  MPL-2.0 (Lightning CSS, which Tailwind and Vite use), BSD-2-Clause, `MIT OR Apache-2.0` (Biome), and Python-2.0
  and PSF-2.0 (argparse, through markdownlint-cli2). MPL-2.0 covers changes to its own files, not the CSS it
  outputs. Build tools also put code into the output: Tailwind's MIT header and Vite's preload helper.
- **Advisories:** npm `overrides` setting re2js to 2.8.7 and KaTeX to 0.18.2 cleared both, and markdownlint-cli2
  still linted the repository with no findings. An override pins exactly, so it must be removed once the parent's
  own range admits the fix.
- **Checks:** TypeScript 7's `tsc --noEmit` and Biome ran on a `vite.config.ts`. Biome parses Tailwind's `@source`
  and `@theme` only with `css.parser.tailwindDirectives` on, and exits 0 on warnings unless told not to. Biome's
  `noUndeclaredDependencies` with `devDependencies: false` failed a file in `src/` that imported `vite`, while the
  config could.

## Reading the manifest in Rust

ADR-0010 steps 1, 4, and 7 for `serde_json`, read 2026-11-10. It must parse Vite's JSON manifest in `build.rs`
and in the crate's tests. It is already in the lockfile through the benchmark harness. License MIT or Apache-2.0;
version 1.0.154; 1.6 billion downloads, 422 million in the last 90 days; last push 2026-10-16, with 251 open
issues; by serde's maintainers. The binary calls none of it, so the linker drops it. A hand-written parser would
avoid the dependency but repeat what serde already does, for a file Vite controls.

## Where it stands

Options 1 and 2 both meet C1, C2, C3 (shared code checked for Vite, and for esbuild with its splitting), and C5.
C4 is untested: no package was built. C7 holds once the checks the ADR describes exist, with a license list wider
than Rust's because Lightning CSS is MPL-2.0; the tree passes today with two overrides. C6 favours option 2, with
fewer and smaller tools. Option 1 gives one manifest for styles and
scripts, shares its core with tsdown for C4, and has a path to hot reload; option 2 has a smaller tree but two
outputs to read, and both projects lean on one main contributor. Weakest evidence: no island exists yet, so C3
rests on the maintainer's expectation; tsdown and oxfmt are before 1.0; and builds were compared on one machine.
