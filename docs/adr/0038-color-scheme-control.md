# 0038. A color-scheme control without script

- Status: Superseded by 0070
- Date: 2026-11-11
- Superseded by [ADR-0070](0070-a-remembered-color-scheme.md): the control moves to the footer and remembers
- Supersedes the color-scheme rule in [ADR-0035](0035-component-sources-and-look.md)

## Context

[ADR-0035](0035-component-sources-and-look.md) has light and dark follow the system setting through Tailwind's
`dark:` variant, and holds a switch the visitor sets for islands
([#88](https://github.com/heavymedl-labs/tetsu/issues/88)), since a switch that remembers needs a script and a
stored choice. While the site is young, the maintainer wants to flip a page between schemes to check it, and wants
to stay on Tailwind's own approach. A review in November 2026 found a control in Tetsu's header that ADR-0035 does
not allow. It set the scheme through CSS's `light-dark()` instead of the `dark:` variant, so a `dark:` class would
have ignored it.

## Decision

**Tetsu's header has a System, Light, and Dark control, built on Tailwind's dark variant, without script.**

- The control is three radios. Tailwind's documented `@custom-variant dark` is redefined so the variant applies
  in a dark system scheme unless the control picks Light, and whenever it picks Dark.
- Each color token takes its light value, and its dark value under that variant. Every `dark:` class follows the
  same variant, so the tokens and the classes always agree.
- The choice lasts one page view, since nothing stores it. A choice that lasts waits for islands, as before.
- Business pages have no control, so they follow the system setting.
- A test checks that the dark tokens follow both the system scheme and the control.

## Alternatives considered

- **No control, as ADR-0035 says.** It is the simplest, but checking a page in the other scheme then means
  changing the system setting or emulating it in the browser's tools.
- **Tailwind's toggle with a stored choice.** It needs the site's first script and a looser content security
  policy, which belong with the first island.
- **`light-dark()` tokens.** They need no variant, but `dark:` classes then follow only the system.

## Consequences

Anyone can check a page in either scheme with one click. Following a link resets the scheme to the system's, and
the browser bar's `theme-color` follows only the system. If the control later stores a choice, an ADR supersedes
this one.

## Sources

- Tailwind CSS, "Dark mode", section "Toggling dark mode manually", <https://tailwindcss.com/docs/dark-mode>.
- MDN Web Docs, ":has()", <https://developer.mozilla.org/en-US/docs/Web/CSS/:has>.
- MDN Web Docs, "color-scheme", <https://developer.mozilla.org/en-US/docs/Web/CSS/color-scheme>.
