# 0016. Crate layout and coverage by tier

- Status: Accepted
- Date: 2026-11-06

## Context

[#19](https://github.com/heavymedl-labs/tetsu/issues/19) adds the first real module, so later modules will copy
where it lives and how it is arranged. [ADR-0003](0003-architecture.md) makes each module a crate, but says
nothing about layout. [ADR-0005](0005-testing-and-performance.md) sets coverage by tier, while
[ADR-0015](0015-rust-checks.md) set one limit across the workspace, which lets a well-tested crate hide a weak
one. [#25](https://github.com/heavymedl-labs/tetsu/issues/25) needs to tell platform crates from vertical ones.

## Decision

- **Crates live flat in `crates/`.** The folder is `crates/<name>` and the package is `tetsu-<name>`, except the
  `tetsu` binary. There are no `platform/` or `verticals/` folders.
- **Each crate declares its kind and tier** in its `Cargo.toml`, and the check fails without them:

  ```toml
  [package.metadata.tetsu]
  kind = "platform" # platform, vertical, or root (the binary)
  tier = 2          # 1, 2, or 3
  ```

  A crate takes the tier of its riskiest code, so a module holding any Tier 1 code is Tier 1. #25 reads `kind`.
- **Coverage limits apply to each crate:** 100% of lines and regions at Tier 1, and 90% at Tiers 2 and 3. These
  replace ADR-0015's single limit. Branch coverage stays exception E3. A report that matches no crate fails the
  check, and the floor guard flags a changed `kind` or `tier`.
- **Every module has the same shape,** matching the module view in [ARCHITECTURE.md](../../ARCHITECTURE.md#module):

  ```text
  crates/<name>/
    Cargo.toml
    README.md     hot paths, as ADR-0005 requires
    src/
      lib.rs      public surface only: use cases, driven ports, domain events, Router
      domain.rs
      events.rs
      ports.rs
      use_cases/
      adapters/   HTTP, Postgres, and other technology, and nowhere else
    sql/
    migrations/
    templates/
    tests/
    benches/
  ```

  A module leaves out what it does not need, and a file may become a folder as it grows.

## Alternatives considered

- **`crates/platform/` and `crates/verticals/` folders.** Shows the boundary at a glance and makes a check
  simple, but moving a module means changing paths, and a folder enforces nothing. #25 can read `kind` instead.
- **A separate crate for each module's adapters.** [ADR-0007](0007-server-stack.md) already rejected this.
- **One coverage limit per tier set in a config file.** Lets crates be listed in one place, but a new crate could
  be forgotten. Declaring in the crate's own manifest fails loudly when it is missing.
- **Per-file limits.** More precise, but a small file makes the limit noisy.

## Consequences

Adding a crate takes two more lines, and a missing declaration fails the check. Each crate's coverage now stands
alone, so a small crate can fail on a few uncovered lines.

**Tradeoffs.** A crate's tier rests on its author's judgment, so the fresh review has to question it. Nothing yet
checks that a module has this shape. One Tier 1 file holds its whole crate to 100%, and splitting a module to avoid
that would need a superseding ADR, since ADR-0003 and ADR-0007 keep each module in one crate.

**Weakest evidence.** No module exists yet, so the shape is untested until #19 uses it. 100% of regions may prove too
strict in practice, because regions count some paths tests rarely reach. We have not compared this layout with other workspaces, and the case for a flat
`crates/` is from memory.

**Still open.** ADR-0005's "coverage never goes down" is not enforced: these limits are fixed floors, so coverage
can fall as far as the limit. This affects every tier. Branch coverage (E3) is also open, and must be settled
before the first Tier 1 code.

**Revisit** after #19 and after the first Tier 1 module, when the shape and the limits have met real code.

## Sources

- [ADR-0003](0003-architecture.md), [ADR-0005](0005-testing-and-performance.md), and [ADR-0015](0015-rust-checks.md).
- Cargo reference, "The `[package.metadata]` table": a place for tool configuration that Cargo ignores.
- Alistair Cockburn, "Hexagonal Architecture," 2005.
