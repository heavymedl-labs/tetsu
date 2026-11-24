# 0085. Admins, not operators

- Status: Accepted
- Date: 2026-11-23
- Supersedes the term "operator" in [ADR-0040](0040-identity-and-sign-in.md) and the ADRs that follow it: the people
  who work for Tetsu itself are admins

## Context

The ADRs and code call the people who work for Tetsu itself, not for a business, operators, and their realm `ops`.
[ADR-0084](0084-realm-routes.md) moved that realm's pages to `/admin`, the address a person would guess for a
platform's back office. The pages now say one word and the code another: a page under `/admin` is served by
`Operators`, checked against `Realm::Operator`, and stored in `identity.operators`. The maintainer asked for the code
to say what the product says.

The word is in stored data as well as code: events such as `identity.operator_created` with fields such as
`operator_id`, in the outbox and the audit record, and actors such as `operator-7@Firefox on Linux`. Passkeys already
on devices carry the label "Tetsu operator · Name", which only the device holds. Tetsu holds no data anyone needs yet:
the maintainer's own account, and the demo's, which its deploy can rebuild.

## Decision

- **"Admin" replaces "operator"** in code, the database, templates, scripts, the CLI (`tetsu admin create`), and
  current docs, and "ops" goes with it (`ops_page.html`, `src/*/ops.ts`). There is no alias for the old CLI command.
- **The migrations say "admin" where they said "operator",** as written in place, and the schemas they built are
  rebuilt with `tetsu reset`: Identity, Audit, and Mail, locally and on the demo. So no table, column, function,
  event, or record keeps the old word, and no code reads two names. This is allowed only because nothing stored is
  needed yet; once a real business's data is held, a rename takes a new migration.
- **The session cookie becomes `__Host-tetsu-admin`,** which logs everyone out once.
- **New passkeys are labelled "Tetsu admin · Name".** Passkeys already made keep their label until replaced.
- **ADRs and research keep their wording,** as records of what was decided and found. The glossary says that older
  ADRs call admins operators.

## Alternatives considered

- **Keep "operator" in code and say "admin" only on pages.** Nothing stored changes, but every reader translates
  between the two words, and new code would follow whichever it read last.
- **A new migration that renames each object, keeping the data.** Keeps the maintainer's account and the audit
  history, but the audit record is append-only, so its history would keep the old names, and the dispatcher would
  read both, for data no one needs.
- **Keep the stored names as a frozen format** (`operator_id`, `identity.operator_created`). Nothing stored changes,
  but "operator" would stay in the code for good.

## Consequences

- One pass renames Identity, the CLI, the web scripts and templates, and the docs, and changes no behavior.
- Each environment's Identity, Audit, and Mail schemas are reset once: the maintainer recreates their admin with
  `tetsu admin create`, and the demo's deploy resets its schemas. The audit history before the reset is gone.
- Open issues that say "operator" are updated with the code.

## Sources

- [ADR-0084](0084-realm-routes.md), for `/admin`, and its sources for the industry's word.
- [ADR-0040](0040-identity-and-sign-in.md), which named the realm.
