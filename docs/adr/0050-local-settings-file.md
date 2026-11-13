# 0050. Local settings file

- Status: Proposed
- Date: 2026-11-13

## Context

`tetsu` reads its settings from the environment ([crates/tetsu/README.md](../../crates/tetsu/README.md#settings)).
On a host that is right: Render and CI set them, and the image holds none. On a laptop it means typing
`DATABASE_URL`, `DATABASE_TLS`, `TETSU_DEMO`, and `PORT` into every command, and no document says what they are for
a local run. Commands should run as written, with their settings already in place
([#124](https://github.com/heavymedl-labs/tetsu/issues/124)).

## Decision

- **`tetsu` reads a `.env` file in the folder it runs from, when there is one,** for every command. Each line is
  `NAME=value`, a `#` comment, or blank. Values are taken as written, trimmed like an environment variable: no
  quotes, escapes, `export`, or `${}` expansion.
- **The environment wins.** The file fills only variables the environment leaves unset, so a host's settings, CI's,
  and a variable typed before a command all still apply.
- **`.env` is never committed or shipped.** Git and the image build ignore it. `.env.example` holds the local
  defaults, for the test Postgres and a `tetsu_local` database, and a new clone copies it to `.env`.
- **A line that is not a setting stops the command,** naming the file and line number but never its text, since a
  value may be a password.
- **Production secrets stay out of it.** A command run against a host's database, such as seeding the demo, still
  takes the address at a prompt ([demo-environment.md](../operations/demo-environment.md)).
- **The reader is ours, about 30 lines in `settings.rs`,** next to the code that already reads every variable
  through one function.

## Alternatives considered

- **Cargo's `[env]` in `.cargo/config.toml`.** Already holds `TEST_DATABASE_URL`, and needs no code. But it applies
  only under `cargo run`, is committed, so it cannot hold a personal value such as an export key, and it reaches
  `cargo test` too.
- **The `dotenvy` crate.** Widely used, but quoting, escapes, and expansion are features no setting here needs, and
  each is a way for a value to read differently than written. It would need an ADR-0010 review for 30 lines.
- **A wrapper script that exports the settings.** Works for one command, but every other way of running `tetsu`
  misses it.
- **direnv.** Loads `.env` for any program, but every contributor installs and trusts it first.

## Consequences

- Locally, `cargo run -- serve` runs with no settings typed. The README says how to set up once.
- Integration tests run the binary from `crates/tetsu`, where no `.env` is, and set or remove each variable they
  test, so a developer's file cannot change a test.
- A `.env` left in a folder a command runs from on a server would apply. Images hold none, and Render runs from `/`.
- Revisit if a setting needs a value the format cannot hold, such as one with a line break.

## Sources

- The Twelve-Factor App, [III. Config](https://12factor.net/config): settings in the environment, apart from the
  code.
- [Cargo configuration, `[env]`](https://doc.rust-lang.org/cargo/reference/config.html#env), read 2026-11-13.
- [dotenvy](https://crates.io/crates/dotenvy), read 2026-11-13.
