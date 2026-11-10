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
| `tetsu serve` | Serves the public pages until SIGTERM or SIGINT, then finishes open requests for up to 10 seconds. Refuses to start before `migrate`, or while a tenant holds an address now reserved for Tetsu's own pages |

A folder means its `.toml` files at the top level, in name order, leaving out hidden files. One that is not a
regular file fails the run. Seeding
checks every file before it writes anything, and writes only through public use cases. A database failure partway leaves earlier tenants in place,
and a rerun skips them.

Exit codes are 0 for success, 1 for a failure, and 2 for a command or arguments it does not understand.

## Pages and styles

Pages are Askama templates: the site's in `templates/`, each module's in its own crate, and the shell and shared
parts every page extends in `web/templates/`. Vite builds `web/src/tetsu.css` with Tailwind into
`assets/build/`, which is committed and compiled into the binary, and `serve` gives each built file at the path
Vite's manifest names, under `/assets/`, kept for a year ([ADR-0034](../../docs/adr/0034-front-end-build.md)).

```bash
npm run build   # what check.sh compares, and what is committed
npm run watch   # rebuilds as templates change; restart it after adding one, then rebuild and restart tetsu
```

The watcher keeps classes a template no longer uses, so run `npm run build` before committing.

## Settings

| Variable | Used by | Meaning |
| --- | --- | --- |
| `DATABASE_URL` | `migrate`, `seed`, `serve` | The Postgres to use, which must store text as UTF8. Required, and may not set `sslmode` |
| `DATABASE_TLS` | `migrate`, `seed`, `serve` | `require` (the default) or `disable`, for local tests only ([ADR-0013](../../docs/adr/0013-postgres-tls.md)) |
| `DATABASE_CA_FILE` | `migrate`, `seed`, `serve` | Optional: a PEM file of certificate authorities, such as a provider's own, trusted instead of the public ones. Refused with `DATABASE_TLS=disable` |
| `TETSU_DEMO` | `serve` | `true` or `false`, with no default: whether every page says its business is a demo and asks not to be indexed ([ADR-0018](../../docs/adr/0018-demo-businesses-as-data.md)) |
| `PORT` | `serve` | The port a host such as Render sets, listened on at every interface. Without it, `serve` listens on `127.0.0.1:8080` |

TLS always verifies the certificate: it must chain to a public authority, or only to one in `DATABASE_CA_FILE` when
that is set, and name the host in the URL. Settings are checked before any connection, so a missing or unusable CA file fails at
start.

Errors never print the database URL or its password, and a URL with an unencoded `@` in its password is refused,
so write it as `%40`. Messages from Postgres may name the user or the database.

## Seed files

Demo businesses live in the `tetsu-demos` repository, one tenant per file
([ADR-0018](../../docs/adr/0018-demo-businesses-as-data.md)). Format 1:

```toml
format = 1

[tenant]
slug = "sample-studio"    # 1 to 63 lowercase letters, digits, and inner hyphens, not a reserved name such as assets; address /sample-studio
name = "Sample Studio"    # 1 to 100 characters

[tenant.theme]
primary = "#3A4250"       # #RRGGBB
accent = "#E5622B"

[[tenant.locations]]      # any number, in display order
name = "Main Street"      # 1 to 100 characters
time_zone = "America/Denver"  # 1 to 100 characters
```

Unknown fields, duplicate keys, and unknown formats fail, and two files may not share a slug. `format` is read
first, so an older binary reports a newer file as newer. Every change to the format raises it, including a new
optional field, and the binary keeps reading the previous format until the demos repository moves
([ADR-0019](../../docs/adr/0019-seed-format-versions.md)).

## Hot paths

None yet. These commands run once per deploy or demo change.
