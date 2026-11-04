# 0008. Benchmark harness

- Status: Accepted
- Date: 2026-11-03

## Context

[ADR-0007](0007-server-stack.md) chose the server stack from benchmarks run on
one laptop over loopback. Those numbers need repeating on Linux, on a
production Postgres version, and more than once, before the choice is trusted.
[ADR-0005](0005-testing-and-performance.md) will need the same machinery for
hot-path benchmarks once code exists. The repository holds no Rust code in this
phase, so the harness needs an explicit exception.

## Decision

- **Harness in `benchmarks/`.** A standalone Rust crate, outside any future
  product workspace, holds the benchmark code. It is the only Rust code allowed
  before the implementation phase opens.
- **One image per commit.** The harness, its servers, and the `oha` load
  generator build into one OCI image, pushed to GitHub Container Registry and
  tagged with the commit SHA. Every run measures that image.
- **On demand only.** A `workflow_dispatch` workflow runs chosen suites a
  chosen number of times, each run as its own job on a fresh standard GitHub
  runner, against a Postgres service container. It never runs on push or pull
  request.
- **Ratios and averages.** Within a run, every option in a suite runs on the
  same machine in a shuffled order. Reports lead with each option's ratio to a
  baseline, computed per run and then averaged, and show the mean, median,
  standard deviation, and 95% confidence interval across runs. Results are
  saved as a workflow artifact.

## Alternatives considered

- **Separate repository.** Keeps this one free of Rust, but splits the
  benchmarks from the decisions they support, and ADR-0005's suite will live
  here anyway.
- **Run on every pull request.** Slow, and noisy shared runners would make a
  per-change gate unreliable before there is code to gate.
- **Larger or self-hosted runners.** Steadier numbers, but they cost money or
  upkeep. Ratios and repeated runs make standard runners usable for now.
- **Absolute numbers only.** Shared runners vary too much between machines for
  absolute numbers to compare across runs.

## Consequences

Anyone can rerun the stack comparison with one click and get averaged results.
Standard runners have 2 vCPUs, and Postgres, the server, and the load
generator share them, so absolute numbers stay low and network latency is
still missing. Measuring across a real network waits for the
[first hosting environment](https://github.com/heavymedl-labs/tetsu/issues/18).
Runs use free Actions minutes and store images in the organization's package
registry, which should be pruned. The `benchmarks/` exception ends when the
implementation phase opens and the harness moves into the main workspace.

## Sources

- GitHub Docs, "Events that trigger workflows": `workflow_dispatch`.
- GitHub Docs, "Working with the Container registry".
- Brendan Gregg, *Systems Performance*, 2nd ed., Addison-Wesley, 2020, ch. 12:
  benchmarking, including active benchmarking and statistics.
- oha, github.com/hatoo/oha: HTTP load generator.
