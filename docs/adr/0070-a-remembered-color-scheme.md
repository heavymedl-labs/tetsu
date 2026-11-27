# 0070. A remembered color scheme, in the footer

- Status: Accepted
- Date: 2026-11-17
- Supersedes [ADR-0038](0038-color-scheme-control.md)
- The control in the footer of the admin pages, now every realm's account area and login pages, superseded by
  [ADR-0095](0095-the-color-scheme-on-a-preferences-page.md): a Preferences page

## Context

[ADR-0038](0038-color-scheme-control.md) put a System, Light, and Dark control in Tetsu's header without script,
so a choice lasted one page view, and left a choice that lasts for islands, which now exist
([ADR-0031](0031-front-end-model-revised.md)). The maintainer wants the control on all of Tetsu's own pages,
operator pages included, and the choice kept. Business pages are left as they are: their own colors lead
([ADR-0035](0035-component-sources-and-look.md)) and they follow the system setting.

Sites that keep a visible control put it in the footer or settings, with System as the default
([Vercel Geist](https://vercel.com/geist/theme-switcher)); Chrome's guidance prefers following the system and
offering the override in settings, and says a control must always be able to return to System
([Chrome](https://developer.chrome.com/docs/modern-web-guidance/guides/ui-components/dark-mode-toggle)). Most sites
keep the choice in `localStorage` and apply it with an inline script before the page draws, which Tetsu's content
security policy forbids. A cookie the server reads lets the page arrive in the chosen scheme, with no flash and no
script to apply it.

## Decision

- **The control moves to the footer** of Tetsu's own pages and the operator pages: the same three radios, System
  first, which still switch the page without script.
- **The choice is kept in a `theme` cookie,** `light` or `dark`, and no cookie means System. The server reads it
  and renders that radio checked, so the page arrives in the chosen scheme. Any other value counts as System.
- **A `<tetsu-scheme>` island writes the cookie** when the radio changes: for a year, on the whole site, `Secure`
  and `SameSite=Lax`, readable by script since it holds nothing secret. Choosing System removes it.
- **Pages with the control load Tetsu's own script,** so the site's policy gains `script-src 'self'` on them, as
  the operator pages have. Pages that differ by the cookie say so with `Vary: Cookie`.
- **The page names the kept scheme from its first byte:** `<html data-scheme>` holds it, which the styles follow
  until the browser has read the footer's control, and the `color-scheme` meta and the browser bar's `theme-color`
  are the kept scheme's, or both schemes' when none is kept. So a page kept light never paints dark for a moment in
  a dark system, even while the top of a long page is drawn before its footer is read.
- **The root page is still rendered once,** in each of the three schemes, and served by the cookie.
- **A test checks** that each scheme's page comes back for its cookie, and that the dark tokens still follow both
  the system scheme and the control.

## Alternatives considered

- **`localStorage` and an inline script,** as most sites do. It needs `script-src 'unsafe-inline'` or a hash for
  every page's script, and the page still draws once before a module script could apply the choice.
- **A form that posts the choice,** which needs no script. Every switch reloads the page, and the site's policy
  would gain `form-action 'self'` instead.
- **A two-state toggle,** System and the opposite of the current scheme, which Chrome and Lea Verou suggest for site
  chrome ([Verou](https://lea.verou.me/blog/2026/dark-mode-toggles/)). The footer has room for all three, which the
  control already has.
- **No control,** following the system alone. The maintainer checks pages in both schemes and wants it.

## Consequences

- A visitor's choice holds across Tetsu's pages and visits, until they choose System or the cookie ends.
- The root page holds three rendered copies, and a cache keeps one per cookie.
- Tetsu's own pages run a script for the first time, its own and nothing inline.
- Business pages keep both schemes in their meta and follow the system, as before.

## Sources

- Chrome for Developers, "Dark mode toggles",
  <https://developer.chrome.com/docs/modern-web-guidance/guides/ui-components/dark-mode-toggle>.
- Vercel Geist, "Theme Switcher", <https://vercel.com/geist/theme-switcher>.
- Lea Verou, "Dark mode toggles: two states are enough", 2026-08-06,
  <https://lea.verou.me/blog/2026/dark-mode-toggles/>.
- web.dev, "User preference media features client hints headers",
  <https://web.dev/articles/user-preference-media-features-headers>.
