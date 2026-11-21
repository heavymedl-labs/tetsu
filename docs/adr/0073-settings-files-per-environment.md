# 0073. Settings files per environment

- Status: Accepted
- Date: 2026-11-17
- Supersedes the production secrets rule of [ADR-0050](0050-local-settings-file.md)
- The environment winning over the file superseded, for the mail settings, by [ADR-0078](0078-mail-settings-from-one-place.md)

## Context

An operator on a hosted environment is made with `tetsu operator create`, run from a terminal against that
environment's database, since its invite link prints only to a terminal. `tetsu` reads `.env` for local settings
([ADR-0050](0050-local-settings-file.md)), which says a host's database address is typed at a prompt and never kept
in a file. That means copying the demo's address in by hand for each command, which no command does today.

Twelve-Factor's admin processes run a host's tasks as the app's own command, once, with the host's settings. The
settings are what change between hosts, and where a secret comes from should change without changing Tetsu, so no
command names a host or a secret store.

## Decision

- **`tetsu --env <name> <command>` reads `.env.<name>` in place of `.env`,** for any command, such as
  `cargo tetsu --env demo operator create "Ada Lovelace"`. A name is lowercase letters, digits, and hyphens,
  starting with a letter, so it cannot name a path or look like an option. The file must exist, since it was asked
  for, and is read as ADR-0050 reads `.env`.
- **The environment still wins** over the file, so a secret manager can supply any setting for one command, such as
  `op run -- cargo tetsu --env prod …`, and Tetsu needs no change to use one.
- **A demo's file may hold its database address in plain text.** The demo holds no one's real data. A host whose
  data matters takes its secrets from a secret manager through the environment, and its file holds none.
- **`.env.*` is never committed or built,** apart from `.env.example`. Git and the image build ignore it.

## Alternatives considered

- **A script per host that reads the address from the host's API.** Ties the tooling to Render.
- **The address at a prompt each time,** as ADR-0050 says. Safe, but a copy from Neon's dashboard for every
  command, for a demo whose data does not matter.
- **A secret manager now,** such as SOPS with age or 1Password. A tool to adopt for one secret on a demo. The
  environment rule leaves room for one later.
- **Refusing a file others can read,** as `ssh` does. Code for a risk the demo does not have; it belongs with the
  first host whose data matters.
- **Only `operator` commands with `--env`.** Every command reads settings the same way, and a demo's are no secret.

## Consequences

- Making or inviting an operator on the demo is one command, once `.env.demo` exists. The demo environment page says
  which settings it holds.
- Any command can reach the demo's database with `--env demo`, including `reset`, since the demo sets `TETSU_DEMO`.
- Nothing checks that the build matches what is deployed; run it from the commit the host runs.
- A production host needs a secret manager before its first operator, and the fresh review checks that its file
  holds no secret.

## Sources

- The Twelve-Factor App, [XII. Admin processes](https://12factor.net/admin-processes), read 2026-11-17.
- The Twelve-Factor App, [III. Config](https://12factor.net/config).
- Google Cloud, [Running database migrations with Cloud Run jobs](https://cloud.google.com/blog/topics/developers-practitioners/running-database-migrations-cloud-run-jobs),
  read 2026-11-17.
