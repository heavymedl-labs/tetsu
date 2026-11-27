# Where a person picks the color scheme

Researched 2026-11-26 for [#111](https://github.com/heavymedl-labs/tetsu/issues/111), after the maintainer asked to
take the color scheme control and Tetsu's mark out of the footer of the account area and the login and join pages,
and to give the control a page of its own in the account area. What is settled is not repeated here: the control
itself, its cookie, and how a page arrives in the kept scheme ([ADR-0070](../../adr/0070-a-remembered-color-scheme.md)).
Sources were read through search summaries on 2026-11-26, and say so where they are a third party rather than the
vendor's documentation.

## Summary

Products with an account area put the color scheme on a page of a person's own settings, not in a footer: GitHub's
Appearance page, Linear's Preferences, Slack's Preferences, and Vercel's Account Settings each offer the system's,
light, and dark. None of the login pages read shows the control. Linear and Slack keep the choice on each device, as
Tetsu's cookie does, while GitHub and Notion keep it on the account. Tetsu's dark styles already follow the scheme the server writes on `<html>` when no control is on
the page, so taking the control out of most pages changes no page's colors.

## Leaders keep the scheme in a person's own settings

**Takeaway:**

A settings page named for the person's choices holds the scheme, as three options with System first.

**Cited Findings:**

| Product | Page | Choices | Kept |
| --- | --- | --- | --- |
| GitHub | Settings, Appearance, `github.com/settings/appearance` | Sync with system, light, or dark, with high-contrast and colorblind themes | On the account |
| Linear | Settings, Account, Preferences, `/<workspace>/settings/account/preferences` | Light, dark, or system | On each device |
| Vercel | Account Settings, Theme | System, light, or dark | Not found |
| Slack | Preferences, Appearance | Light, dark, or the system's | On each device |
| Notion | Settings, Appearance | Always light, always dark, or the system's | On the account, for every workspace |

- Slack: "device specific", so setting it on the desktop app does not reach the mobile app
  ([Slack Help](https://slack.com/help/articles/360019434914)).
- Notion: a choice applies to every workspace on the account ([Notion Help](https://notion.com/help/account-settings));
  a third-party guide says it follows the account across devices, which Notion's help does not say.

- GitHub: Settings from the profile menu, then Appearance in the sidebar, then "Theme mode"
  ([GitHub Docs](https://docs.github.com/en/get-started/accessibility/managing-your-theme-settings)).
- Linear: Settings, Account, Preferences, and the theme "is one of the few that is not synced across platforms"
  ([Linear Docs](https://linear.app/docs/account-preferences)). The address is the one Linear's router settled on
  for `/settings` ([realm paths on hosts](realm-paths-on-hosts.md#linear-puts-the-workspace-bare-at-the-root)).
- Vercel: an Account Settings "Theme" control with system, light, and dark, seen only in search snapshots of
  dashboard pages, not in Vercel's documentation.

## The control's absence changes no page's colors

**Takeaway:**

Pages without the control still arrive in the kept scheme, from the server.

**Cited Findings:**

- `web/src/tetsu.css`'s `dark` variant follows the page's checked radio when there is one, and otherwise
  `<html data-scheme>`, which the server writes from the `theme` cookie (ADR-0070). Each realm's host keeps its own
  cookie, since each is its own site ([a host for each realm](realm-hosts.md)).
- The account area's footer holds the control and Tetsu's mark (`web/templates/settings.html`), and the login and
  join pages' footer holds the control (`crates/identity/templates/admin_page.html`).

## The menu's slide on an iPhone

**Takeaway:**

WebKit runs a popover's entry transition from `@starting-style` since Safari 17.4, but has open bugs in popover
transitions, and iOS's Reduce Motion turns off every `motion-safe:` transition. Which one stops the slide needs the
device's iOS version and motion settings.

**Cited Findings:**

- `@starting-style` and `transition-behavior` are in Safari 17.4 ([web.dev](https://web.dev/blog/baseline-entry-animations)).
- WebKit bug 275184: popovers and dialogs transition in but not out, on iOS 17.5.1
  ([WebKit Bugzilla](https://bugs.webkit.org/show_bug.cgi?id=275184)). Bug 301510: entry animations run only on the
  first open ([WebKit Bugzilla](https://bugs.webkit.org/show_bug.cgi?id=301510)). Bug 299081: popovers lose the top
  layer while closing ([WebKit Bugzilla](https://bugs.webkit.org/show_bug.cgi?id=299081)).
- Safari has no `overlay` property, which exit transitions out of the top layer need
  ([MDN](https://developer.mozilla.org/docs/Web/CSS/overlay)); Chrome's guide suggests a class set by script for
  browsers without it ([Chrome for Developers](https://developer.chrome.com/docs/modern-web-guidance/guides/ui-behaviors/animate-to-from-top-layer)).
- Reduce Motion, under Settings, Accessibility, Motion, sets `prefers-reduced-motion`
  ([WebKit](https://webkit.org/blog/7551/responsive-design-for-motion/)), which the drawer's `motion-safe:` classes
  obey (`web/templates/settings.html`).
- Chrome's device emulation does not run WebKit, so the laptop shows Chromium's behavior, not Safari's.
