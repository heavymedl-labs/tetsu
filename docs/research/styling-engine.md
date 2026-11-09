# Styling engine research

Research for [#86](https://github.com/heavymedl-labs/tetsu/issues/86) and ADR-0032, read 2026-11-09, following
[ADR-0010](../adr/0010-evaluating-technology.md). This is research, not a decision. Where it disagrees with an ADR,
the ADR wins. It covers how Tetsu's pages are styled. Islands, the token generator, and packaging are decided
elsewhere.

## Criteria

Written before looking at sources.

| # | Criterion | Why |
| --- | --- | --- |
| M1 | Styles come from the `--tetsu-*` token properties, so a tenant's theme changes at run time without a rebuild | [ADR-0031](../adr/0031-front-end-model-revised.md): a theme is token values stored with the tenant |
| M2 | Works with Askama templates and no JavaScript framework | ADR-0031 and [ADR-0007](../adr/0007-server-stack.md) |
| M3 | Output is small static stylesheets, so the policy can drop `'unsafe-inline'` and a phone loads them fast | [#84](https://github.com/heavymedl-labs/tetsu/issues/84); [Anvil Row](../reference-businesses/anvil-row-strength.md)'s customers book from their phones |
| M4 | Adds no runtime to the image or the server | One binary per [ADR-0003](../adr/0003-architecture.md) |
| M5 | Nothing Tetsu ships to businesses depends on licensed code | The maintainer's north star: a design system businesses can build on |
| W1 | One maintainer builds pages quickly | The project's size |

## Options

- **Hand-written CSS on the tokens.** The homegrown baseline, as #84 plans today.
- **Tailwind CSS through its standalone CLI.** One binary, no `package.json`.
- **Tailwind CSS through npm.** The same output, with a `package.json` and lockfile.
- **Tailwind CSS without Tailwind Plus,** and **MIT snippet sets** such as HyperUI and Preline, as sources of
  markup.

Rust ports of Tailwind exist but were not evaluated: Tailwind Plus's markup targets Tailwind CSS 4, and a port that
lags it would break that markup.

## Project health

Read 2026-11-09.

| Signal | Tailwind CSS |
| --- | --- |
| License | MIT |
| Releases | v4.3.5, 2026-11-06; v4.3.4, 2026-11-02; v4.3.3, 2026-07-16 |
| Security advisories | None published on the repository |
| Contributors | 11 with commits in the last 26 weeks |
| Vendor | Tailwind Labs cut about 75% of its engineering team on 2026-01-06, three of four engineers, and its founder put revenue down about 80% ([devclass](https://www.devclass.com/ai-ml/2026/01/08/tailwind-labs-lays-off-75-percent-of-engineers-thanks-to-brutal-impact-of-ai/4079571)). Tailwind CSS is MIT, so it outlives the company; Tailwind Plus depends on it |
| Use | About 176 million npm downloads a week for the `tailwindcss` package; about 99,000 GitHub stars |
| Open work | 45 issues and 75 pull requests |
| Standalone CLI | Built for Linux, macOS, and Windows, 109 MB for v4.3.5 on Linux, with a `sha256sums.txt` on each release |
| npm packages | `tailwindcss` and `@tailwindcss/cli` carry SLSA provenance; `npm audit signatures` verified 14 attestations in the installed tree, 26 MB |

None of the three releases lists a security fix; v4.3.4 adds a Turbopack loader and changes class detection. The
`sha256sums.txt` comes from the same release as the binary, so it shows the download is intact, not who made it,
and a version pinned in a script gets no advisory alerts. The npm route needs a local install: `npx` alone cannot
resolve `@import "tailwindcss"`.

**Tailwind Plus**, which the maintainer already pays for, adds licensed HTML for page sections, and Elements
(`@tailwindplus/elements` 1.0.22 on npm), headless custom elements shipped as an ES module
([announcement](https://tailwindcss.com/blog/vanilla-js-support-for-tailwind-plus)).

## License

The [Tailwind Plus license](https://tailwindcss.com/plus/license) was read through the maintainer's account. It
allows a commercial SaaS application by name, and says customers of a SaaS product are not "Clients". It forbids:

- products "designed to allow an End User to build their own End Products" from the components, such as a website
  builder;
- a UI library made from them, or one that requires end users to depend on the Elements library;
- redistributing the components or Elements apart from an End Product, or publishing a repository of them.

A Personal license covers one individual and forbids sharing access. A Team license covers up to 25 employees and
contractors of a business, and its products must belong to the licensee or its clients. Neither mentions AI
agents. The license also forbids producing anything Tailwind Labs deems competitive, and an admin panel tool sold
or given away, like Laravel Nova; a staff app inside Tetsu is part of the SaaS, not such a tool. It invites
questions to support.

Tetsu's own pages clearly qualify. A tenant's page that the tenant fills with data and colors is a closer call, and
a framed embed on a business's site, often built with a site builder, is closer still. Those readings are not
settled by the text.

## Community practice

- **Server-rendered frameworks ship Tailwind through the standalone CLI.** Phoenix's
  [tailwind](https://github.com/phoenixframework/tailwind) package installs the standalone CLI, and Rails'
  [tailwindcss-rails](https://github.com/rails/tailwindcss-rails) depends on a gem that installs the executable.
  Strength: strong, since both are each framework's own packaging. They have no npm tree to lean on; Tetsu's
  checks already run Node, which makes npm's provenance and advisories available.
- **Class order is usually sorted** by `prettier-plugin-tailwindcss`, the community default, a choice for the
  template row in CONSTRAINTS.md.
- **Reuse lives in templates, not in `@apply`.** Tailwind's documentation recommends components or partials for
  repeated markup ([managing duplication](https://tailwindcss.com/docs/styling-with-utility-classes#managing-duplication)).
  In Askama that means includes and macros.

## How it fails

Tested with the v4.3.3 standalone CLI, checksum matched, on small Askama templates, then again with v4.3.5 from
npm across two crates' templates.

| Test | Result |
| --- | --- |
| Map a color to a token with `@theme inline` | `.text-primary{color:var(--tetsu-color-primary)}`: the tenant's value applies at run time |
| An opacity modifier on a token color, `bg-primary/50` | `color-mix(in oklab, var(--tetsu-color-primary) 50%, transparent)` |
| Built-in colors cleared with `--color-*: initial` | `bg-indigo-600` and `text-white` produce no CSS and no warning |
| Built-in colors kept | `bg-indigo-600` compiles to a fixed color, so a Tailwind Plus snippet ignores the tenant's theme |
| Spacing, radius, and shadow with colors cleared | Still generated from Tailwind's scales |
| Class names inside `{% if %}` branches | Found, both branches |
| A class built from a variable, `text-{{ tone }}-600` | Not generated |
| A class in Rust source, with sources limited to `templates/` | Not generated |
| `@source "crates/*/templates"` | No utilities at all, and no warning |
| `@source "crates/*/templates/**/*.html"` | Finds both crates' templates |
| Spacing and radius mapped to tokens | `calc(var(--tetsu-space-unit) * 4)` and `var(--tetsu-radius-lg)` |
| A fixed color, `bg-[#ff80b5]`, with colors cleared | Compiles, so only a check catches it |
| `dark:` turned off with `@custom-variant` | No `prefers-color-scheme` rule |
| An inline style as a class, `[clip-path:polygon(…)]` | Compiles |
| Build time and size for the sample | About 20 ms; 5.1 KB minified, 1.8 KB gzipped |

If a page sets no `--tetsu-*` values, a mapped utility has no value, so the shared stylesheet needs defaults on
`:root`. Tailwind Plus marketing sections also carry inline `style=` attributes, such as clip paths, which
`style-src 'self'` blocks.

Today's policy in `crates/tetsu/src/serve.rs` is `style-src 'unsafe-inline'` with no `script-src`, so #84 adds
`'self'` for styles. Tailwind 4 needs Chrome 111, Safari 16.4, and Firefox 128
([compatibility](https://tailwindcss.com/docs/compatibility)); without `color-mix`, `bg-primary/50` falls back to
an opaque color.

**Elements, for #86.** Reading 1.0.22's bundle: it patches `attachShadow` and `attachInternals`, adds global event
classes, and its `command` polyfill turns `oncommand` attributes into `new Function`, which a policy without
`'unsafe-eval'` blocks. `<el-selectedcontent>` copies option nodes into itself. It bundles MIT code from
Floating UI and three polyfills without license notices. Much of it fills gaps in native `<dialog>`, `popover`, and
`commandfor`, which #86 should compare.

## Benchmark

Skipped: the engine runs at build time, not on a hot path, and the output is a static file. Its size is measured
above, and #84's tests cover what the server sends.

## What this cannot show

No real Tailwind Plus section built against the tokens, no measure of how quickly one maintainer builds pages with
it, and no contrast check of tenant colors against its light-on-dark text. Whether an AI agent working for the
license holder counts as shared access is a reading, not text in the license.

## Note, 2026-11-09

After three reviews, the maintainer set licensing questions aside for now and asked for implementation choices
that keep prototyping fast. ADR-0032 therefore keeps Tailwind's palette and scales, so snippets render at once,
maps only the tenant-themed brand colors to tokens, and returns to the standalone CLI: the npm route's install
script, MPL-2.0 packages, and the checks an npm tree would need outweighed provenance for a tool that writes
static CSS.
