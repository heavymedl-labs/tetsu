# 0036. Build front-end assets where they are used

- Status: Accepted
- Date: 2026-11-10
- Supersedes committing the build in [ADR-0034](0034-front-end-build.md)

## Context

[ADR-0034](0034-front-end-build.md) committed Vite's output, `crates/tetsu/assets/build/`, so Cargo and the image
need no Node, and a push check compared it with a clean build. Every styling change then swapped a hashed file in
git and needed a rebuild by hand, and the comparison raised whether macOS and Linux builds match byte for byte. The
maintainer prefers generated files out of git, as is usual where a bundler serves a backend.

## Decision

- **The build is made where it is used, never committed.** `crates/tetsu/assets/build/` is in `.gitignore` and
  `.dockerignore`.
- **`scripts/check.sh` builds it first,** before anything compiles the crate, in both modes. The clean-build
  comparison goes, since there is nothing committed to compare.
- **The image builds it in a Node stage,** pinned to `.nvmrc`'s release by digest, installing without scripts, and
  the Rust stage copies the output in. The final image still has no Node.
- **A fresh clone runs `npm ci --ignore-scripts --include=dev` and `npm run build` before `cargo`.** `build.rs` stops
  with that instruction when the build is missing.

The rest of ADR-0034 stands: Vite, the manifest Rust embeds at compile time, hashed names, and the npm checks.

## Alternatives considered

- **Keep committing it.** Cargo alone builds a fresh clone, but generated files churn in git and need a rebuild by
  hand.
- **`build.rs` runs npm itself.** One command builds everything, but every Rust build would depend on Node and the
  network, hidden inside Cargo.

## Consequences

- Every Rust build needs a front-end build first; CI's checks job gets it from `check.sh`, and the image from its
  Node stage.
- The Docker build pulls a Node image and the npm packages, so it needs the network, as it does for crates.
- Removing the clean-build comparison and its test changes protected files, so it needs a `Floor-reason:`.

**Revisit** if needing a front-end build before `cargo` gets in the way more than the churn did.

## Sources

- [Front-end build research](../research/front-end-build.md), 2026-11-10.
- Vite: [backend integration](https://vite.dev/guide/backend-integration).
