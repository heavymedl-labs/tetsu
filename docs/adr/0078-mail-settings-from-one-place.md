# 0078. Mail settings from one place

- Status: Accepted
- Date: 2026-11-21
- Supersedes, for the mail settings only, [ADR-0073](0073-settings-files-per-environment.md)'s rule that the
  environment wins over the file for any setting

## Context

[ADR-0073](0073-settings-files-per-environment.md) lets the environment supply any setting over a settings file,
`.env` or `.env.<name>`, so a secret manager can supply one secret for one command, such as
`op run -- cargo tetsu --env prod …`. Each setting is looked up on its own: from the environment if it is set
there, and from the file if not.

For the mail settings ([ADR-0074](0074-operator-email-in-development.md)), that lets a password come from one
source and the server it goes to from another, and nothing notices. A fresh review of the local Mailpit work
([#149](https://github.com/heavymedl-labs/tetsu/issues/149)) showed the harm on 2026-11-21:
`scripts/mailpit.py` gives Tetsu a file whose host is the local Mailpit, and a person who exports a real
`TETSU_SMTP_PASSWORD` in the terminal where Tetsu runs sends that password to Mailpit. Reversed, a host exported
there receives Mailpit's login. No script can see another terminal's environment, so only Tetsu can refuse it.

## Decision

- **Tetsu takes the mail settings as a group:** all from the environment, or all from its settings file. Some from
  each is refused at start, naming one setting from each and never a value. The mail settings are the
  `TETSU_SMTP_*` and `TETSU_MAIL_*` ones Tetsu reads.
- **Every other setting keeps ADR-0073's rule,** and the environment wins over the file for it alone.
- **A secret manager supplies the whole mail group,** such as through `op run`'s env file listing every mail
  setting, rather than the password alone.

## Alternatives considered

- **Keep ADR-0073's rule for mail too,** and warn in the docs. Leaves a real password one export away from a local
  server, with nothing to stop it.
- **Refuse only a password from the environment beside a server from the file.** Narrower, but a server from the
  environment beside a file's login leaks the other way, so the rule would need every pairing listed.
- **Have `scripts/mailpit.py` check for exports.** It can only see its own terminal, not the one Tetsu runs in.

## Consequences

- A password can no longer reach a server another source named, locally or on a hosted environment.
- Supplying only the mail password from a secret manager stops working; the whole group comes from it instead.
- Hosts whose settings all come from the environment, such as the demo on Render, and files that hold all of
  them, such as a local `.env`, are unchanged.

## Sources

- [ADR-0073](0073-settings-files-per-environment.md) and [ADR-0050](0050-local-settings-file.md), on how settings
  are found.
- [1Password CLI: `op run`](https://developer.1password.com/docs/cli/reference/commands/run/), which loads every
  variable an env file lists, read 2026-11-21.
