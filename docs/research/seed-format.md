# Seed file format

An evaluation of the format and crates for demo business files, read and tried 2026-11-07, for
[ADR-0018](../adr/0018-demo-businesses-as-data.md). Seeding runs once per change, far from any hot path, and
is cheap to replace, so [ADR-0010](../adr/0010-evaluating-technology.md) steps 1, 4, and 7 apply. We also ran
one small failure test. This is research, not a decision. Where it disagrees with an ADR, the ADR wins.

## Criteria

Set before looking at results. The format must:

- be easy for a person to read and edit by hand, with comments
- reject unknown fields, duplicate keys, and wrong types, and say where
- parse into plain Rust types, so the file format stays separate from domain types
- add no crate that ADR-0011's revisit triggers would already flag

## Options

| Option | Result |
| --- | --- |
| TOML with `toml` and `serde` | **Pick.** Comments, a strict spec, and Cargo uses it, so contributors know it |
| JSON with `serde_json` | No new crate, since [ADR-0007](../adr/0007-server-stack.md) chose it. No comments, and noisy to edit by hand |
| YAML with `serde_yaml` | Archived by its author in March 2024. YAML's implicit typing also turns some values into the wrong type |
| TOML with `basic-toml` | Fewer dependencies, but archived in March 2025 |
| Our own parser | No dependency, but we would own escaping, errors, and edge cases for no gain |

## Project health

| Crate | License | Latest | Last push | Contributors, top share | Open issues | Advisories | Minimum Rust |
| --- | --- | --- | --- | --- | --- | --- | --- |
| [serde](https://github.com/serde-rs/serde) | MIT OR Apache-2.0 | 1.0.229, July 2026 | 2026-10-16 | 100+, 64% | 414 | None in RustSec | 1.56 |
| [toml](https://github.com/toml-rs/toml) | MIT OR Apache-2.0 | 1.2.1, 2026-11-06 | 2026-11-06 | 100+, 65% | 68 | None in RustSec | 1.85 |

Recent downloads on crates.io are about 413M for `serde` and 303M for `toml`. Each leans on one maintainer.
Contributor counts cover only the top 100 that GitHub lists. Open issues include pull requests. Both crates are
already in `Cargo.lock`, through the benchmark harness, and `serde` arrives with `serde_json` anyway.

## What we ran

A scratch crate parsed a sample file with `toml` 1.2.1 and `serde` 1.0.229, with `deny_unknown_fields` on
every struct. It accepted the valid file and rejected each of these with its line and column:

- an unknown field at the top level and inside a nested table
- a duplicate key
- a string where a number belongs
- a missing required field

Not tested: very large files, and what an error message shows when a file holds a secret by mistake.

## Weighing it

TOML costs one crate we already build and gives hand-editable files with comments. JSON would add nothing but
reads worse, and demo files are edited by hand. The weakest evidence is the health table, read once, and both
crates depending on one person each. Replacing the format means rewriting the file types in `crates/tetsu` and
converting the demo files, which is a small job while there are few demos.
