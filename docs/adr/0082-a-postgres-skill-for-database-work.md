# 0082. A Postgres skill for database work

- Status: Accepted
- Date: 2026-11-22

## Context

Tetsu's database is Tier 1: it holds the tenant isolation every business relies on, and its schema, migrations,
and locking get expensive to change once real data exists. No contributor has Postgres expertise, so database
design and review lean on research passes such as the [database design review](../research/database-design-review.md),
done once rather than on every change. The duties already name [agent skills](../research/agent-skills.md) for
other kinds of work; none covers databases.

The [evaluation](../research/database-agent-skills.md) compared the two skills from Postgres companies. Supabase's
`supabase-postgres-best-practices` is 31 plain-text rules for Postgres running anywhere, and caught a known weak
spot in Tetsu independently. Neon's `neon-postgres` is a platform guide that acts on the account and fetches more
skills, against the repository's rules.

## Decision

- **Database work uses Supabase's `supabase-postgres-best-practices`,** from
  [supabase/agent-skills](https://github.com/supabase/agent-skills), installed as the duties' skills are and pinned
  to a reviewed commit, `c9be0e9` today. It applies when writing or reviewing SQL, migrations, schema, indexes,
  row-level security, or connection settings.
- **The fresh review names it** for any change to SQL or migrations, beside the code review skill, and asks the
  reviewer to check the change against its rules.
- **The repository wins where they disagree.** An ADR, the invariants, or `CONSTRAINTS.md` overrides a rule, and
  Supabase-specific notes do not apply. Its primary-key rule is out of date: Postgres 18 builds in `uuidv7()`.
- **Neon's skills are not adopted.** Neon's documentation stays the source for hosting details, such as pooled
  versus direct connections.
- **Moving to a newer commit means reading what changed first,** as for the duties' skills.
- **It does not replace a human Postgres review,** which the Level 2 review before real data
  ([#83](https://github.com/heavymedl-labs/tetsu/issues/83)) includes.

## Alternatives considered

- **No skill, research passes alone.** They are thorough but rare, so most changes get no Postgres expertise.
- **Neon's skill too.** It fits the host, but it acts on the account, fetches skills unreviewed, and is aimed at
  TypeScript apps.
- **A community skill.** Directories list several, of uneven quality and maintenance, with no company behind them.
- **Write Tetsu's own rules.** They would fit exactly, but would need the expertise the skill exists to supply.

## Consequences

- Every database change is checked against an expert's rules, not only once in a while.
- Contributors install one more skill, and the review prompt grows by a line.
- The weakest evidence is a single maintainer and 534 open issues and pull requests; if the skill stops being
  maintained or drifts toward Supabase's products, revisit.

## Sources

- [Supabase: Postgres best practices for AI agents](https://supabase.com/blog/postgres-best-practices-for-ai-agents)
- [supabase/agent-skills](https://github.com/supabase/agent-skills), read at commit `c9be0e9`
- [neondatabase/agent-skills](https://github.com/neondatabase/agent-skills), read at commit `d3601d9`
- [Neon: Agent Skills](https://neon.com/docs/ai/agent-skills)
