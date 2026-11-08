# 0022. Benchmark image tag

- Status: Proposed
- Date: 2026-11-08

## Context

[ADR-0008](0008-benchmark-harness.md) builds the benchmark harness into one image per commit, "tagged with the
commit SHA". The workflow it shipped with, in commit 4cfa0c3, already tagged the image with a hash of
`benchmarks/` instead and reused an image whose tag existed. Commit 22bf5dc added `crates/` and the workspace
files to the hash. The accepted rule and the workflow have disagreed from the start
([#57](https://github.com/heavymedl-labs/tetsu/issues/57)). The maintainer chose to record the workflow's rule
rather than change it.

## Decision

- **The benchmark image is tagged with a hash of the files it builds from,** as git sees them at the commit
  under test, and a run reuses an image with that tag instead of building it again.
- **Results stay traceable:** the report's title names the image tag, and the workflow run records the commit.
- **On acceptance,** ADR-0008's "tagged with the commit SHA" is marked superseded by this ADR.

## Alternatives considered

- **Tag by commit SHA, as ADR-0008 says.** Simple to trace, but every commit rebuilds the image, including
  commits that change nothing it builds from, which costs CI minutes and gives identical images different names.
- **Tag by both.** Reusable and readable at a glance, but two tags per image to keep, for no gain while the
  workflow run already records the commit.

## Consequences

- A commit that changes none of the hashed files reuses the last image, so its benchmark run starts sooner.
  Docs inside those folders, such as a crate's README, still change the hash and force a rebuild.
- A file the build reads that sits outside the hashed paths would not change the tag. The hash must grow with
  the build, such as when `benchmarks/Dockerfile` copies a new top-level file.
- The cost of replacing this is one workflow step.

**Weakest evidence.** Rebuild savings were not measured.

**Revisit** if a stale image is ever measured because the hash missed a file.

## Sources

- [git-hash-object](https://git-scm.com/docs/git-hash-object) and [git-ls-tree](https://git-scm.com/docs/git-ls-tree):
  the tag is the hash of the listed trees, so it changes exactly when their content does.
- [ADR-0008](0008-benchmark-harness.md), the rule this supersedes.
