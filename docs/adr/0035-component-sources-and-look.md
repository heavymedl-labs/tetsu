# 0035. Component sources and look

- Status: Accepted
- Date: 2026-11-10
- Color-scheme rule superseded by [ADR-0038](0038-color-scheme-control.md)
- Supersedes Tailwind Plus as a source, and the `dark` variant being off, in [ADR-0032](0032-styling-engine.md)

## Context

[ADR-0032](0032-styling-engine.md) drew Tetsu's page sections from Tailwind Plus, kept for Tetsu's own pages, while
anything businesses use is Tetsu's own components. Its license forbids UI libraries built from it and leaves
business pages and embeds unclear ([styling-engine research](../research/styling-engine.md#license)), so two
component sets would grow side by side, and the one businesses use could never draw on it. The maintainer wants one
open foundation for both, Tetsu's own pages in the look and feel of Linear, and light and dark color schemes that
follow the system setting ([research](../research/component-sources.md)).

## Decision

- **Every page is built from Tetsu's own components,** Askama templates and macros in `web/templates/` on the
  `--tetsu-*` tokens, for Tetsu's pages and businesses' alike. Their markup is written by us or adapted from MIT
  sources: [HyperUI](https://github.com/markmead/hyperui) first, Flowbite's core HTML second, and Heroicons for
  icons. No Tailwind Plus code enters the repository, adapted or not.
- **Tailwind Plus is the standard each component is held to.** Before a component lands, it is compared with the
  nearest Tailwind Plus counterpart, read from the local library: spacing and type scale, behavior at each
  breakpoint, hover, focus, and disabled states, accessibility attributes and screen-reader text, and both color
  schemes. Where ours falls short, we close the gap in our own markup. The comparison guides; it never copies.
- **Each adapted section names its source** in a comment at its top, and `THIRD-PARTY-NOTICES.md` carries each
  source's license and copyright notice, as MIT and the OFL require.
- **Inter is the typeface,** from `@fontsource-variable/inter` (OFL-1.1), built by Vite and served from `/assets/`.
  The policy gains `font-src 'self'`, and `.config/npm-checks.toml` allows OFL-1.1, a font license, with its reason.
- **Tetsu's own pages take Linear's look and feel:** restrained neutrals with one accent, Inter with tight display
  tracking, hairline borders, soft gradients and glows, small mono labels, and generous space. That is a style to
  draw on; Linear's code, images, logo, copy, and distinctive layouts stay out, so Tetsu keeps its own identity.
- **Business pages keep each tenant's colors** as their accents, on the same components, with neutral type.
- **Light and dark color schemes follow the system setting,** through Tailwind's `dark:` variant on
  `prefers-color-scheme`, with no script. The tokens gain a dark set, redefined for the dark scheme. A switch the
  visitor sets waits for islands ([#88](https://github.com/heavymedl-labs/tetsu/issues/88)), since it needs a script
  and a stored choice.

ADR-0032's build, tokens, template check, and `@theme inline` mapping stand.

## Alternatives considered

- **Keep Tailwind Plus for Tetsu's pages,** and MIT sources for businesses'. The most polished sections, but two
  component sets in two styles, and the license blocks publishing anything drawn from it.
- **Flowbite first.** More sections, but its interactive parts need its own script, and its look is more opinionated
  to restyle.
- **daisyUI.** Component class names through a Tailwind plugin, so pasted markup depends on the plugin and does not
  read as utilities.
- **Our own markup only.** No source to credit, but every section designed from scratch.
- **No outside standard.** Less work per component, but quality would rest on taste alone, with nothing to measure
  a section against.
- **Dark only, as Linear defaults to.** Simpler tokens, but the maintainer wants both schemes, and many visitors
  prefer light.

## Consequences

- The pages #84 built, adapted from Tailwind Plus, are rebuilt from these sources, which a follow-up issue tracks.
- Every token needs a dark value, and #96's contrast check covers both schemes.
- Tailwind Plus stays a paid reference: the bar every component is compared against, and outside the repository at
  most a throwaway prototype, never code we keep.
- Allowing OFL-1.1 in `.config/npm-checks.toml` changes a protected file, so it needs a `Floor-reason:`.

**Weakest evidence.** No page has been built from HyperUI yet, and it has one maintainer, though its sections are
copied, not installed.

**Cost to replace.** Low. Sections are our own templates; a source only seeds them.

**Revisit** if a source changes its license, if HyperUI and Flowbite lack sections Tetsu needs, or if visitors need
to choose a scheme themselves.

## Sources

- [Component sources research](../research/component-sources.md), 2026-11-10.
- [HyperUI](https://github.com/markmead/hyperui), [Flowbite](https://github.com/themesberg/flowbite),
  [Heroicons](https://github.com/tailwindlabs/heroicons), and [Inter](https://github.com/rsms/inter).
- Tailwind CSS: [dark mode](https://tailwindcss.com/docs/dark-mode).
