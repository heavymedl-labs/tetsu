# Benchmarks

Repeatable benchmarks for the server stack in
[ADR-0007](../docs/adr/0007-server-stack.md), run as
[ADR-0008](../docs/adr/0008-benchmark-harness.md) describes. It is a member of the
workspace, but not product code, and Cargo skips it unless asked with
`-p tetsu-bench`. Findings and their reading are in
[Server stack research](../docs/research/server-stack.md).

## Run in CI

Run the **Benchmarks** workflow from the Actions tab and choose how many runs
and which suites. Each run gets a fresh runner and a Postgres 18 service. The
**Report** job's summary shows the combined results, and the
`benchmark-report` artifact holds `report.md`, `summary.json`, and each run's
raw records.

The image is tagged with a hash of this directory, `crates/`, and the workspace
files, so it is rebuilt only when those change.

## Suites

| Suite | Compares | Baseline |
| --- | --- | --- |
| `db` | SQLx, `tokio-postgres` (sequential and pipelined), Cornucopia (as generated, prepared, prepared and pipelined), Diesel async, on lookup, list, and booking transactions under row-level security | `sqlx` |
| `tpl` | Askama, Sailfish, Maud, and homegrown string building, rendering a 50-row schedule | `askama` |
| `http` | Axum, Axum with tracing, timeout, and request ID layers, Axum with a `to_vec` JSON response, actix-web, Salvo, and raw hyper | `axum` |
| `rt` | Tokio current-thread and multi-thread, monoio, and compio on one core; Linux only | `tokio-mt` |
| `e2e` | An Axum page that queries 50 classes and renders Askama, with each driver | `sqlx` |

## Reading results

Each option's speed is its throughput divided by the baseline's in the same
run, then averaged across runs. Shared runners differ from one another, so
compare speed ratios, not absolute numbers. A wide confidence interval or a CV
above about 10% means more runs are needed.

CPU per op is the client process's time in the `db` suite and the server's in
the `http`, `rt`, and `e2e` suites. Server cores busy shows what set the
throughput in those three. In `http` and `rt`, a value below the cores the
server was given means the load generator hit its limit first, so compare CPU
per op instead of speed. In `e2e`, the server also waits on Postgres, so a low
value does not implicate the load generator.

## Run locally

Docker is the simplest way, and matches CI:

```bash
docker build -f benchmarks/Dockerfile -t tetsu-bench .
docker run -d --name bench-pg --network host -e POSTGRES_HOST_AUTH_METHOD=trust postgres:18.6 \
  -c listen_addresses=localhost
docker run --rm --network host --security-opt seccomp=unconfined \
  -e ADMIN_URL=postgres://postgres@localhost:5432/postgres \
  -v "$PWD/benchmarks/results:/work/results" tetsu-bench
python3 benchmarks/aggregate.py benchmarks/results report.md summary.json
```

Set `SUITES`, `SCALE`, and `SECONDS_PER` to narrow or lengthen a run; `run.sh`
documents them.

## Changing queries

`cornucopia/` is generated from `queries/` and committed. After editing a
query or `sql/schema.sql`, seed a database and regenerate:

```bash
cargo install --locked cornucopia@1.0.1
cornucopia live postgres://app@localhost:5432/tetsu -q queries -d cornucopia
```
