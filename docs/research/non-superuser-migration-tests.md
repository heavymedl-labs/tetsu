# Migration tests as a runner that is no superuser

Researched 2026-11-26 for [#168](https://github.com/heavymedl-labs/tetsu/issues/168), which asks for a test that
migrates every module as a runner with `CREATEROLE` that is no superuser, as `tetsu_owner` is on the deploy
([production](../operations/production.md)). What is settled is not repeated here: module and lookup roles
([ADR-0042](../adr/0042-tenant-isolation.md)), and the fake module's runner test in `crates/postgres/tests/migrate.rs`.
Sources were read through search summaries on 2026-11-26.

## Summary

Projects that run migrations as a superuser in tests and as a lesser role in production find the gap the same way
Tetsu nearly did: a statement only a superuser may run passes CI and fails at deploy. GitLab added a CI job that
migrates as a role that is no superuser for this reason. Postgres 16 and later let a `CREATEROLE` role grant a
role only while it holds the ADMIN option on it, which its creator gets. Roles belong to the whole server, so a
test on a shared server either holds ADMIN on the roles already there, renames the roles it uses, or runs on a
server of its own.

## Others test migrations as the production role

**Takeaway:**

Running tests as a superuser hides permission bugs until production; the fix leaders reached is a CI job that
migrates as the production role.

**Cited Findings:**

- GitLab's test pipeline ran migrations as a superuser, unlike production, and an issue proposed the production
  role instead ([GitLab issue](https://gitlab.com/gitlab-org/database-team/gitlab-com-database-testing/-/issues/60)).
  A query needing SUPERUSER was merged, and its corrective action was a role that is no superuser in CI
  ([GitLab issue 412756](https://gitlab.com/gitlab-org/gitlab/-/issues/412756)); GitLab now has a
  `db:migrate-non-superuser` job ([GitLab Docs](https://docs.gitlab.com/development/database/dbmigrate_multi_version_upgrade_job/)).
  GitLab's role is a plain user on the shared test server, with no roles of its own to grant
  ([create_postgres_user.sh](https://gitlab.com/gitlab-org/gitlab-foss/blob/148816cd67a314f17e79c107270cc708501bdd39/scripts/create_postgres_user.sh)).
- Neon advises an owner role for migrations and a restricted role at runtime, and tests that use the same roles as
  production ([Neon guide](https://neon.com/guides/test-rls-on-neon-branches)).

## A runner grants only roles it holds ADMIN on

**Takeaway:**

Since Postgres 16, a `CREATEROLE` role may grant membership only in roles it holds the ADMIN option on; it gets
that option on the roles it creates.

**Cited Findings:**

- Grants without ADMIN fail with "Only roles with the ADMIN option on role … may grant this role", and a creator
  that is no superuser is given ADMIN on what it creates; `createrole_self_grant` can add SET and INHERIT
  ([Percona](https://www.percona.com/blog/exploring-the-improved-createrole-attribute-for-user-management-in-postgresql-16/);
  [PostgreSQL 16: CREATE ROLE](https://www.postgresql.org/docs/16/sql-createrole.html);
  [postgresqlco.nf](https://postgresqlco.nf/doc/en/param/createrole_self_grant/)).

## Roles belong to the server, so role tests share it or leave it

**Takeaway:**

A fresh database does not isolate roles. Guides keep shared servers for data tests and give tests that create,
alter, or grant roles a server of their own, at the cost of starting one.

**Cited Findings:**

- Postgres roles are cluster-wide, not per database, so a separate database does not isolate role tests
  ([PlanetScale](https://planetscale.com/blog/one-postgres-cluster-many-apps)); a guide keeps a dedicated container
  for tests that change roles or authentication and shares one otherwise, since a container per test spends its
  time booting ([QA Skills](https://qaskills.sh/blog/testcontainers-postgres-per-test-database)).

## Recommendations for #168

- **A throwaway runner on the shared test server, given ADMIN on the module roles already there.** It then holds
  what `tetsu_owner` holds on the deploy, ADMIN on the roles and SET without INHERIT on the module roles, so a
  statement on an object another role owns fails as it would there. It needs no new tooling and no change to the
  migrator.
- **Every role it makes is dropped when the test ends, and the test databases' sweep drops any a crashed run
  left,** by the ended process its name carries, so no throwaway role keeps ADMIN on the module roles.
- **Not chosen: renaming the roles for the test.** Migration files name the roles about 125 times, so the migrator
  would rewrite SQL, and the test would run text production never runs.
- **Not chosen for now: a server of its own.** Postgres runs in Docker here, with no `initdb` on the machine, so this
  needs a second container in `check.sh` and CI. It isolates best, and is the move if the shared roles ever change
  under a test.

## Gaps

- Pages were read through search summaries. Whether GitLab's job grants roles was not found; its script makes none.
- No source tested a runner holding ADMIN on roles another role created, as the throwaway would.
