# Rust checks

An evaluation of the tools for the Rust row in [CONSTRAINTS.md](../../CONSTRAINTS.md), read and tried
2026-11-06. Each is cheap to replace and off the hot path, so [ADR-0010](../adr/0010-evaluating-technology.md)
steps 1, 4, and 7 apply, and we also ran each one here. This is research, not a decision. Where it
disagrees with an ADR, the ADR wins.

## Criteria

Set before looking at results. A tool must:

- run the same way locally and in CI, from one script, at a pinned version
- be free and open source, and need no account or service
- exit non-zero on a violation, so it can fail a check
- check changed code quickly enough to run before each push

## Options

| Rule | Pick | Considered |
| --- | --- | --- |
| Tests catch mistakes | `cargo-mutants` | No real alternative found |
| Coverage | `cargo-llvm-cov` | `cargo-tarpaulin`, not tried. ADR-0005 already leads with `cargo-llvm-cov` |
| Duplication | jscpd | `cargo-dupes` and others not evaluated |
| Complexity | lizard and Clippy | Clippy's `cognitive_complexity`, disputed in [its own issue](https://github.com/rust-lang/rust-clippy/issues/3793); `rust-code-analysis`, a library with no limit flag |
| Image base | distroless `cc` | `debian:bookworm-slim`, larger and with a shell |

## Project health

| Project | License | Latest | Last push | Contributors, top share | Open issues | Advisories |
| --- | --- | --- | --- | --- | --- | --- |
| [cargo-mutants](https://github.com/sourcefrog/cargo-mutants) | MIT | 27.1.0, June 2026 | 2026-10-19 | 32, 95% | 113 | None published |
| [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov) | Apache-2.0 on GitHub | 0.9.1, September 2026 | 2026-11-01 | 49, 80% | 67 | None published |
| [jscpd](https://github.com/kucherenko/jscpd) | MIT | 5.4.2 | 2026-11-06 | 71, 78% | 52 | None published |
| [lizard](https://github.com/terryyin/lizard) | MIT on PyPI; GitHub cannot detect it | 1.25.0 | 2026-11-06 | 68, 79% | 33 | None published |

Each leans on one person. Contributor counts cover at most the top 100 listed by GitHub. Recent downloads
are about 1.9M for `cargo-llvm-cov` and 416K for `cargo-mutants`. Minimum Rust versions were not read,
beyond each installing on our 1.99.1.

## What we ran

| Tool | Clean code | A planted violation |
| --- | --- | --- |
| Clippy with `clippy.toml` | Passes | Flags `axum::Json` once and `Row::get` 14 times in the benchmark harness, which uses both |
| `cargo llvm-cov --fail-under-lines 90 --fail-under-regions 90` | Passes at 100% | Not planted; passes 100 as a limit too |
| lizard, `-C 10 -L 50 -a 5` | Exits 0 | A function of cyclomatic complexity 11 exits 1 |
| jscpd, `--threshold 0` | Exits 0 | A copied 8-line function exits 1 |
| `cargo mutants --in-diff` | 5 mutants in 8 seconds | Found a surviving mutant in our first draft. It was an equivalent one, so we removed the function |

**Branch coverage.** `cargo-llvm-cov` measures branches only on nightly Rust, per its README, and ADR-0005
asks for them. Stable measures lines and regions. Regions are finer than lines, but they are not branches,
and we did not measure how far they differ.

## Weakest evidence

All of this ran on about 30 lines of code, so nothing here shows how these tools behave at size. Mutation
testing will get slower, and the limits are starting values rather than tuned ones. Community practice was
not reviewed. lizard counts decision points from text, not from the compiler.
