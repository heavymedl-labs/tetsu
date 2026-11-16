# 0063. CI minutes per run

- Status: Proposed
- Date: 2026-11-16

## Context

[ADR-0023](0023-ci-on-demand.md) made CI run on demand, in two jobs: the checks in one, and the image, the
benchmark harness compile, the link check, and workflow lint in the other. That stopped the drain from every push,
but each run still costs more than it needs to. GitHub's billing API shows November at 1,483 of the 2,000 minutes on
2026-11-16. Read from each job's times, 713 of them went to 44 manual runs, about 16 minutes each, and 658 to the
pushes before ADR-0023.

A typical run billed about 19 minutes on 2026-11-16: about 11 for the checks, 7 for the image job, and 1 to publish.
Two parts of the image job repeat the same work from cold on every run:

- **The image's release build,** about 5 minutes. The Dockerfile copied the whole repository before building, so no
  layer survived a change, and a runner keeps nothing between runs. A cold release build took 101 s on two CPUs
  locally, and 31 s with the dependencies already built.
- **The benchmark harness check,** about 2 minutes. The harness builds about twenty crates nothing else uses, from
  cold, on every run, though it rarely changes.

## Decision

- **The image's Rust dependencies are a layer of their own.** The Dockerfile builds them from the manifests alone,
  over stub sources, so the layer lasts until a manifest or the lockfile changes. Tetsu's own crates are cleaned
  and built from their real sources after it. A crate added without its manifest in the Dockerfile fails the build.
- **CI keeps that layer between runs.** The image job imports it from the Actions cache, through a builder of
  Docker's own BuildKit, and exports a new one only when the cache key changes. BuildKit's image is pinned by
  version and digest, as every image here is.
- **The benchmark harness is checked only when something it builds from changed** since the base the run checks:
  `benchmarks/`, a manifest, the lockfile, or the toolchain. It runs in the checks job, which knows that base, and
  in a target folder outside the cache, which would grow by about 1 GB. This supersedes the part of ADR-0023 that
  puts the harness compile in the image job, on acceptance.

## Alternatives considered

- **`cargo-chef`, the common tool for this layer.** It does what the stub layer does with a recipe file, but it is
  a new tool for a few lines of Dockerfile, and those lines fail loudly when they fall behind.
- **Docker's own GitHub Actions for BuildKit and its cache.** `docker/setup-buildx-action` and the `gha` cache
  backend are the documented route, but they add two third-party actions where one `docker buildx create` and the
  cache action CI already uses do the same.
- **A cache mount in the Dockerfile.** A cache mount lives in one builder's storage, which a runner throws away,
  and BuildKit does not export it.
- **Keeping the harness check on every run, with its own cache.** About 1 GB more of cache, restored on every run,
  to save a check that rarely has anything to find.
- **A larger runner.** Faster, but billed at a multiple of the standard rate.

## Consequences

- A typical run should bill about 12 to 13 minutes rather than 19, measured on the first runs after this lands.
- The image job pulls BuildKit's image each run, about 10 to 20 s, and the Actions cache holds about 1 GB more,
  next to the tools cache of about 2.3 GB, within the 10 GB GitHub allows.
- Adding a crate means adding its manifest to the Dockerfile's dependency layer.
- A change that breaks the benchmark harness without touching what it builds from is not caught by CI. The
  Benchmarks workflow still builds it before every benchmark run.

**Weakest evidence.** The savings are measured on this machine, on two CPUs where it mattered; the runner's own
numbers come with the first runs.

**Revisit** if a release build with the dependencies cached still takes most of the image job, if the cache is
evicted often enough that runs rebuild from cold, or if BuildKit's image stops being maintained.

## Sources

- GitHub's billing usage API, and each job's start and end times from the Actions API, for heavymedl-labs, read
  2026-11-16.
- [Docker: optimize builds with cache](https://docs.docker.com/build/cache/optimize/): order layers from least to
  most often changed, and keep dependency installs in a layer of their own.
- [Docker: cache storage backends](https://docs.docker.com/build/cache/backends/): the local backend exports a
  build's cache to a directory, and needs a builder other than the default `docker` driver.
- [GitHub: dependency caching](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching):
  10 GB per repository, and caches unused for seven days are removed.
- moby/buildkit on GitHub, read 2026-11-16: Apache-2.0, actively maintained, about 10,400 stars. v0.34.2,
  released 2026-11-03, patches every advisory published so far; v0.35.0 was five days old, so the older patched
  release is pinned.
