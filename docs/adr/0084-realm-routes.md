# 0084. Realm routes

- Status: Accepted
- Date: 2026-11-23
- Supersedes [ADR-0061](0061-one-service-until-domains.md)'s `/ops` as the operators' path

## Context

[ADR-0061](0061-one-service-until-domains.md) serves every realm from one service, each under its own reserved path,
and names `/ops` for operators. The paths under it, and the staff and customer paths, come from the account area's
research ([Account area and dashboards](../research/identity/account-area-and-dashboards.md#one-account-area-the-same-in-every-realm)),
which offered them as suggestions. #156 built the first of them: `/ops/account/profile` and `/ops/account/security`.

The maintainer asked for the pattern industry products use, so that a person can guess an address. Products differ by
who the person is, not by one pattern repeated:

- **A platform's own back office** lives at `/admin`: Django's admin, WordPress's `/wp-admin`, and Discourse. Larger
  platforms move it to its own host, as Shopify does with `admin.shopify.com`. `ops` appears in few public addresses.
- **A tool used across several organizations** keeps personal pages apart from each organization's: GitHub's
  `/settings/profile` and `/settings/security` beside each organization's own settings, Vercel's `/account` beside
  each team, and Linear's `/<workspace>/settings/account/...` beside its workspace settings.
- **A shop's customer** has an account under the shop's own address: a Shopify store's classic `/account`, and now
  a branded `account.<store domain>` ([Passkey domains](../research/passkey-domains.md)).

Tools say "settings" for personal pages; shops and consumer services say "account". Section names are nearly always
`profile` and `security`.

Two of Tetsu's own rules shape where each realm can go. A business's public booking page is `/<business>`, so staff
work cannot sit at that address. A staff account spans businesses, while a customer account belongs to one
([ADR-0040](0040-identity-and-sign-in.md)).

## Decision

Each realm takes the address pattern that is standard for its kind of user:

| Realm | Home | Personal pages | Other pages |
| --- | --- | --- | --- |
| Operators | `/admin` | `/admin/settings/profile`, `/admin/settings/security` | `/admin/mailbox`, `/admin/login`, `/admin/join`, and later `/admin/businesses` |
| Staff | `/app/<business>` | `/app/settings/profile`, `/app/settings/security` | `/app/<business>/settings/...` for the business |
| Customers | `/<business>/account` | `/<business>/account/profile`, `/<business>/account/security` | none |

- **Operators move from `/ops` to `/admin` now,** with every page and request under it. The rows for staff and
  customers are the intended map, and each realm's sign-in slice ([ADR-0048](0048-sign-in-design.md)) settles its
  own, including whether a staff address must name its business.
- **Personal pages share their templates in every realm.** Only the addresses differ: "settings" where people work,
  "account" where they book.
- **A realm's path stays reserved** as a business name ([#90](https://github.com/heavymedl-labs/tetsu/issues/90)).
  `admin`, `app`, and `account` are already reserved; `ops` stays reserved too, so no business takes an address
  operators used.
- **Until operators have a home** ([#157](https://github.com/heavymedl-labs/tetsu/issues/157)), `/admin` sends them to
  Profile, where logging in lands. Their sidebar carries a Platform group of pages only operators have, starting with
  the mailbox, and no "Back to app" while there is no app to go back to. What the home shows stays with #157.

## Alternatives considered

- **Keep `/ops` and `/ops/account/...`.** Already built, and short. It lost because `ops` is internal jargon a person
  would not guess, and `account` under a work tool departs from what GitHub, Linear, and Stripe teach.
- **One pattern for every realm,** such as `/<realm>/account/...`. Simple to explain, but it puts staff and customers
  in shapes neither recognizes, and a customer's account would not sit under the business it belongs to.
- **Flat personal pages, such as `/admin/profile`.** Shorter, but it mixes personal pages with the realm's work pages,
  which the settings shell keeps apart, and leaves no home for business settings.
- **A separate host for each realm now.** The industry's end state, but Tetsu has no domains until
  [#8](https://github.com/heavymedl-labs/tetsu/issues/8) and [#102](https://github.com/heavymedl-labs/tetsu/issues/102).
  ADR-0061 keeps hosts and paths as settings, so `/admin` and `/app` become `admin.` and `app.` hosts by
  configuration later.

## Consequences

- Operators' pages, requests, and the return-to-page rule after login move to `/admin`, and invite links follow. The
  session cookie, `__Host-tetsu-operator` on path `/`, and the origin check do not change. Nothing is deployed under
  the new paths yet, so no saved link breaks for anyone but the maintainer.
- Docs that name operator paths change with the code: the READMEs, the security testing guide, and the research's
  suggested URLs, which point here.
- The staff and customer slices start from this map rather than choosing again.

## Sources

- [Django: The Django admin site](https://docs.djangoproject.com/en/stable/ref/contrib/admin/), mounted at `admin/`
  in Django's own tutorial.
- [WordPress: Administration Screens](https://wordpress.org/documentation/article/administration-screens/), under
  `/wp-admin`.
- [Shopify Help Center: Shopify admin](https://help.shopify.com/en/manual/shopify-admin), at `admin.shopify.com`, and
  [Customer accounts](https://help.shopify.com/en/manual/customers/customer-accounts), under the store's own
  address.
- [GitHub Docs: Managing your personal account](https://docs.github.com/en/account-and-profile), whose pages live at
  `github.com/settings/profile` and `github.com/settings/security`.
- Linear, Vercel, and Stripe, from the research's
  [comparison of peers](../research/identity/account-area-and-dashboards.md#peers-agree-the-avatar-holds-me-the-switcher-holds-us).
