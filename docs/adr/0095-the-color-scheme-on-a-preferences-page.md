# 0095. The color scheme on a Preferences page

- Status: Accepted
- Date: 2026-11-26
- Supersedes [ADR-0070](0070-a-remembered-color-scheme.md)'s control in the footer of the admin pages, which now
  include the account area and the login and join pages of every realm; its cookie and first-byte rules stand

## Context

ADR-0070 puts the color scheme control, System, Light, and Dark, in the footer of Tetsu's own pages and the admin
pages, beside Tetsu's mark. Those pages now include every realm's account area and its login, join, refused, and
unavailable pages. The maintainer asked to take the control and the mark out of those footers, and to give the
control a page of its own in the account area.

Products with an account area keep the scheme in a person's own settings, and show no control on their login pages:
GitHub's Appearance, Linear's Preferences, and Vercel's Account Settings. Linear and Slack keep the choice on each
device, as Tetsu's cookie does, while GitHub and Notion keep it on the account
([where a person picks the color scheme](../research/identity/theme-preference.md)).

## Decision

- **Each realm's account area has a Preferences page,** `/settings/preferences`, in its Personal group after
  Profile and Security, with one section, Theme: System, Light, and Dark, each named beside its icon, System first.
- **The footers of the account area and of the login, join, refused, and unavailable pages hold no control and no
  mark.** The site's own pages keep theirs, as ADR-0070 has them.
- **The choice stays on the device,** in ADR-0070's `theme` cookie, as Linear and Slack keep it, and each realm's
  host keeps its own, since each is its own site. A page without the control still arrives in the kept scheme, from
  `<html data-scheme>`.

## Alternatives considered

- **Keep the choice on the account, as GitHub does.** It follows a person across devices, but needs a column in
  each realm's account table and a write that a cookie does not, and the first byte of a logged-out page could still
  only read a cookie.
- **"Appearance" at `/settings/appearance`, as GitHub names it.** As clear for one setting, but Preferences can hold
  the next personal setting, such as a time zone, without a second page.
- **Keep the control on the login pages.** A person can pick a scheme before logging in, but none of the leaders
  read offers it there, and the account area is where the setting is found.

## Consequences

- Each realm's sidebar gains Preferences, and the session island, menu, and page shell load on it as on Profile.
- A person who picked a scheme on a login page keeps it, since the cookie stays; one who never did sees the system's
  until they choose on Preferences.
- The account area's footer is empty on wide screens, so it goes.

## Sources

- [Where a person picks the color scheme](../research/identity/theme-preference.md), 2026-11-26.
- [GitHub Docs: Managing your theme settings](https://docs.github.com/en/get-started/accessibility/managing-your-theme-settings).
- [Linear Docs: Preferences](https://linear.app/docs/account-preferences), whose theme is "not synced across
  platforms".
- [Slack Help: Use dark mode in Slack](https://slack.com/help/articles/360019434914), set on each device.
