# Storing admin and staff accounts for one login flow

Researched 2026-11-24 for [#111](https://github.com/heavymedl-labs/tetsu/issues/111), before its first sub-slice
makes Identity's login serve staff as well as admins. Today every Identity table points at `admins`: passkeys,
invites, ceremonies, and sessions, though `sessions` already has a `realm` column that allows only `'admin'`. Should
staff accounts share those tables with admins, or have their own? Customers are out of scope: ADR-0048 makes their
accounts and passkeys belong to one business, so they get tables of their own in any case
([multi-tenant identity patterns](multitenant-identity-patterns.md)).

## Summary

There are three shapes. **One account table for both realms, with a realm column,** is how Keycloak stores every
realm's users, and it keeps one set of login functions; its risk is that admin and staff rows sit together, so a
mistake in a function that forgets the realm reaches both. Storing the realm on every child row too, with a foreign
key on the pair `(account, realm)`, makes the database refuse a staff passkey or session that points at an admin,
which removes most of that risk. **A nullable column per realm on each shared table** (an exclusive arc) keeps
foreign keys but makes every function branch. **Separate tables per realm,** GitLab's rule for its own schema, isolates
the realms best and suits ADR-0061's plan to give admins their own database login, but copies each login function per
realm. The "type and id" columns with no foreign key are the one shape every source rejects. For Tetsu, one account
table with the realm carried in each foreign key fits best: it is what a shared login flow means in storage, and the
separate admin login ADR-0061 plans for later can be a row policy on the realm.

## Identity products keep every realm's accounts in one table

**Takeaway:**

Keycloak, where realms are fully separate login domains, keeps every realm's users as rows of one entity with a realm
id, including the master realm that holds its administrators. Isolation is enforced in its code, not by separate
tables.

**Cited Findings:**

- Keycloak's [`UserEntity`](https://www.keycloak.org/docs-api/21.0.2/javadocs/org/keycloak/models/jpa/entities/UserEntity.html)
  has a `realmId` field beside `username`, `email`, and `credentials`, so users of every realm are rows of one
  entity.
- Keycloak's [server administration guide](https://www.keycloak.org/docs/25.0.6/server_admin/index.html) says realms
  "are isolated from one another and can only manage and authenticate the users that they control", and Red Hat's
  advises keeping ordinary users out of the master realm, which is for administration
  ([Red Hat build of Keycloak: managing users](https://docs.redhat.com/en/documentation/red_hat_build_of_keycloak/26.2/html/server_administration_guide/assembly-managing-users_server_administration_guide)).
  Both were read through search summaries, not opened.
- Ory Kratos is reported to store several identity schemas in one table with a schema id, but only in community
  threads; its migration could not be read, so it is not relied on here.

## A child table pointing at one of several parents

**Takeaway:**

The shape to avoid is a pair of columns, a type and an id, with no foreign key: the database cannot check it. The
alternatives each keep foreign keys. An exclusive arc gives the child one nullable column per parent with a check
that exactly one is set; a separate table per parent duplicates the child; and a supertype, one parent table that
both kinds of account extend, keeps one child table with one key.

**Cited Findings:**

- [GitLab: polymorphic associations](https://docs.gitlab.com/development/database/polymorphic_associations/): "use
  a separate table for every type you would otherwise store in the same table", so that "we can define foreign keys
  easily".
- [Hashrocket: modeling polymorphic associations](https://hashrocket.com/blog/posts/modeling-polymorphic-associations-in-a-relational-database)
  recommends the exclusive arc on Postgres, where null columns cost little, and names its cost: every new parent adds
  a column to each child table.
- Bill Karwin's SQL Antipatterns names the type-and-id pair an antipattern and suggests a table per parent or
  reversing the reference ([O'Reilly excerpt](https://www.oreilly.com/library/view/sql-antipatterns-volume/9798888650011/f_0056.xhtml)).

## Privileged accounts are kept apart by account and by access, not by table

**Takeaway:**

Security guidance separates a person's privileged account from their everyday one, and limits who can act as an
administrator. None found asks for administrators to be stored in a different table. What matters for Tetsu is that
admin rows can be reached only by admin login, and later only by the admin realm's own database login (ADR-0061).
Either shape can give that: separate tables through grants, one table through a row policy on the realm.

**Cited Findings:**

- [UC Berkeley: privileged accounts guidelines](https://security.berkeley.edu/privileged-accounts-guidelines): a
  unique administrator account "separate from their day-to-day user account", for cloud and SaaS services too.
- [Australian Cyber Security Centre: restrict administrative privileges](https://www.cyber.gov.au/resources-business-and-government/essential-cyber-security/small-business-cyber-security/small-business-cloud-security-guide/technical-example-restrict-administrative-privileges):
  separating privileged accounts from standard ones reduces the risk of privilege escalation.
- [ADR-0061](../../adr/0061-one-service-until-domains.md): admins get their own database login when #24 lets them read
  inside businesses. [ADR-0057](../../adr/0057-operator-passkeys-before-level-2.md): admin passkey rules tighten
  before the Level 2 review, so the realms' credential rules will differ.

## How each shape fits Tetsu

| | One account table, realm in each key | Exclusive arc | Separate tables per realm |
| --- | --- | --- | --- |
| Login functions | One set, each taking the realm | One set, each branching | One set per realm |
| A staff row pointing at an admin | Refused by the `(account, realm)` key | Refused by the check | Impossible |
| A function that forgets the realm | Reaches both realms' rows | Reaches both | Reaches one |
| Admins' own database login later | Row policy on the realm | Row policy | Table grants |
| Realm rules that differ (attestation) | A check per realm | A check per column | Each table's own |
| Adding a third global realm | New realm value | A column on every child | Every table again |
| Migration from today | Rename `admins` to accounts, add realm to keys | Add `staff_id` beside `admin_id` | New tables beside |

## Recommendations for Tetsu

- **One `accounts` table for admins and staff, with a realm column,** after Keycloak. Realm details that differ, such
  as a staff member's businesses, live in tables of their own that point at it.
- **Carry the realm in every key.** Passkeys, invites, ceremonies, and sessions store the realm and reference
  `accounts (id, realm)`, and every function takes the realm it serves and matches it, so the database refuses a row
  of one realm that points at another's account.
- **Migrate, keep the data.** A new migration turns `admins` into `accounts` and adds the realm to each key, so the
  maintainer's admin survives; ADR-0085's reset was allowed only while nothing stored mattered.
- **When admins get their own database login (ADR-0061),** a row policy on the realm limits it, and a test shows a
  staff function cannot read an admin row.
- **Decide it in an ADR,** since it settles how ADR-0048's global tables are shaped.

## Gaps

- No source compared these shapes for authentication data specifically; the evidence is general schema design and
  Keycloak's model.
- Kratos's storage was not confirmed.
- Whether a row policy on the realm costs anything measurable on the session lookup, which every request makes, was
  not measured.
