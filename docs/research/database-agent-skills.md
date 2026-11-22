# Database agent skills

An evaluation, under [ADR-0010](../adr/0010-evaluating-technology.md), of agent skills that bring Postgres
expertise to database design and review, read on 2026-11-22. The maintainer asked for it: Tetsu's database work
needs expertise no contributor has yet. This is research, not a decision; where it disagrees with an ADR, the ADR
wins. A skill is cheap to drop and sits on no hot path, so steps 1, 4, and 7 apply, with a small trial added.

## Criteria

Set before reading either skill. It must:

- bring expert knowledge Tetsu lacks: schema, indexes, migrations, locking, connections, and row-level security
- fit plain Postgres 18 on Neon, reached from Rust through hand-written SQL, with no Supabase or ORM
- be plain text any contributor and any agent can read, with nothing that runs or fetches on its own
- give advice the repository's ADRs can override without editing the skill, as with
  [the duties' skills](agent-skills.md)
- carry an open license and show active maintenance

## Options

Directories list many community Postgres skills of uneven quality and upkeep. Two come from Postgres companies:

- **[supabase/agent-skills](https://github.com/supabase/agent-skills)**, skill `supabase-postgres-best-practices`,
  read at commit `c9be0e9`: 31 rules in 8 categories (query performance, connections, security and row-level
  security, schema, locking, data access, monitoring, advanced features), about 7,100 words, each rule a short
  explanation with wrong and right SQL. Its description says it applies to Postgres running anywhere.
- **[neondatabase/agent-skills](https://github.com/neondatabase/agent-skills)**, skill `neon-postgres`, read at
  commit `d3601d9`: a guide to Neon's platform (pooled versus direct connections, branching, scale-to-zero, egress,
  search), about 8,100 words of references.

## Project health

| Signal | Supabase | Neon |
| --- | --- | --- |
| License | MIT | Apache-2.0 |
| Security advisories | None | None |
| Latest release | v0.1.9, 2026-09-28 | No releases |
| Last push | 2026-11-12 | 2026-11-20 |
| Contributors | 11, one holding 70 of 96 commits | 18, one holding 197 of 332 |
| Open issues and pull requests | 534 | 43 |
| Stars and forks | About 3,100 and 270 | About 120 and 26 |

Both skill folders hold only Markdown: no scripts. Installs and downloads say little about quality.

## What each skill is

**Supabase's is a reviewer's checklist.** Most rules hold for any Postgres. Fourteen of its files mention Supabase,
mostly in notes beside general advice (its `auth.uid()` in the row-level security examples, its pooler in the
connection rules), which Tetsu reads past.

- **One rule is out of date for Postgres 18:** the primary-key rule says time-ordered UUIDs need the `pg_uuidv7`
  extension, while Postgres 18 has `uuidv7()` built in, which Tetsu already uses.
- **One rule matches Tetsu's design:** wrapping the row-level security lookup in `(SELECT ...)` so it runs once a
  query, which Tetsu's tenant rule already does.

**Neon's is an operations guide that acts.** It is written mostly for TypeScript apps (drivers, Drizzle, Vercel and
Cloudflare notes), and it tells the agent to list organizations, create projects, write `DATABASE_URL` into `.env`,
and fetch or install further skills from the web. That conflicts with the repository's rules: settings come from
the environment or a local `.env` a person writes, and nothing is fetched or installed unreviewed. Its table of
pooled versus direct connections is useful, and Neon's own documentation holds it.

## Trial

Two of Supabase's rules were tried against Tetsu on 2026-11-22.

- **Foreign keys need indexes** (`schema-foreign-key-indexes`): its detection query, tightened to an index's leading
  column, found none missing among the 8 foreign keys across 7 tables of a migrated database.
- **Keep transactions short** (`lock-short-transactions`): its "long transaction with external calls" is the
  dispatcher's open transaction across consumer work, which the [database design review](database-design-review.md)
  found and [#160](https://github.com/heavymedl-labs/tetsu/issues/160) tracks. The rule would have caught it independently.

## Weighing it

Supabase's skill meets every criterion: expert, mostly portable rules in plain text, MIT, maintained by a Postgres
company. Its weakest points are one stale rule, Supabase-specific notes to read past, a large open-issue count, and
a single main maintainer. Neon's fails the plain-text and no-action criteria and duplicates Neon's documentation.

A skill is a checklist, not a reviewer. It raises the floor of every database change, but it does not replace a
human Postgres review before real data, which the [Level 2 review](https://github.com/heavymedl-labs/tetsu/issues/83)
should include.
