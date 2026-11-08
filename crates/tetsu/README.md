# tetsu

The binary and composition root ([ADR-0003](../../docs/adr/0003-architecture.md)). It registers every module and
names no business.

## Commands

| Command | What it does |
| --- | --- |
| `tetsu` | Prints the version and the registered modules |
| `tetsu migrate` | Runs each module's new migrations |
| `tetsu seed <file or folder>` | Creates the tenants in seed files, and skips any whose slug exists |
| `tetsu seed --check <file or folder>` | Checks seed files without a database |

A folder means its regular `.toml` files at the top level, in name order, leaving out hidden files. Seeding
checks every file before it writes anything, and writes only through public use cases. A database failure partway leaves earlier tenants in place,
and a rerun skips them.

Exit codes are 0 for success, 1 for a failure, and 2 for a command or arguments it does not understand.

## Settings

| Variable | Used by | Meaning |
| --- | --- | --- |
| `DATABASE_URL` | `migrate`, `seed` | The Postgres to use, which must store text as UTF8. Required, and may not set `sslmode` |
| `DATABASE_TLS` | `migrate`, `seed` | `require` (the default) or `disable`, for local tests only ([ADR-0013](../../docs/adr/0013-postgres-tls.md)) |

Errors never print the database URL or its password. Messages from Postgres may name the user or the database.

## Seed files

Demo businesses live in the `tetsu-demos` repository, one tenant per file
([ADR-0018](../../docs/adr/0018-demo-businesses-as-data.md)). Format 1:

```toml
format = 1

[tenant]
slug = "sample-studio"    # 1 to 63 lowercase letters, digits, and inner hyphens; reached at /sample-studio
name = "Sample Studio"    # 1 to 100 characters

[tenant.theme]
primary = "#3A4250"       # #RRGGBB
accent = "#E5622B"

[[tenant.locations]]      # any number, in display order
name = "Main Street"
time_zone = "America/Denver"
```

Unknown fields, duplicate keys, and unknown formats fail, and two files may not share a slug. `format` is read
first, so an older binary reports a newer file as newer. Every change to the format raises it, including a new
optional field, and the binary keeps reading the previous format until the demos repository moves
([ADR-0019](../../docs/adr/0019-seed-format-versions.md)).

## Hot paths

None yet. These commands run once per deploy or demo change.
