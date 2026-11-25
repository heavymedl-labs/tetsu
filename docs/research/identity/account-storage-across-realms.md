# Storing admin and staff accounts for one login flow

Researched 2026-11-24 for [#111](https://github.com/heavymedl-labs/tetsu/issues/111), and extended the same day when
the maintainer asked what industry leaders do, before its first sub-slice
makes Identity's login serve staff as well as admins. Today every Identity table points at `admins`: passkeys,
invites, ceremonies, and sessions, though `sessions` already has a `realm` column that allows only `'admin'`. Should
staff accounts share those tables with admins, or have their own? Customers are out of scope: ADR-0048 makes their
accounts and passkeys belong to one business, so they get tables of their own in any case
([multi-tenant identity patterns](multitenant-identity-patterns.md)).

## Summary

Industry leaders keep the people who work for the platform apart from the people who use it. Identity vendors split
**workforce identity**, a company's own staff, from **customer identity**, everyone its product serves, into separate
products and tenants: Microsoft puts customers in an external tenant "distinct and separate" from the workforce one.
Stripe's employees log in through company SSO with hardware keys on Stripe-issued machines, and reach a business's
Dashboard only through an access request the business approves. Tetsu's admins are its workforce, and staff and
customers are its customer identity, so leaders would keep admins out of the tables staff use. Within customer
identity, one account store with a type or realm is normal: Keycloak keeps every realm's users in one table. The
schema guidance agrees that each realm's rows need real foreign keys, never a type-and-id pair. For Tetsu that means
admins keep their own tables, staff get tables of their own, and the login flow, templates, and script are shared in
code rather than in storage. A later choice, not needed now, is whether admins move to a company SSO, as Stripe's
employees use.

## Leaders separate workforce identity from customer identity

**Takeaway:**

Every source draws the same line: employees and the platform's own tools on one side, customers and the businesses
it serves on the other, in separate tenants or separate products, with employees reaching a customer's account only
through an access the customer grants and that is logged.

**Cited Findings:**

- [Microsoft Entra: tenant configurations](https://learn.microsoft.com/en-us/entra/external-id/tenant-configurations):
  an external tenant, for apps published to consumers and business customers, is "distinct and separate from your
  workforce tenant", where employees and internal apps live.
- [Okta: IAM overview](https://developer.okta.com/docs/concepts/iam-overview) describes customer identity users as
  people who create their own identities and sign in from many places, unlike workforce users; Okta sells the two as
  separate products, Workforce Identity and Customer Identity.
- [Stripe: security](https://docs.stripe.com/security): employees use SSO and two-factor authentication with a
  hardware token, with mTLS on Stripe-issued machines, and systems outside an employee's standard work need further
  permission.
- [Stripe: grant Stripe employees access to your Dashboard](https://support.stripe.com/questions/grant-stripe-employees-access-to-your-dashboard):
  employees "may request temporary access to view your Dashboard by providing you with an access request link to
  approve", and every such access is logged.
- [Shopify: collaborator accounts](https://help.shopify.com/en/manual/your-account/staff-accounts/collaborator-accounts):
  outside partners reach a store only through a request its owner approves, on a track separate from staff accounts.

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

## Sessions follow their accounts

**Takeaway:**

Added 2026-11-24, when building staff sessions asked whether they share the admins' table. Where one store keeps every
realm's accounts, it keeps every realm's sessions beside them, told apart by a realm column: Keycloak's sessions
carry a `REALM_ID`, and its cleanup deletes by realm. Where a platform keeps its workforce apart, as the leaders above
do, the workforce's sessions live in that separate system too, and GitLab, which keeps admins in its one user table,
raises an admin's own session with "Admin Mode" rather than a second store. So sessions go where their accounts go:
with separate admin and staff tables (ADR-0088), a table per realm keeps every session's key a real foreign key to
its own account table, as GitLab's rule for polymorphic links asks, where one shared table needs a nullable key per
realm and a check that one is set.

**Cited Findings:**

- Keycloak keeps online and offline sessions in the same tables with a flag, and its purge deletes from
  `OFFLINE_USER_SESSION` by `REALM_ID`, according to a Keycloak forum post and a keycloak-dev thread rather than its
  documentation ([Keycloak forum](https://forum.keycloak.org/t/technical-documentation-on-how-the-user-session-clearing-process-is-implemented/29652);
  [keycloak-dev](https://groups.google.com/g/keycloak-dev/c/ujWCcOE-GyI)).
- GitLab's Admin Mode raises an administrator's own session for six hours, regenerating its ID on entry, and GitLab
  Dedicated for Government recommends dedicated admin accounts or Admin Mode
  ([GitLab: sign-in restrictions](https://docs.gitlab.com/administration/settings/sign_in_restrictions/);
  [GitLab: authentication development guidelines](https://docs.gitlab.com/development/authentication/);
  [GitLab Dedicated for Government](https://docs.gitlab.com/security/dedicated_for_government_secure_config_guide/)).
- [GitLab: polymorphic associations](https://docs.gitlab.com/development/database/polymorphic_associations/): "use
  a separate table for every type you would otherwise store in the same table".
- No OWASP guidance found asks for privileged sessions in a separate store; the
  [Session Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html)
  was read through a search summary.

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
| Workforce apart from customers, as leaders do | No | No | Yes |

## Recommendations for Tetsu

- **Keep admins apart in storage,** as leaders keep workforce identity apart: `admins` and its passkeys, invites, and
  ceremonies stay admin tables. ADR-0061's admin database login then comes from table grants, and ADR-0057's stricter
  admin passkeys from the admin tables' own checks.
- **Give staff tables of their own,** with their businesses and roles in tables that point at them, as ADR-0048 has
  it. Customers get theirs per business later.
- **Share the flow in code, not in storage.** One login template, passkey script, ceremony logic, and account area
  serve every realm; each realm has its own store behind the same port, and its SQL functions are its own.
- **Sessions can stay one table,** since its `realm` column was built for it, with a nullable key per realm and a check
  that exactly one is set and matches the realm (an exclusive arc), or each realm can keep its own; the slice's plan
  decides.
- **Admins reach inside a business only by a grant the business gives,** logged, as Stripe and Shopify do; that is
  [#24](https://github.com/heavymedl-labs/tetsu/issues/24)'s decision.
- **Decide it in an ADR,** since it settles how ADR-0048's global tables are shaped.

## Gaps

- No source compared these shapes for authentication data specifically; the evidence is general schema design,
  Keycloak's model, and how leaders separate their workforce.
- Shopify's and Stripe's internal storage is not public; what is public is that their employees log in apart from
  their customers.
- Kratos's storage was not confirmed.
- Whether a row policy on the realm costs anything measurable on the session lookup, which every request makes, was
  not measured.
