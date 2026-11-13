# tetsu

The binary and composition root ([ADR-0003](../../docs/adr/0003-architecture.md)). It registers every module and
names no business that uses Tetsu. Its root page names fictional customers only in a demo
([ADR-0037](../../docs/adr/0037-fictional-customers-on-tetsus-page.md)).

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

## Logs

`serve` writes JSON lines to standard output ([ADR-0045](../../docs/adr/0045-operations-baseline.md)), and
exports the same lines over OTLP when an endpoint is set ([ADR-0049](../../docs/adr/0049-log-export.md);
[settings](#settings)). Each request logs one `served` line with its status and milliseconds, and its `span` holds
the request ID, realm, `http.request.method` (`OTHER` for one HTTP does not define), `http.route` such as
`/{address}`, and tenant ID once a module finds one. Only those span fields are exported. Tetsu's own crates log
from info up, and other crates only warnings and errors, since no one has checked what they write; the exporter's
own crates are never exported, and their warnings, such as a refused key, stay on standard output. No line names
the path, query, or body, since those could carry a name, an email, a code, or a token. What a module reports for
the operator, such as a page that could not load, is an `ERROR` line in the request's span. A line that raises an
alert has an `alert` field, with the values ADR-0049's table lists. Other commands' output stays plain text.

## Pages and styles

Pages are Askama templates: the site's in `templates/`, each module's in its own crate, and the shell and shared
components every page uses in `web/templates/`, adapted from MIT sources and compared with Tailwind Plus
([ADR-0035](../../docs/adr/0035-component-sources-and-look.md); credits in
[THIRD-PARTY-NOTICES.md](../../THIRD-PARTY-NOTICES.md)). Light and dark follow the visitor's system setting, or the header's control for one page view
([ADR-0038](../../docs/adr/0038-color-scheme-control.md)). Vite builds `web/src/tetsu.css` with Tailwind into
`assets/build/`, which is not in git but compiled into the binary, and `serve` gives each built file at the path
Vite's manifest names, under `/assets/`, kept for a year, and the touch icon at `/favicon.ico` too, kept a day ([ADR-0034](../../docs/adr/0034-front-end-build.md),
[ADR-0036](../../docs/adr/0036-build-assets-where-used.md)). A fresh clone builds it before Cargo can compile the
crate; `scripts/check.sh` and the image build make it themselves.

The site's templates:

| Path | Holds |
| --- | --- |
| `site.html` | The head tags, header, and footer of Tetsu's own pages |
| `home.html`, `home/` | The root page, which lists its sections, one file each |
| `home/panels/` | Each product picture on the root page |
| `home/tour.html` | A slide of the root page's product tour |
| `art/` | The wordmark and its booked slots (`brand.html`, shared with the hero board), the hero board, the figures, bands, and spot maps, drawn from rows of data where they can be, and the parts of the product pictures (`app.html`) |

Every component, such as a button, a section, its heading, a group's tabs, or a footer column, is a macro in
`web/templates/components.html`, which every crate imports ([ADR-0035](../../docs/adr/0035-component-sources-and-look.md)).
What stays here only draws or tells Tetsu's own story.

`web/src/tetsu.css` holds the tokens and imports one file per feature, with any motion beside its feature:
`motion.css`, `figures.css`, `app.css`, `stories.css`, and `groups.css`.

```bash
npm ci --ignore-scripts --include=dev   # once, and after the lockfile changes
npm run build                           # before cargo build or cargo run
npm run watch                           # rebuilds as templates change; restart it after adding one
```

## Run locally

Once, with the test Postgres from [AGENTS.md](../../AGENTS.md#checks) running:

```bash
cp .env.example .env
psql postgres://postgres@localhost:54329/postgres -c 'CREATE DATABASE tetsu_local'
cargo run -- migrate
cargo run -- seed crates/tetsu/tests/fixtures/sample.toml
```

Then `cargo run -- serve` serves `http://localhost:8080` until Ctrl+C. Settings come from `.env` for any the
environment leaves unset ([ADR-0050](../../docs/adr/0050-local-settings-file.md)), so no command needs them typed.
Stop the server when you are done.

## Settings

| Variable | Used by | Meaning |
| --- | --- | --- |
| `DATABASE_URL` | `migrate`, `seed`, `serve` | The Postgres to use, which must store text as UTF8. Required, and may not set `sslmode` |
| `DATABASE_TLS` | `migrate`, `seed`, `serve` | `require` (the default) or `disable`, for local tests only ([ADR-0013](../../docs/adr/0013-postgres-tls.md)) |
| `DATABASE_CA_FILE` | `migrate`, `seed`, `serve` | Optional: a PEM file of certificate authorities, such as a provider's own, trusted instead of the public ones. Refused with `DATABASE_TLS=disable` |
| `TETSU_DEMO` | `serve` | `true` or `false`, with no default: whether every page says its business is a demo and asks not to be indexed, and whether the root page shows its fictional customers ([ADR-0018](../../docs/adr/0018-demo-businesses-as-data.md), [ADR-0037](../../docs/adr/0037-fictional-customers-on-tetsus-page.md)) |
| `PORT` | `serve` | The port a host such as Render sets, listened on at every interface. Without it, `serve` listens on `127.0.0.1:8080` |
| `OTEL_EXPORTER_OTLP_LOGS_ENDPOINT`, or `OTEL_EXPORTER_OTLP_ENDPOINT` with `/v1/logs` added | `serve` | Optional: where logs are exported over OTLP ([ADR-0049](../../docs/adr/0049-log-export.md)). HTTPS, or HTTP to this machine. Without it, nothing is exported |
| `OTEL_EXPORTER_OTLP_LOGS_HEADERS` or `OTEL_EXPORTER_OTLP_HEADERS` | `serve` | Optional: percent-encoded `name=value` pairs sent with each export, such as the backend's key |
| `OTEL_EXPORTER_OTLP_LOGS_COMPRESSION` or `OTEL_EXPORTER_OTLP_COMPRESSION` | `serve` | `none` (the default) or `gzip` |
| `OTEL_SERVICE_NAME` | `serve` | The service name the backend sees, `tetsu` by default. `OTEL_RESOURCE_ATTRIBUTES` cannot replace it |
| `OTEL_RESOURCE_ATTRIBUTES` | `serve` | Optional: percent-encoded `name=value` pairs describing this process, such as `deployment.environment.name=demo` |

Any of these can also live in a local `.env` ([Run locally](#run-locally)). The logs-only export names win over the shared ones, as in every OpenTelemetry exporter. A bad export setting is
a warning, and `serve` runs without export. The exporter itself also reads OpenTelemetry's tuning names, such as
`OTEL_EXPORTER_OTLP_TIMEOUT` and `OTEL_BLRP_SCHEDULE_DELAY`; their defaults are used unless they are set.

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

`serve` sends responses in brotli or gzip when the browser accepts them, and fonts as they are. The root page may
take at most 40 KB that way and its stylesheet 20 KB, which a test checks
([ADR-0039](../../docs/adr/0039-response-compression-and-page-budget.md)).
