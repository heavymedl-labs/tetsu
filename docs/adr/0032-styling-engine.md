# 0032. Styling engine

- Status: Accepted
- Date: 2026-11-09
- Standalone CLI and build script superseded by [ADR-0034](0034-front-end-build.md)
- Tailwind Plus as a source, and the `dark` variant being off, superseded by [ADR-0035](0035-component-sources-and-look.md)

## Context

[ADR-0031](0031-front-end-model-revised.md) gives Tetsu's design system three layers: design tokens generated into
`--tetsu-*` CSS properties, base CSS on them, and Web Components. It left the tools to
[#86](https://github.com/heavymedl-labs/tetsu/issues/86). One maintainer builds every page and wants to prototype
good-looking UI quickly, drawing on Tailwind Plus, which the maintainer already has, for templates, components,
and ideas ([research](../research/styling-engine.md)).

## Decision

- **Tailwind CSS 4 styles Tetsu's pages,** built by its standalone CLI. `scripts/install-tools.sh` pins the release,
  first 4.3.5, and checks each platform's binary against a recorded SHA-256, as it does for Betterleaks.
- **A tenant's brand comes from tokens; the rest is Tailwind's.** The input stylesheet maps the brand colors and
  their text colors, such as `--color-primary: var(--tetsu-color-primary)` and
  `--color-on-primary: var(--tetsu-color-on-primary)`, with `@theme inline`, so a tenant's theme changes at run
  time. Tailwind's own palette, spacing, type, radius, shadows, and breakpoints stay as Tetsu-wide defaults, so a
  pasted snippet works at once. Making one brand-aware means changing its accent classes to the token names.
  [#87](https://github.com/heavymedl-labs/tetsu/issues/87)'s generator later writes these mappings from the DTCG
  file.
- **Tenants set colors only.** Tenancy derives or checks each text color against its background for contrast,
  for new and existing tenants. The shared stylesheet sets every token's default on `:root`, and a tenant's theme
  sheet loads after it.
- **The `dark` variant is off** until the tokens have a dark set, so a snippet's `dark:` classes do nothing.
- **Only templates are scanned:** `source(none)`, then `crates/*/templates/**/*.html`, so a new module's templates
  are found without a change. The root page moves into `crates/tetsu/templates/`. Class names are written whole in
  templates, never built from a variable or in Rust code, and repeated markup becomes an Askama include or macro.
  Islands change state through attributes, such as `aria-expanded` or `data-state`, styled with Tailwind's
  `aria-*` and `data-*` variants, so their scripts add no classes.
- **A tested script checks templates** ([ADR-0014](0014-checks-and-duties.md)). It fails on a class the stylesheet
  does not define, except marker classes such as `group` and `peer`, on `{{ }}` inside a `class` attribute, and on
  a `style=` attribute, which the policy blocks. A snippet's inline style becomes an arbitrary-property class.
- **`scripts/build-css.sh` builds the stylesheet,** with `--watch` while prototyping. The output is committed at
  `crates/tetsu/assets/tetsu.css` and served from the binary, so Cargo and the image build need no CLI.
  `scripts/check.sh` rebuilds it on push and fails if it differs, and jscpd skips it.
- **This stylesheet is for Tetsu's own pages.** A design system that businesses install, and `<tetsu-embed>`, are
  Tetsu's own components on the same tokens.

Islands' tools and packaging stay open in #86, the staff app's styling and the browser floor in
[#14](https://github.com/heavymedl-labs/tetsu/issues/14).

## Alternatives considered

- **Hand-written CSS on the tokens.** No tool, but every section is designed from scratch, which is slow to
  prototype.
- **Every value from a token, Tailwind's palette and scales cleared.** Fully themeable, but every snippet needs
  rewriting before it renders, and tenants theme only colors today.
- **Tailwind CSS through npm.** Provenance and advisory alerts, but an install script runs on every machine, and
  the npm tree would need its own license, registry, and advisory checks for a tool that only writes static CSS.
- **Open Props.** Token-native CSS properties, but no utility classes, and Tailwind Plus markup does not use it.
- **A class prefix.** Avoids clashing with a host page's styles, but ADR-0031's embeds are frames, so nothing
  clashes, and every snippet's classes would change.

## Consequences

- [#84](https://github.com/heavymedl-labs/tetsu/issues/84)'s shared stylesheet is built by Tailwind, and the policy
  gains `style-src 'self'` without `'unsafe-inline'`, and `img-src` and `font-src 'self'` when pages need them.
  A snippet's placeholder images and logos are replaced, and fonts are served from Tetsu.
- Templates carry long class lists. Replacing Tailwind later means rewriting them, though tokens and themes stay.
- Tailwind 4 needs Chrome 111, Safari 16.4, or Firefox 128, an input to #14's browser floor.
- The template rows in CONSTRAINTS.md are filled before #84 lands ([ADR-0017](0017-format-and-lint.md)), and
  jscpd's new ignore changes a protected file, so it needs a `Floor-reason:`.
- The CLI, about 109 MB, joins the tools each machine and CI installs and caches.

**Weakest evidence.** No real Tailwind Plus section has been built against the tokens yet, and Tailwind Labs cut
most of its engineering team in January 2026, though Tailwind CSS is MIT and widely used.

**Revisit** if tenants need to theme more than colors, if the tokens gain a dark set, if Tetsu builds a component
library businesses install, or if class lists make templates hard to review.

## Sources

- [Styling engine research](../research/styling-engine.md), 2026-11-09.
- Tailwind CSS: [theme variables](https://tailwindcss.com/docs/theme),
  [managing duplication](https://tailwindcss.com/docs/styling-with-utility-classes#managing-duplication), and
  [compatibility](https://tailwindcss.com/docs/compatibility).
- Phoenix's [tailwind](https://github.com/phoenixframework/tailwind) and Rails'
  [tailwindcss-rails](https://github.com/rails/tailwindcss-rails): server-rendered frameworks shipping the
  standalone CLI.
