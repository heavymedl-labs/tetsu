# 0088. Admins apart from staff, one login flow in code

- Status: Accepted
- Date: 2026-11-24

## Context

[ADR-0048](0048-sign-in-design.md) makes staff identities, their passkeys, admins, and sessions global tables, but
not whether admins and staff share them. Identity holds admins alone today: every table points at `admins`, though
`sessions` already has a `realm` column. [#111](https://github.com/heavymedl-labs/tetsu/issues/111) adds staff, and
the maintainer wants one login flow for every realm, with one "Log in" button
([ADR-0087](0087-one-log-in-button.md)).

The [research](../research/identity/account-storage-across-realms.md) found that industry leaders keep the people
who work for the platform apart from the people it serves: Microsoft puts customers in a tenant "distinct and
separate" from the workforce one, Okta sells the two as separate products, and Stripe's employees log in through
company SSO and reach a business's Dashboard only through an access request the business approves. Within customer
identity, one store with a realm column is normal, as Keycloak's is. Schema guidance rejects a type-and-id pair
without foreign keys. Admins will also differ from staff in ways that need their own place: stricter passkeys before
the Level 2 review ([ADR-0057](0057-operator-passkeys-before-level-2.md)), and their own database login once they
may read inside businesses ([ADR-0061](0061-one-service-until-domains.md)).

## Decision

- **Admins are Tetsu's workforce and keep their own tables:** `admins` and its passkeys, invites, and ceremonies.
  Nothing a staff member or customer does reads or writes them.
- **Staff get tables of their own,** global as ADR-0048 has them, with each business they work for and their role
  there in tables that point at them. Customers get theirs, per business, in their own slice.
- **The login flow is shared in code, not in storage.** One login and join template, passkey script, ceremony logic,
  session handling, and account area serve every realm. Each realm has its own store behind the same port, and its
  own SQL functions, even where they repeat another realm's.
- **Every reference has a real foreign key.** A row shared across realms, such as a session, has one nullable key per
  realm and a check that exactly one is set and matches its realm, never a type-and-id pair.
- **ADR-0087's remembered browser is unchanged.** It chooses which login page `/login` shows, not where the account
  is stored.
- **A move to a company SSO for admins,** as Stripe's employees use, stays open for later; ADR-0040's passkey login
  serves admins now.

## Alternatives considered

- **One `accounts` table for admins and staff, with the realm in every key.** One set of SQL functions, and
  Keycloak's shape, but it puts Tetsu's workforce in the store its customers use, which no leader found does, and a
  function that forgets the realm reaches admin rows.
- **A nullable `staff_id` beside `admin_id` on every admin table.** Keeps foreign keys without new tables, but mixes
  the realms in each table and makes every function branch.

## Consequences

- #111's first sub-slice makes Identity's login code realm-generic over a store per realm, with admin behavior
  unchanged and admin tables untouched; the next adds staff tables in a new migration, so no data is reset.
- Some SQL repeats per realm. A change to one realm's login function is checked against the others' by the shared
  tests each realm runs.
- ADR-0061's admin database login comes from grants on the admin tables when #24 needs it.

## Sources

- [Storing admin and staff accounts for one login flow](../research/identity/account-storage-across-realms.md), with
  its sources: Microsoft Entra's tenant configurations, Okta's IAM overview, Stripe's security and Dashboard access
  pages, Shopify's collaborator accounts, Keycloak's user model, and GitLab's and Hashrocket's guidance on
  polymorphic keys.
