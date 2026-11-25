# 0091. Realm SQL rendered from one source

- Status: Accepted
- Date: 2026-11-24

## Context

[ADR-0088](0088-admins-apart-from-staff.md) keeps admins and staff in separate tables, each realm with its own SQL,
and [ADR-0090](0090-a-session-table-per-realm.md) gives each its own sessions. Most of that SQL is the same login
flow over different tables: about twenty functions that start, use, and end sessions, log in, and add passkeys. A
probe showed that writing the staff copies, even with every table and column renamed, fails the repository's
duplicate check, which allows none; and copies drift when one is fixed and the other is not.

A doubt-driven review of the alternative, one set of tables and functions for both realms keyed by realm, found that
a single forgotten realm filter would let a staff session pass as an admin's, and that no admin-only database login
(ADR-0061) could be limited when every function serves both realms. Separate tables rule out both by construction.

The [research](../research/identity/account-storage-across-realms.md#one-source-for-each-realms-functions) found two
ways to write similar SQL once: generating functions in the database with `EXECUTE format(...)`, as pg_partman does,
whose bodies no linter reads; and rendering a template before a migration runs, as Flyway's placeholders and dbt's
Jinja do, which SQLFluff lints after rendering.

## Decision

- **A shared function is written once, as the admin realm's SQL,** linted and reviewed like any other.
- **Each other realm's copy is rendered from it when migrations run,** by a declared list of names to swap, such as
  `admin_sessions` to `staff_sessions`, `admins` to `staff`, `admin_id` to `staff_id`, and the realm's own name. The
  database then holds separate functions per realm, each reading only its realm's tables, and no copy exists in the
  source to drift.
- **A test fails if a rendered function still names anything of the admin realm's,** so a missed swap never points a
  realm's function at another realm's tables, and the catalog checks see every rendered function as they see the rest.
- **What differs by realm is written out, not rendered:** staff invites, owned by each business (ADR-0089), and
  joining and the role it adds. Each realm's passkey cap is a small function, as its session limits are: two for
  admins (ADR-0057) and ten for staff, and a realm it does not know may add none.
- **A store asks only for its own realm:** the port's methods take no realm from the caller, since each store names
  its realm (`LoginStore::REALM`).

## Alternatives considered

- **One set of tables and functions keyed by realm.** No duplication, but its isolation rests on every function's
  realm filter, and it leaves no way to limit an admin-only database login.
- **Copies with an exception to the duplicate check.** Simple, but it lowers a check, and nothing would notice copies
  drifting apart.
- **Generating functions in the database with `EXECUTE format(...)`.** One source too, but the generated bodies are
  strings no linter or reviewer reads as SQL.

## Consequences

- The migration runner gains a small, tested renderer, and Identity a list of swaps per realm.
- A change to a shared function changes every realm's at once, and its review covers both.
- Customers, whose accounts belong to one business (ADR-0040), will need functions of their own, rendered or not.

## Sources

- [Storing admin and staff accounts for one login flow](../research/identity/account-storage-across-realms.md),
  with its sources: pg_partman, Flyway, SQLFluff, and dbt.
