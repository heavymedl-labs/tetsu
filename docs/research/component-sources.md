# Component sources

Research for replacing Tailwind Plus as the source of page sections, 2026-11-10. It follows
[ADR-0010](../adr/0010-evaluating-technology.md) steps 1, 4, and 7: sections are copied into Tetsu's own templates
and adapted, so no library is installed at run time, apart from a font.

## Criteria

| ID | Criterion | Why |
| --- | --- | --- |
| C1 | A license that lets Tetsu ship, adapt, and publish the result, including in components businesses use | Tailwind Plus forbids UI libraries built from it and leaves business pages and embeds unclear ([styling-engine research](styling-engine.md#license)) |
| C2 | Plain HTML with Tailwind 4 classes | Askama templates; no React or framework runtime |
| C3 | Light and dark versions of each section | The maintainer wants both, following the system setting |
| C4 | No JavaScript needed for the static parts | The policy runs no scripts until islands exist ([#88](https://github.com/heavymedl-labs/tetsu/issues/88)) |
| C5 | Enough marketing and application sections for Tetsu's pages, business pages, and sign-in | Fewer pieces written from scratch |

## Options

1. **HyperUI.** Copy-paste sections, each with a dark version.
2. **Flowbite's core.** A larger set, in HTML, whose interactive parts need its own script.
3. **daisyUI.** Component class names through a Tailwind plugin, so markup reads `btn` rather than utilities, and a
   pasted snippet depends on the plugin.
4. **Our own markup only.** No outside source, but every section designed from scratch.
5. **Keep Tailwind Plus for Tetsu's pages.** Most polished, but fails C1 and keeps two component sets.

## Project health

Read 2026-11-10 from GitHub and npm.

| Project | License | Stars | Open issues | Last push | Notes |
| --- | --- | --- | --- | --- | --- |
| [HyperUI](https://github.com/markmead/hyperui) | MIT | 12,279 | 4 | 2026-10-25 | 269 sections in `public/examples/`, each `N.html` and `N-dark.html`, written with `dark:` classes; one maintainer |
| [Flowbite](https://github.com/themesberg/flowbite) | MIT | 9,382 | 259 | 2026-06-27 | v4.0.2; Pro parts are paid and separate |
| [Heroicons](https://github.com/tailwindlabs/heroicons) | MIT | 23,907 | 6 | 2026-05-12 | Tailwind Labs' icons, copied as inline SVG |
| [Inter](https://github.com/rsms/inter) | OFL-1.1 | 20,056 | 149 | 2024-11-19 | v4.1; on npm as `@fontsource-variable/inter` 5.3.0, OFL-1.1 |

## Visual direction

The maintainer wants Tetsu's own pages to take the look and feel of [Linear](https://linear.app): restrained
neutrals and one accent, Inter with tight display tracking, hairline borders, soft gradients and glows behind key
elements, small mono labels, and generous space. That is a style to draw on, not a design to copy: Linear's code,
images, logo, copy, and distinctive layouts stay out. Business pages keep each tenant's colors.

## Where it stands

HyperUI meets C1 to C5 and adapts best: its dark versions already use the `dark:` variant the system setting
drives. Flowbite is the second source, for sections HyperUI lacks, taking only its HTML. Weakest evidence: HyperUI
has one maintainer, which matters little since its sections are copied, not installed; and no Tetsu page has yet
been built from it.
