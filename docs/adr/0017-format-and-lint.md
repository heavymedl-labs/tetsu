# 0017. Format and lint

- Status: Accepted
- Date: 2026-11-06

## Context

Rust has rustfmt and Clippy ([ADR-0015](0015-rust-checks.md)), but SQL, Python, and Bash had no formatter or
linter, and [ADR-0007](0007-server-stack.md) named Squawk for migrations without wiring it in. Every check ran
at push, including mutation testing, so a formatting slip cost minutes to find. The maintainer asked for
checks that run on commit, as in a typical TypeScript setup, using current community configuration. The
evidence is in [Format and lint research](../research/format-and-lint.md).

## Decision

- **Tools.** SQLFluff lints and formats SQL, Squawk checks migrations for locking and downtime risks, Ruff lints
  and formats Python, and ShellCheck and shfmt lint and format Bash.
  [scripts/install-tools.sh](../../scripts/install-tools.sh) pins each one in `.tools`.
- **uv builds `.tools` on a pinned Python 3.13.** SQLFluff needs Python 3.10 or newer, and macOS ships 3.9, so
  the tools no longer depend on the machine's Python. Our own scripts still run on the system `python3`.
- **Settings live in standard files.** `.sqlfluff` uses the Postgres dialect and every default rule, allowing only
  the column name `name`, which Postgres does not reserve. `ruff.toml` picks a strict rule set that includes security,
  complexity at 10 as in Rust, and modern syntax, and leaves out docstring and type annotation rules. `.config/squawk.toml`
  assumes each migration runs in a transaction, as our runner does. `.editorconfig` sets shell formatting.
- **Two stages.** The pre-commit hook runs `scripts/check.sh --fast`: formatters, linters, and markdownlint,
  in seconds. The pre-push hook and CI run everything. The floor guard runs only at push, because it reads
  `Floor-reason:` from commit messages, which do not exist yet at commit time.
- **The floor guard protects lint settings.** Removing a line from `clippy.toml`, `ruff.toml`,
  `.config/squawk.toml`, `.sqlfluff`, `.editorconfig`, the root `Cargo.toml`, or either hook needs a `Floor-reason:`.
- **Every language row has a format and lint column** in [CONSTRAINTS.md](../../CONSTRAINTS.md), so a new
  language, such as TypeScript for [#32](https://github.com/heavymedl-labs/tetsu/issues/32), must name both before
  it lands.
- **E2 narrows, and E4 records an existing gap.** Python complexity moves to Ruff. SQL had no checks before,
  and exception E4 records that its coverage, duplication, and complexity still have no good tool.

## Alternatives considered

- **A hook manager such as pre-commit, lefthook, or Husky.** Staged-file runs and parallel jobs, but one more
  tool, and our whole repository lints in seconds. Plain hooks calling one script keep local and CI the same.
- **Flake8, Black, and isort.** The older Python stack, now largely replaced by Ruff, which does all three.
- **Ruff's full rule set.** Strictest, but 552 findings, mostly style rules that would earn their own ignores.
- **Squawk alone for SQL.** It covers migration safety but not style, and queries outside migrations go
  unchecked.
- **Pinning SQLFluff 3.x, which runs on Python 3.9.** Avoids uv, but misses two high-severity parser fixes.
- **Running only changed files at commit.** The TypeScript norm, but it adds a tool, and the full run is fast.

## Consequences

Formatting and lint problems surface at commit, and contributors run `scripts/install-tools.sh` again to get
the new tools, which now also need uv. Formatting changed 11 Python files, 4 shell scripts, and the benchmark
SQL, with no change in behavior: the brand generator was refactored to pass Ruff and produces byte-identical
SVGs, and regenerating Cornucopia's code from the reformatted queries gives identical output, because it
collapses whitespace. Removing a
dependency from the root `Cargo.toml` now needs a `Floor-reason:`, because that file also holds lint levels.

**Tradeoffs.** `git commit --no-verify` skips the commit hook, and the push hook and CI still catch what it
missed. Each new migration needs lock and statement timeouts at its top to pass Squawk.
Squawk asks for `CREATE INDEX CONCURRENTLY`, but assumes every file runs in a transaction, where that
statement fails, so a file that opts out of the transaction may need a `squawk-ignore` comment. The floor
guard flags that comment, so it needs a `Floor-reason:`.

**Weakest evidence.** "Current community configuration" rests on our knowledge, not a survey. Squawk has run on
one planted file, and SQLFluff on 48 lines of benchmark SQL, so neither has met a real migration yet.

**Cost to replace.** Low. Each tool is a line in each script and a small settings file.

**Revisit** when a language joins, or if the commit stage grows past about ten seconds.

## Sources

- [Format and lint research](../research/format-and-lint.md): criteria, project health, and what we ran,
  2026-11-06.
- [Squawk rules](https://squawkhq.com/docs/rules), [Ruff rules](https://docs.astral.sh/ruff/rules/),
  [ShellCheck wiki](https://www.shellcheck.net/wiki/), and [EditorConfig](https://editorconfig.org/).
