# 0022. Benchmark image tag

- Status: Proposed
- Date: 2026-11-08

## Context

[ADR-0008](0008-benchmark-harness.md) builds the benchmark harness into one image per commit, "tagged with the
commit SHA". Since commit 22bf5dc, `.github/workflows/benchmarks.yml` tags it instead with a hash of what the
image builds from, `benchmarks/`, `crates/`, and the workspace files, and reuses an image whose tag already
exists. No ADR recorded the change, so the accepted rule and the workflow disagree
([#57](https://github.com/heavymedl-labs/tetsu/issues/57)). The maintainer chose to record the change rather
than revert it.

## Decision

- **The benchmark image is tagged with a hash of the files it builds from,** as git sees them at the commit
  under test, and a run reuses an image with that tag instead of building it again.
- **Results stay traceable:** the report's title names the image tag, and the workflow run records the commit.
- **On acceptance,** ADR-0008's "tagged with the commit SHA" is marked superseded by this ADR.

## Alternatives considered

- **Tag by commit SHA, as ADR-0008 says.** Simple to trace, but every commit rebuilds the image, including
  commits that change only docs, which costs CI minutes and gives identical images different names.
- **Tag by both.** Reusable and readable at a glance, but two tags per image to keep, for no gain while the
  workflow run already records the commit.

## Consequences

- Docs-only commits reuse the last image, so benchmark runs start sooner.
- A file the build reads that sits outside the hashed paths would not change the tag. The hash must grow with
  the build, such as when `benchmarks/Dockerfile` copies a new top-level file.
- The cost of replacing this is one workflow step.

**Weakest evidence.** Rebuild savings were not measured.

**Revisit** if a stale image is ever measured because the hash missed a file.

## Sources

- [git-hash-object](https://git-scm.com/docs/git-hash-object) and [git-ls-tree](https://git-scm.com/docs/git-ls-tree):
  the tag is the hash of the listed trees, so it changes exactly when their content does.
- [ADR-0008](0008-benchmark-harness.md), the rule this supersedes.
