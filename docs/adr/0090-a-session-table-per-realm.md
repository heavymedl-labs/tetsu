# 0090. A session table per realm

- Status: Accepted
- Date: 2026-11-24
- Supersedes [ADR-0089](0089-staff-join-and-login.md)'s rule that sessions stay one table with a nullable key per
  realm

## Context

[ADR-0089](0089-staff-join-and-login.md) kept one `sessions` table for admins and staff, with an `admin_id` or a
`staff_id` and a check that one is set. Building it would widen the table's realm check, which only dropping and
adding it again can do, and the migration lint bans that without the maintainer's exception, and it would change the
admins' live table. The [research](../research/identity/account-storage-across-realms.md#sessions-follow-their-accounts)
found that sessions go where their accounts go: Keycloak keeps one table of sessions beside one table of accounts,
told apart by realm, while platforms that keep their workforce apart keep its sessions apart too. Admin and staff
accounts are in separate tables ([ADR-0088](0088-admins-apart-from-staff.md)). The maintainer asked that the two
realms' session tables be named alike.

## Decision

- **Each realm keeps its sessions in its own table:** `admin_sessions` for admins and `staff_sessions` for staff,
  each keyed to its own account table, with the same shape and limits as ADR-0048 sets for its realm.
- **The admins' table is renamed `admin_sessions` by editing its migrations in place,** as
  [ADR-0085](0085-admins-not-operators.md) allowed while nothing stored is needed, so no migration rewrites every
  function that names it. Identity, Audit, and Mail are reset once on each environment, locally and on the demo.

## Alternatives considered

- **One table with a key per realm (ADR-0089).** One place for every session, but each realm's functions are their
  own already, and it needs a lint exception and changes the admins' live table.
- **Keep the admins' table named `sessions` beside `staff_sessions`.** No reset, but the names would not say whose
  sessions each holds, which the maintainer asked for.
- **A migration that renames the table.** Keeps the data, but must recreate every admin function that names it,
  since Postgres keeps a function's body as text, for data no one needs.

## Consequences

- The admins' sessions table and every function and test that names it say `admin_sessions`, and no behavior changes.
- Each environment resets Identity, Audit, and Mail once more; the maintainer recreates their admin and the demo's
  deploy resets its schemas, as after ADR-0085.
- Once a real business's data is held, a rename takes a new migration.

## Sources

- [Storing admin and staff accounts for one login flow](../research/identity/account-storage-across-realms.md), its
  section on sessions, with its sources: Keycloak, GitLab, and OWASP.
