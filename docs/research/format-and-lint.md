# Format and lint

An evaluation of formatters and linters for SQL, Python, and Bash, read and tried 2026-11-06, for
[ADR-0017](../adr/0017-format-and-lint.md). Each is cheap to replace and off the hot path, so
[ADR-0010](../adr/0010-evaluating-technology.md) steps 1, 4, and 7 apply, and we ran each here. This is
research, not a decision. Where it disagrees with an ADR, the ADR wins.

## Criteria

Set before looking at results. A tool must:

- be free and open source, and pin to a version from PyPI, so `.tools` holds it
- run on our files in seconds, so it fits the pre-commit hook
- exit non-zero on a finding, and read its settings from a file in the repository

## Project health

| Project | Use | License | Latest | Contributors, top share | Open issues | Advisories |
| --- | --- | --- | --- | --- | --- | --- |
| [Squawk](https://github.com/sbdchd/squawk) | Migration safety | Apache-2.0 | 2.71.0, October 2026 | 53, 79% | 47 | None published |
| [Ruff](https://github.com/astral-sh/ruff) | Python lint and format | MIT | 0.17.4, November 2026 | 100+, 28% | 2,259 | None published |
| [ShellCheck](https://github.com/koalaman/shellcheck) | Bash lint | GPL-3.0 | 0.11.0, August 2025 | 100+, 76% | 1,090 | None published |
| [shfmt](https://github.com/mvdan/sh) | Bash format | BSD-3-Clause | 3.14.1, September 2026 | 100+, 95% | 74 | None published |
| [SQLFluff](https://github.com/sqlfluff/sqlfluff) | SQL lint and format | MIT | 4.5.0, October 2026 | 100+, 41% | 345 | Three, all fixed: two high-severity parser issues with hostile queries, fixed in 4.1.0 and 4.2.0, and a 2023 config file issue |
| [uv](https://github.com/astral-sh/uv) | Builds `.tools` on a pinned Python | Apache-2.0 | 0.13.7, November 2026 | Not read | 3,049 | Not read |

ShellCheck's GPL covers the tool, not our scripts, because we only run it. Its last release is over a year
old, though it is still pushed to. Contributor counts cover at most the top 100 listed by GitHub.

## What we ran

| Tool | Our files | A planted problem |
| --- | --- | --- |
| Ruff, our rule set | 89 findings, 67 of them line length, which the formatter settles. The rest are fixed, or covered below | Not planted; real findings included a NaN check written as `x != x` |
| Ruff, every rule | 552 findings, mostly docstring and type annotation style | |
| ShellCheck | One warning, on sourcing nvm from a path it cannot follow | |
| shfmt | Formatting differences in 4 scripts | |
| Squawk, with `assume_in_transaction` | A plain `CREATE TABLE` passes | A blocking index on a new non-null column fails 4 rules |
| SQLFluff, Postgres dialect | 34 findings on 48 lines of benchmark SQL, in about a second. `sqlfluff fix` settled the layout ones, and the rest were unaliased expressions and `name` as a column | |

SQLFluff 4.x needs Python 3.10 or newer, which macOS does not ship, so uv now provides `.tools` with its own
Python.

Without `assume_in_transaction`, Squawk fails even a plain `CREATE TABLE`, because it assumes each file may
be rerun after failing partway. Our runner applies each file in a transaction unless the file opts out.

## Weakest evidence

Community practice was not reviewed beyond popularity, so "modern" rests on our knowledge, not a survey.
Squawk ran on one planted file, and SQLFluff on the benchmark SQL, since no module migration exists yet. uv's
own health was not read beyond its license and activity.
